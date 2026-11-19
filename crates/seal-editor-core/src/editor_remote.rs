//! Local HTTP and WebSocket control of the running editor (opt-in from Preferences).

use crate::command::CommandArgs;
use crate::session::EditorSession;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::io::ErrorKind;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const DEFAULT_PORT: u16 = 47810;

const WEB_PANEL: &str = include_str!("remote_web_panel.html");

struct HttpWork {
    path: String,
    method: String,
    body: String,
    token: String,
    reply: Sender<(u16, Vec<u8>, &'static str)>,
}

struct WsClient {
    tx: Sender<String>,
}

/// Running remote listener.
pub struct RemoteControl {
    pub port: u16,
    pub ws_port: u16,
    clients: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    http_rx: Receiver<HttpWork>,
    ws_out: Arc<Mutex<Vec<WsClient>>>,
}

impl Drop for RemoteControl {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn port(session: &EditorSession) -> u16 {
    session
        .foundation
        .settings
        .get("editor.remote.port")
        .and_then(|p| p.trim().parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

pub fn token(session: &EditorSession) -> String {
    session
        .foundation
        .settings
        .get("editor.remote.token")
        .filter(|t| !t.trim().is_empty())
        .map(|t| t.to_string())
        .unwrap_or_else(|| "seal-remote".into())
}

pub fn enabled(session: &EditorSession) -> bool {
    session.foundation.settings.get("editor.remote.enabled") == Some("true")
}

impl RemoteControl {
    pub fn start(port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let ws_listener = TcpListener::bind(("127.0.0.1", port.saturating_add(1)))?;
        ws_listener.set_nonblocking(true)?;
        let ws_port = ws_listener.local_addr()?.port();
        let (http_tx, http_rx) = channel();
        let clients = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let ws_out: Arc<Mutex<Vec<WsClient>>> = Arc::new(Mutex::new(Vec::new()));
        let (c, s, w) = (clients.clone(), stop.clone(), ws_out.clone());
        let s_http = stop.clone();
        thread::Builder::new()
            .name("seal-remote-listen".into())
            .spawn(move || accept_loop(listener, http_tx, s_http))?;
        thread::Builder::new()
            .name("seal-remote-ws".into())
            .spawn(move || ws_accept_loop(ws_listener, w, c, s))?;
        Ok(Self {
            port,
            ws_port,
            clients,
            stop,
            http_rx,
            ws_out,
        })
    }

    pub fn clients(&self) -> usize {
        self.clients.load(Ordering::Relaxed)
    }
}

fn accept_loop(listener: TcpListener, http_tx: Sender<HttpWork>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(8)));
                let http_tx = http_tx.clone();
                let _ = thread::Builder::new()
                    .name("seal-remote-client".into())
                    .spawn(move || handle_client(stream, http_tx));
            }
            Err(_) => thread::sleep(Duration::from_millis(40)),
        }
    }
}

fn ws_accept_loop(
    listener: TcpListener,
    ws_out: Arc<Mutex<Vec<WsClient>>>,
    clients: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let (event_tx, event_rx) = channel();
                if let Ok(mut guard) = ws_out.lock() {
                    guard.push(WsClient { tx: event_tx });
                }
                let c = clients.clone();
                let s = stop.clone();
                let _ = thread::Builder::new()
                    .name("seal-remote-ws-client".into())
                    .spawn(move || ws_session(stream, event_rx, c, s));
            }
            Err(_) => thread::sleep(Duration::from_millis(40)),
        }
    }
}

fn handle_client(stream: TcpStream, http_tx: Sender<HttpWork>) {
    let Ok(parsed) = read_request(&stream) else {
        return;
    };
    let (reply_tx, reply_rx) = channel();
    let work = HttpWork {
        path: parsed.path,
        method: parsed.method,
        body: parsed.body,
        token: parsed.token,
        reply: reply_tx,
    };
    if http_tx.send(work).is_ok() {
        if let Ok(resp) = reply_rx.recv_timeout(Duration::from_secs(10)) {
            let _ = write_http_response(stream, resp.0, resp.1, resp.2);
        }
    }
}

struct ParsedHead {
    method: String,
    path: String,
    body: String,
    token: String,
}

fn read_request(stream: &TcpStream) -> std::io::Result<ParsedHead> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let raw_path = parts.next().unwrap_or("/").to_string();
    let (path, mut token) = split_token_query(&raw_path);
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.trim().is_empty() {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("content-length:") {
            content_length = lower.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
        }
        if lower.starts_with("authorization: bearer ") {
            token = line[22..].trim().to_string();
        }
    }
    let body = read_body(&mut reader, content_length)?;
    Ok(ParsedHead {
        method,
        path,
        body,
        token,
    })
}

