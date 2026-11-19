//! The assistant link: AI tools on this computer talk to the running editor.
//!
//! The editor listens on `127.0.0.1` (port [`DEFAULT_PORT`], or `SEAL_AGENT_PORT`)
//! while `agent.bus.enabled` is on. Each line is one JSON-RPC 2.0 message in
//! the MCP shape (`initialize`, `tools/list`, `tools/call`). `seal-editor mcp`
//! bridges an MCP client's stdin and stdout to this port.
//!
//! Network threads only move text; every request is answered on the editor
//! thread in [`poll`], so commands run exactly like menu clicks. A command that
//! changes something waits in [`EditorSession::agent_pending`] until the user
//! allows it in the Assistant panel, unless approvals are switched off.

use crate::session::EditorSession;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_PORT: u16 = 47700;
const LOG_KEEP: usize = 200;

/// The port the link uses: `SEAL_AGENT_PORT` when set, else [`DEFAULT_PORT`].
pub fn port() -> u16 {
    std::env::var("SEAL_AGENT_PORT")
        .ok()
        .and_then(|p| p.trim().parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

struct Incoming {
    request: Value,
    reply: Sender<String>,
}

/// A running listener.
pub struct AgentLink {
    pub port: u16,
    clients: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    rx: Receiver<Incoming>,
}

impl AgentLink {
    /// Start listening on `port`.
    pub fn start(port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let (tx, rx) = channel();
        let clients = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (c, s) = (clients.clone(), stop.clone());
        std::thread::Builder::new().name("seal-agent-link".into()).spawn(move || {
            while !s.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let (tx, c, s) = (tx.clone(), c.clone(), s.clone());
                        let _spawned = std::thread::Builder::new()
                            .name("seal-agent-client".into())
                            .spawn(move || serve(stream, tx, c, s));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(50)),
                }
            }
        })?;
        Ok(Self { port, clients, stop, rx })
    }

    /// How many tools are connected now.
    pub fn clients(&self) -> usize {
        self.clients.load(Ordering::Relaxed)
    }
}

impl Drop for AgentLink {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn serve(
    stream: TcpStream,
    tx: Sender<Incoming>,
    clients: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
) {
    clients.fetch_add(1, Ordering::Relaxed);
    let _blocking = stream.set_nonblocking(false);
    let _timeout = stream.set_read_timeout(Some(Duration::from_millis(250)));
    if let Ok(mut out) = stream.try_clone() {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        while !stop.load(Ordering::Relaxed) {
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let text = std::mem::take(&mut line);
                    if text.trim().is_empty() {
                        continue;
                    }
                    let answer = match serde_json::from_str::<Value>(&text) {
                        Ok(request) => {
                            let (reply, wait) = channel();
                            if tx.send(Incoming { request, reply }).is_err() {
                                break;
                            }
                            match wait.recv() {
                                Ok(answer) => answer,
                                Err(_) => break,
                            }
                        }
                        Err(e) => error(Value::Null, -32700, &format!("not JSON: {e}")),
                    };
                    if !answer.is_empty()
                        && writeln!(out, "{answer}").and_then(|_| out.flush()).is_err()
                    {
                        break;
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break,
            }
        }
    }
    clients.fetch_sub(1, Ordering::Relaxed);
}

fn error(id: Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

fn result(id: Value, value: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": value }).to_string()
}

fn tool_text(id: Value, text: String, is_error: bool) -> String {
    result(id, json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
}

/// A command an AI tool asked to run, waiting for the user.
pub struct PendingCall {
    pub serial: u64,
    pub command: String,
    pub title: String,
    pub args: String,
    rpc_id: Value,
    reply: Sender<String>,
}

/// One line of the Assistant panel's activity list.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentEntry {
    pub command: String,
    pub args: String,
    pub outcome: String,
    pub ok: bool,
}

/// Tools the link offers.
pub fn tools() -> Value {
    json!({ "tools": [
        {
            "name": "list_commands",
            "description": "Editor commands an assistant may run, with their argument schemas.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "scene",
            "description": "Objects in the open scene: id, name, position, rotation, scale, sprite, script and tag.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "run_command",
            "description": "Run one editor command, exactly like a menu click; it is one undo step. The user may have to allow it first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Command id from list_commands" },
                    "args": { "type": "object", "description": "Arguments, as the command's schema says" }
                },
                "required": ["id"]
            }
        }
    ]})
}

/// Start, stop and answer the link. Call once per frame.
pub fn poll(session: &mut EditorSession) {
    let enabled = session.foundation.settings.get("agent.bus.enabled") == Some("true");
    if !enabled {
        if session.agent_link.take().is_some() {
            for call in session.agent_pending.drain(..) {
                let _gone = call.reply.send(tool_text(
                    call.rpc_id,
                    "the assistant link was switched off".into(),
                    true,
                ));
            }
        }
        session.agent_link_error = None;
        return;
    }
    if session.agent_link.is_none() && session.agent_link_error.is_none() {
        match AgentLink::start(port()) {
            Ok(link) => session.agent_link = Some(link),
            Err(e) => session.agent_link_error = Some(format!("port {}: {e}", port())),
        }
    }
    let incoming: Vec<Incoming> = match &session.agent_link {
        Some(link) => link.rx.try_iter().collect(),
        None => Vec::new(),
    };
    for Incoming { request, reply } in incoming {
        if let Some(answer) = answer(session, request, &reply) {
            let _gone = reply.send(answer);
        }
    }
}

/// Answer one request now, or `None` when it waits for the user.
fn answer(session: &mut EditorSession, request: Value, reply: &Sender<String>) -> Option<String> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let Some(id) = id else {
        // Notifications get no answer.
        return Some(String::new());
    };
    Some(match method {
        "initialize" => result(
            id,
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "seal-editor", "version": env!("CARGO_PKG_VERSION") }
            }),
        ),
        "ping" => result(id, json!({})),
        "tools/list" => result(id, tools()),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(Value::Null);
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match name {
                "list_commands" => tool_text(id, list_commands(session).to_string(), false),
                "scene" => tool_text(id, scene(session).to_string(), false),
                "run_command" => return run_command(session, id, &args, reply),
                other => tool_text(id, format!("no tool named '{other}'"), true),
            }
        }
        other => error(id, -32601, &format!("no method '{other}'")),
    })
}

fn list_commands(session: &EditorSession) -> Value {
    let mut out = Vec::new();
    for spec in session.commands.specs() {
        if !spec.agent_safe || spec.alias_of.is_some() {
            continue;
        }
        let schema: Value = serde_json::from_str(&spec.schema_json()).unwrap_or(Value::Null);
        out.push(json!({
            "id": spec.id,
            "title": session.messages.get(&spec.title_key),
            "category": spec.category,
            "schema": schema,
        }));
    }
    Value::Array(out)
}

fn scene(session: &EditorSession) -> Value {
    let objects: Vec<Value> = session
        .world
        .entities
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "name": e.name,
                "position": e.translation,
                "rotation": e.rotation,
                "scale": e.scale,
                "sprite": e.sprite,
                "script": e.script,
                "tag": e.tag,
            })
        })
        .collect();
    json!({ "objects": objects, "playing": session.world.mode != seal_world::PlayMode::Editing })
}

fn run_command(
    session: &mut EditorSession,
    id: Value,
    args: &Value,
    reply: &Sender<String>,
) -> Option<String> {
    let Some(command) = args.get("id").and_then(Value::as_str).map(str::to_string) else {
        return Some(tool_text(
            id,
            "missing \"id\" (a command id from list_commands)".into(),
            true,
        ));
    };
    let args_json = args.get("args").map_or_else(|| "{}".to_string(), Value::to_string);
    let spec = session.commands.canonical(&command).and_then(|c| session.commands.get(&c).cloned());
    let Some(spec) = spec else {
        return Some(tool_text(id, format!("unknown command '{command}'"), true));
    };
    if !spec.agent_safe {
        session.agent_record(&spec.id, &args_json, "refused: not open to assistants", false);
        return Some(tool_text(id, format!("'{}' is not open to assistants", spec.id), true));
    }
    if session.foundation.settings.get("agent.bus.require_approval") != Some("false") {
        session.agent_serial += 1;
        session.agent_pending.push(PendingCall {
            serial: session.agent_serial,
            command: spec.id.clone(),
            title: session.messages.get(&spec.title_key),
            args: args_json,
            rpc_id: id,
            reply: reply.clone(),
        });
        session.reveal_panel = Some("assistant".into());
        return None;
    }
    let (text, ok) = session.agent_execute(&spec.id, &args_json);
    Some(tool_text(id, text, !ok))
}