fn read_body(reader: &mut BufReader<TcpStream>, len: usize) -> std::io::Result<String> {
    if len == 0 {
        return Ok(String::new());
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn split_token_query(raw: &str) -> (String, String) {
    let (path, query) = raw.split_once('?').map_or((raw, ""), |(p, q)| (p, q));
    let mut token = String::new();
    for pair in query.split('&') {
        if let Some(v) = pair.strip_prefix("token=") {
            token = v.to_string();
        }
    }
    (path.to_string(), token)
}

fn write_http_response(
    mut stream: TcpStream,
    code: u16,
    body: Vec<u8>,
    content_type: &'static str,
) -> std::io::Result<()> {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(&body)?;
    stream.flush()
}

fn ws_read_timed_out(err: &tungstenite::Error) -> bool {
    matches!(
        err,
        tungstenite::Error::Io(io)
            if matches!(io.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
    )
}

fn ws_session(stream: TcpStream, events: Receiver<String>, clients: Arc<AtomicUsize>, stop: Arc<AtomicBool>) {
    clients.fetch_add(1, Ordering::Relaxed);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    if let Ok(mut ws) = tungstenite::accept(stream) {
        let hello = json!({"type":"hello","text":"connected"}).to_string();
        let _ = ws.send(tungstenite::Message::Text(hello.into()));
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            while let Ok(text) = events.try_recv() {
                if ws.send(tungstenite::Message::Text(text.into())).is_err() {
                    clients.fetch_sub(1, Ordering::Relaxed);
                    return;
                }
            }
            match ws.read() {
                Ok(tungstenite::Message::Close(_)) => break,
                Ok(tungstenite::Message::Ping(payload)) => {
                    let _ = ws.send(tungstenite::Message::Pong(payload));
                }
                Err(e) if ws_read_timed_out(&e) => {}
                Err(_) => break,
                _ => {}
            }
        }
    }
    clients.fetch_sub(1, Ordering::Relaxed);
}

pub fn poll(session: &mut EditorSession) {
    if !enabled(session) {
        session.remote_control.take();
        session.remote_error = None;
        session.remote_ws_queue.clear();
        return;
    }
    if session.remote_control.is_none() && session.remote_error.is_none() {
        match RemoteControl::start(port(session)) {
            Ok(ctrl) => session.remote_control = Some(ctrl),
            Err(e) => session.remote_error = Some(format!("port {}: {e}", port(session))),
        }
    }
    let works: Vec<HttpWork> = session
        .remote_control
        .as_ref()
        .map(|c| c.http_rx.try_iter().collect())
        .unwrap_or_default();
    for work in works {
        answer_http(session, work);
    }
    // Clients hear about every selection change, from the UI or from remote commands.
    let now = (session.foundation.selection.clone(), session.models.selection.clone());
    if session.remote_sent_selection.as_ref() != Some(&now) {
        let first = session.remote_sent_selection.is_none();
        session.remote_sent_selection = Some(now);
        if !first {
            push_selection_event(session);
        }
    }
    if session.remote_ws_queue.is_empty() {
        return;
    }
    let Some(ctrl) = session.remote_control.as_ref() else {
        return;
    };
    let batch: Vec<String> = session.remote_ws_queue.drain(..).collect();
    if let Ok(mut clients) = ctrl.ws_out.lock() {
        clients.retain(|client| batch.iter().all(|msg| client.tx.send(msg.clone()).is_ok()));
    }
}

fn answer_http(session: &mut EditorSession, work: HttpWork) {
    let expect = token(session);
    if work.token != expect {
        let _ = work.reply.send((
            401,
            br#"{"error":"missing or wrong token"}"#.to_vec(),
            "application/json",
        ));
        return;
    }
    let project = session.project.as_ref().map(|p| p.root.clone());
    let response = match (work.method.as_str(), work.path.as_str()) {
        ("GET", "/") | ("GET", "/panel") => (
            200,
            WEB_PANEL.as_bytes().to_vec(),
            "text/html; charset=utf-8",
        ),
        ("GET", path) if path.starts_with("/panels/") => serve_user_panel(path, project.as_deref()),
        ("GET", "/api/selection") => (
            200,
            serde_json::to_vec(&selection_json(session)).unwrap_or_default(),
            "application/json",
        ),
        ("GET", "/api/properties") => (
            200,
            serde_json::to_vec(&properties_json(session)).unwrap_or_default(),
            "application/json",
        ),
        ("GET", "/api/screenshot") => screenshot_response(session),
        ("POST", "/api/command") => run_remote_command(session, &work.body),
        _ => (404, br#"{"error":"not found"}"#.to_vec(), "application/json"),
    };
    let _ = work.reply.send(response);
}

fn serve_user_panel(path: &str, root: Option<&Path>) -> (u16, Vec<u8>, &'static str) {
    let Some(root) = root else {
        return (404, br#"{"error":"no project open"}"#.to_vec(), "application/json");
    };
    let rel = path.trim_start_matches("/panels/");
    let full = safe_join(root.join("web_panels"), rel);
    let Some(full) = full else {
        return (404, br#"{"error":"bad path"}"#.to_vec(), "application/json");
    };
    match std::fs::read(&full) {
        Ok(bytes) => {
            let ty = if rel.ends_with(".js") {
                "application/javascript"
            } else if rel.ends_with(".css") {
                "text/css"
            } else {
                "text/html; charset=utf-8"
            };
            (200, bytes, ty)
        }
        Err(_) => (404, br#"{"error":"missing panel"}"#.to_vec(), "application/json"),
    }
}

fn safe_join(base: PathBuf, rel: &str) -> Option<PathBuf> {
    let mut out = base;
    for part in Path::new(rel).components() {
        match part {
            Component::Normal(name) => out.push(name),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            Component::CurDir => {}
        }
    }
    Some(out)
}

fn selection_json(session: &EditorSession) -> Value {
    json!({
        "ids": session.foundation.selection,
        "primary": session.models.selection,
        "playing": session.world.mode != seal_world::PlayMode::Editing,
    })
}

fn properties_json(session: &EditorSession) -> Value {
    let id = session
        .models
        .selection
        .as_deref()
        .or_else(|| session.foundation.selection.first().map(String::as_str));
    let Some(id) = id else {
        return json!({"object": null, "fields": []});
    };
    let Some(node) = session.models.nodes.iter().find(|n| n.id == id) else {
        return json!({"object": id, "fields": []});
    };
    let fields: Vec<Value> = node
        .fields
        .iter()
        .map(|(k, v)| json!({"name": k, "value": v}))
        .collect();
    json!({"object": id, "fields": fields})
}

fn screenshot_response(session: &mut EditorSession) -> (u16, Vec<u8>, &'static str) {
    // Each request asks the window for a fresh frame; the reply is the latest one captured.
    session.remote_screenshot_wanted = true;
    if let Some(png) = &session.remote_screenshot {
        return (200, png.clone(), "image/png");
    }
    (503, br#"{"error":"no frame captured yet, ask again"}"#.to_vec(), "application/json")
}

/// Store a window frame (RGBA8) as the PNG the remote screenshot endpoint returns.
pub fn set_remote_frame(session: &mut EditorSession, rgba: &[u8], width: u32, height: u32) {
    if width == 0 || height == 0 || rgba.len() != (width * height * 4) as usize {
        return;
    }
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let Ok(mut writer) = encoder.write_header() else { return };
        if writer.write_image_data(rgba).is_err() {
            return;
        }
    }
    session.remote_screenshot = Some(png_bytes);
    session.remote_screenshot_wanted = false;
}


fn run_remote_command(session: &mut EditorSession, body: &str) -> (u16, Vec<u8>, &'static str) {
    let value: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            return (
                400,
                format!(r#"{{"error":"{e}"}}"#).into_bytes(),
                "application/json",
            );
        }
    };
    let Some(id) = value.get("id").and_then(Value::as_str) else {
        return (
            400,
            br#"{"error":"missing id"}"#.to_vec(),
            "application/json",
        );
    };
    let args_json = value.get("args").map_or_else(|| "{}".to_string(), Value::to_string);
    let args = match CommandArgs::from_json_object(&args_json) {
        Ok(a) => a,
        Err(e) => {
            return (
                400,
                format!(r#"{{"error":"{e}"}}"#).into_bytes(),
                "application/json",
            );
        }
    };
    match session.dispatch(id, &args) {
        Ok(()) => {
            session
                .remote_ws_queue
                .push(json!({"type":"command","id": id, "ok": true}).to_string());
            (200, br#"{"ok":true}"#.to_vec(), "application/json")
        }
        Err(e) => (
            400,
            format!(r#"{{"error":"{e}"}}"#).into_bytes(),
            "application/json",
        ),
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::{EditorSession, Layout};
    use std::time::Instant;

    const NETWORK_DEADLINE: Duration = Duration::from_secs(10);

    fn session() -> EditorSession {
        let mut session = EditorSession::new(Layout::default());
        session
            .foundation
            .settings
            .set_user("editor.remote.token", "tok")
            .unwrap();
        session
    }

    fn client_read_timed_out(err: &tungstenite::Error) -> bool {
        ws_read_timed_out(err)
    }

    #[test]
    fn refuses_without_token() {
        let mut session = session();
        let (reply_tx, reply_rx) = channel();
        answer_http(
            &mut session,
            HttpWork {
                path: "/api/selection".into(),
                method: "GET".into(),
                body: String::new(),
                token: String::new(),
                reply: reply_tx,
            },
        );
        let (code, _, _) = reply_rx.recv().unwrap();
        assert_eq!(code, 401);
    }

    #[test]
    fn runs_command_with_token() {
        let mut session = session();
        let (reply_tx, reply_rx) = channel();
        answer_http(
            &mut session,
            HttpWork {
                path: "/api/command".into(),
                method: "POST".into(),
                body: r#"{"id":"viewport.shade","args":{"mode":"lit"}}"#.into(),
                token: "tok".into(),
                reply: reply_tx,
            },
        );
        let (code, body, ty) = reply_rx.recv().unwrap();
        assert_eq!(
            code, 200,
            "expected HTTP 200, got {code}, body={:?}, type={ty}",
            String::from_utf8_lossy(&body)
        );
    }

    #[test]
    fn websocket_gets_an_event() {
        let deadline = Instant::now() + NETWORK_DEADLINE;
        let mut session = session();
        session
            .foundation
            .settings
            .set_user("editor.remote.enabled", "true")
            .unwrap();
        session
            .foundation
            .settings
            .set_user("editor.remote.port", "0")
            .unwrap();
        poll(&mut session);
        let ws_port = session.remote_control.as_ref().unwrap().ws_port;
        let (connect_tx, connect_rx) = channel();
        thread::Builder::new()
            .name("seal-remote-test-connect".into())
            .spawn(move || {
                let url = format!("ws://127.0.0.1:{ws_port}");
                let _ = connect_tx.send(tungstenite::client::connect(url));
            })
            .expect("spawn");
        let mut socket = None;
        while Instant::now() < deadline {
            poll(&mut session);
            if let Ok(ready) = connect_rx.try_recv() {
                socket = Some(ready.expect("websocket connect"));
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let (mut socket, _) = socket.expect("websocket connect timed out");
        if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = socket.get_mut() {
            let _ = tcp.set_read_timeout(Some(Duration::from_millis(50)));
        }
        session
            .remote_ws_queue
            .push(r#"{"type":"test","ok":true}"#.into());
        let mut saw = false;
        while Instant::now() < deadline {
            poll(&mut session);
            match socket.read() {
                Ok(tungstenite::Message::Text(text)) => {
                    if text.contains("test") {
                        saw = true;
                        break;
                    }
                }
                Err(e) if client_read_timed_out(&e) => {}
                Err(e) => panic!("websocket read: {e}"),
                _ => {}
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(saw, "expected websocket event within {:?}", NETWORK_DEADLINE);
        // A selection change reaches connected clients without any remote command.
        session.foundation.selection = vec!["crate".into()];
        let mut selection = false;
        while Instant::now() < deadline {
            poll(&mut session);
            match socket.read() {
                Ok(tungstenite::Message::Text(text)) if text.contains("\"selection\"") => {
                    selection = text.contains("crate");
                    break;
                }
                Err(e) if client_read_timed_out(&e) => {}
                Err(e) => panic!("websocket read: {e}"),
                _ => {}
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(selection, "expected a selection event within {:?}", NETWORK_DEADLINE);
    }

    #[test]
    fn screenshot_asks_for_a_frame_then_returns_the_captured_png() {
        let mut session = session();
        let ask = |session: &mut EditorSession| {
            let (reply_tx, reply_rx) = channel();
            answer_http(
                session,
                HttpWork {
                    path: "/api/screenshot".into(),
                    method: "GET".into(),
                    body: String::new(),
                    token: "tok".into(),
                    reply: reply_tx,
                },
            );
            reply_rx.recv().unwrap()
        };
        let (code, _, _) = ask(&mut session);
        assert_eq!(code, 503, "no frame yet");
        assert!(session.remote_screenshot_wanted);
        set_remote_frame(&mut session, &[255, 0, 0, 255, 0, 255, 0, 255], 2, 1);
        assert!(!session.remote_screenshot_wanted);
        let (code, body, kind) = ask(&mut session);
        assert_eq!((code, kind), (200, "image/png"));
        assert_eq!(&body[1..4], b"PNG");
    }

    #[test]
    fn web_panel_served() {
        let mut session = session();
        let (reply_tx, reply_rx) = channel();
        answer_http(
            &mut session,
            HttpWork {
                path: "/panel".into(),
                method: "GET".into(),
                body: String::new(),
                token: "tok".into(),
                reply: reply_tx,
            },
        );
        let (code, body, ty) = reply_rx.recv().unwrap();
        assert_eq!(code, 200);
        assert_eq!(ty, "text/html; charset=utf-8");
        assert!(body.windows(4).any(|w| w == b"<html" || w == b"<!DO"));
    }
}

pub fn push_selection_event(session: &mut EditorSession) {
    if !enabled(session) {
        return;
    }
    session.remote_ws_queue.push(
        json!({
            "type": "selection",
            "ids": session.foundation.selection,
            "primary": session.models.selection,
        })
        .to_string(),
    );
}