impl EditorSession {
    /// Run an allowed assistant command and record it. Returns the answer text and whether it worked.
    pub fn agent_execute(&mut self, command: &str, args_json: &str) -> (String, bool) {
        self.foundation.agent_activity = command.to_string();
        let outcome =
            crate::command::CommandArgs::from_json_object(if args_json.trim().is_empty() {
                "{}"
            } else {
                args_json
            })
            .and_then(|args| self.dispatch(command, &args));
        let (text, ok) = match outcome {
            Ok(()) => (format!("ran {command}"), true),
            Err(e) => (format!("{command} failed: {e}"), false),
        };
        self.agent_record(command, args_json, &text, ok);
        (text, ok)
    }

    /// Allow or deny the waiting call `serial`.
    pub fn agent_decide(&mut self, serial: u64, allow: bool) {
        let Some(i) = self.agent_pending.iter().position(|c| c.serial == serial) else { return };
        let call = self.agent_pending.remove(i);
        let answer = if allow {
            let (text, ok) = self.agent_execute(&call.command, &call.args);
            tool_text(call.rpc_id, text, !ok)
        } else {
            self.agent_record(&call.command, &call.args, "denied by the user", false);
            tool_text(call.rpc_id, format!("the user denied {}", call.command), true)
        };
        let _gone = call.reply.send(answer);
    }

    /// Add a line to the activity list and to the project's assistant log file.
    pub fn agent_record(&mut self, command: &str, args: &str, outcome: &str, ok: bool) {
        self.agent_log.push(AgentEntry {
            command: command.into(),
            args: args.into(),
            outcome: outcome.into(),
            ok,
        });
        if self.agent_log.len() > LOG_KEEP {
            self.agent_log.remove(0);
        }
        if let Some(project) = &self.project {
            let file = project.root.join(".seal-editor").join("assistant-log.txt");
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let line =
                format!("{secs}\t{}\t{command}\t{args}\t{outcome}\n", if ok { "ok" } else { "no" });
            let written =
                std::fs::create_dir_all(file.parent().unwrap_or(&project.root)).and_then(|_| {
                    std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&file)?
                        .write_all(line.as_bytes())
                });
            if written.is_err() {
                seal_log::log(
                    seal_log::Level::Warn,
                    "assistant",
                    "could not write the assistant log".to_string(),
                );
            }
        }
    }

    /// Read back the project's assistant log, newest last, after opening it.
    pub fn agent_load_log(&mut self) {
        self.agent_log.clear();
        let Some(project) = &self.project else { return };
        let Ok(text) =
            std::fs::read_to_string(project.root.join(".seal-editor").join("assistant-log.txt"))
        else {
            return;
        };
        let lines: Vec<&str> = text.lines().collect();
        for line in lines.iter().skip(lines.len().saturating_sub(LOG_KEEP)) {
            let parts: Vec<&str> = line.splitn(5, '\t').collect();
            if let [_, ok, command, args, outcome] = parts[..] {
                self.agent_log.push(AgentEntry {
                    command: command.into(),
                    args: args.into(),
                    outcome: outcome.into(),
                    ok: ok == "ok",
                });
            }
        }
    }
}

/// `seal-editor mcp`: relay an AI tool's stdin/stdout to the running editor's link.
/// While the editor is unreachable every request gets a plain error back instead of silence.
pub fn bridge(
    port: u16,
    input: impl BufRead,
    output: impl Write + Send + 'static,
) -> std::io::Result<()> {
    let output = Arc::new(std::sync::Mutex::new(output));
    let mut conn: Option<TcpStream> = None;
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if conn.is_none() {
            if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
                let reader = stream.try_clone()?;
                let out = Arc::clone(&output);
                std::thread::spawn(move || {
                    for reply in BufReader::new(reader).lines().map_while(Result::ok) {
                        let mut out = out.lock().unwrap_or_else(|e| e.into_inner());
                        if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
                            break;
                        }
                    }
                });
                conn = Some(stream);
            }
        }
        let sent =
            conn.as_mut().is_some_and(|c| writeln!(c, "{line}").and_then(|_| c.flush()).is_ok());
        if !sent {
            conn = None;
            let id = serde_json::from_str::<Value>(&line).ok().and_then(|v| v.get("id").cloned());
            if let Some(id) = id {
                let reply = json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message":
                    format!("The Seal editor is not reachable on port {port}. Open it and switch on Window > Assistant > Let AI tools connect.")}});
                let mut out = output.lock().unwrap_or_else(|e| e.into_inner());
                writeln!(out, "{reply}")?;
                out.flush()?;
            }
        }
    }
    Ok(())
}
