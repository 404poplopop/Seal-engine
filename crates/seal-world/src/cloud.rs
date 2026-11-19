//! Cloud save queue. Uploads and downloads wait here when the provider
//! cannot be reached, and a newer timestamp wins when both sides have a save.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use seal_sealtext::{parse, SealValue};

use crate::savegame;

const FILE: &str = "saved/cloud/queue.seal";

/// One queued transfer.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub slot: String,
    pub kind: String,
    pub saved_at: u64,
    /// The save file text for an upload. Empty for a download.
    pub body: String,
    pub announced: bool,
    pub told: bool,
}

/// What a script asked the cloud to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Upload(String),
    Download(String),
    Flush,
    Reply { slot: String, saved_at: u64, body: String },
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Queue {
    pub jobs: Vec<Job>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Note {
    Uploaded { slot: String },
    Downloaded { slot: String, body: String },
    Conflict { slot: String, kept: String },
    Waiting { slot: String, message: String },
    Hook { slot: String, kind: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fail {
    Offline(String),
    Bad(String),
}

pub enum Up {
    Stored,
    RemoteNewer(u64),
}

pub enum Down {
    Missing,
    Found { saved_at: u64, body: String },
}

pub trait Api {
    fn upload(&mut self, slot: &str, saved_at: u64, body: &str) -> Result<Up, Fail>;
    fn download(&mut self, slot: &str) -> Result<Down, Fail>;
}

pub enum Mode<'a> {
    Http { url: &'a str },
    Script,
    Custom(&'a mut dyn Api),
}

pub fn queue_path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

pub fn load(dir: &Path) -> Result<Queue, String> {
    let path = queue_path(dir);
    if !path.is_file() {
        return Ok(Queue::default());
    }
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("cannot read the cloud queue: {e}"))?;
    parse_queue(&text)
}

pub fn store(dir: &Path, queue: &Queue) -> Result<(), String> {
    let path = queue_path(dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot make the cloud queue folder: {e}"))?;
    }
    let tmp = path.with_extension("seal.tmp");
    std::fs::write(&tmp, queue_text(queue))
        .and_then(|_| std::fs::rename(&tmp, &path))
        .map_err(|e| format!("cannot write the cloud queue: {e}"))
}

pub fn enqueue_upload(queue: &mut Queue, slot: &str, saved_at: u64, body: String) {
    queue.jobs.retain(|j| !(j.slot == slot && j.kind == "upload"));
    queue.jobs.push(Job {
        slot: slot.to_string(),
        kind: "upload".into(),
        saved_at,
        body,
        announced: false,
        told: false,
    });
}

pub fn enqueue_download(queue: &mut Queue, slot: &str) {
    if queue.jobs.iter().any(|j| j.slot == slot && j.kind == "download") {
        return;
    }
    queue.jobs.push(Job {
        slot: slot.to_string(),
        kind: "download".into(),
        saved_at: 0,
        body: String::new(),
        announced: false,
        told: false,
    });
}

/// Try every job. Offline jobs stay in the queue.
pub fn flush(
    queue: &mut Queue,
    local_at: &dyn Fn(&str) -> Option<u64>,
    mode: Mode<'_>,
) -> Vec<Note> {
    match mode {
        Mode::Script => flush_script(queue),
        Mode::Http { url } => {
            let mut http = Http { url };
            flush_api(queue, local_at, &mut http)
        }
        Mode::Custom(api) => flush_api(queue, local_at, api),
    }
}

fn flush_script(queue: &mut Queue) -> Vec<Note> {
    let mut notes = Vec::new();
    for job in &mut queue.jobs {
        if job.announced {
            continue;
        }
        job.announced = true;
        notes.push(Note::Hook { slot: job.slot.clone(), kind: job.kind.clone() });
    }
    notes
}

fn flush_api(
    queue: &mut Queue,
    local_at: &dyn Fn(&str) -> Option<u64>,
    api: &mut dyn Api,
) -> Vec<Note> {
    let mut notes = Vec::new();
    let mut left = Vec::new();
    for mut job in std::mem::take(&mut queue.jobs) {
        let result = if job.kind == "upload" {
            match api.upload(&job.slot, job.saved_at, &job.body) {
                Ok(Up::Stored) => Ok(Note::Uploaded { slot: job.slot.clone() }),
                Ok(Up::RemoteNewer(_)) => {
                    Ok(Note::Conflict { slot: job.slot.clone(), kept: "remote".into() })
                }
                Err(Fail::Offline(m)) | Err(Fail::Bad(m)) => Err(m),
            }
        } else {
            match api.download(&job.slot) {
                Ok(Down::Missing) => {
                    Ok(Note::Conflict { slot: job.slot.clone(), kept: "local".into() })
                }
                Ok(Down::Found { saved_at, body }) => {
                    let local = local_at(&job.slot);
                    if local.is_some_and(|t| t >= saved_at) {
                        Ok(Note::Conflict { slot: job.slot.clone(), kept: "local".into() })
                    } else {
                        Ok(Note::Downloaded { slot: job.slot.clone(), body })
                    }
                }
                Err(Fail::Offline(m)) | Err(Fail::Bad(m)) => Err(m),
            }
        };
        match result {
            Ok(note) => notes.push(note),
            Err(message) => {
                if !job.told {
                    notes.push(Note::Waiting { slot: job.slot.clone(), message });
                    job.told = true;
                }
                left.push(job);
            }
        }
    }
    queue.jobs = left;
    notes
}

/// Finish a script-hook job. `body` is the remote save text for a download.
pub fn reply(
    queue: &mut Queue,
    slot: &str,
    remote_at: u64,
    body: &str,
    local_at: Option<u64>,
) -> Result<Note, String> {
    let index = queue
        .jobs
        .iter()
        .position(|j| j.slot == slot)
        .ok_or_else(|| format!("there is no cloud job for {slot}"))?;
    let job = queue.jobs.remove(index);
    let note = if job.kind == "upload" {
        if remote_at > job.saved_at {
            Note::Conflict { slot: job.slot, kept: "remote".into() }
        } else {
            Note::Uploaded { slot: job.slot }
        }
    } else if body.is_empty() || local_at.is_some_and(|t| t >= remote_at) {
        Note::Conflict { slot: job.slot, kept: "local".into() }
    } else {
        Note::Downloaded { slot: job.slot, body: body.to_string() }
    };
    Ok(note)
}

pub fn job<'a>(queue: &'a Queue, slot: &str) -> Option<&'a Job> {
    queue.jobs.iter().find(|j| j.slot == slot)
}

fn queue_text(queue: &Queue) -> String {
    let jobs = SealValue::Array(
        queue
            .jobs
            .iter()
            .map(|j| {
                SealValue::Map(
                    vec![
                        ("slot".into(), SealValue::String(j.slot.clone())),
                        ("kind".into(), SealValue::String(j.kind.clone())),
                        ("saved_at".into(), SealValue::Number(j.saved_at.to_string())),
                        (
                            "body".into(),
                            SealValue::String(seal_graph::crypto::encode_b64(j.body.as_bytes())),
                        ),
                        ("announced".into(), SealValue::Bool(j.announced)),
                        ("told".into(), SealValue::Bool(j.told)),
                    ]
                    .into(),
                )
            })
            .collect(),
    );
    SealValue::Map(
        vec![("seal_cloud_queue".into(), SealValue::Number("1".into())), ("jobs".into(), jobs)]
            .into(),
    )
    .to_text()
}

pub fn parse_queue(text: &str) -> Result<Queue, String> {
    let value = parse(text).map_err(|e| format!("the cloud queue is damaged ({e})"))?;
    match value.get("seal_cloud_queue") {
        Some(SealValue::Number(n)) if n == "1" => {}
        _ => return Err("this is not a cloud queue".into()),
    }
    let mut jobs = Vec::new();
    let items = value.get("jobs").and_then(|v| match v {
        SealValue::Array(items) => Some(items.as_slice()),
        _ => None,
    });
    let Some(items) = items else {
        return Err("the cloud queue has no jobs list".into());
    };
    for item in items {
        let slot = text_of(item, "slot");
        let kind = text_of(item, "kind");
        if slot.is_empty() || (kind != "upload" && kind != "download") {
            return Err("a cloud queue job is missing its slot or kind".into());
        }
        let saved_at = match item.get("saved_at") {
            Some(SealValue::Number(n)) => n.parse::<u64>().unwrap_or(0),
            _ => 0,
        };
        let body = match item.get("body") {
            Some(SealValue::String(s)) if !s.is_empty() => {
                let bytes = seal_graph::crypto::decode_b64(s)
                    .map_err(|_| format!("the cloud queue job {slot} has a damaged body"))?;
                String::from_utf8(bytes)
                    .map_err(|_| format!("the cloud queue job {slot} is not text"))?
            }
            _ => String::new(),
        };
        let flag = |key: &str| matches!(item.get(key), Some(SealValue::Bool(true)));
        jobs.push(Job {
            slot,
            kind,
            saved_at,
            body,
            announced: flag("announced"),
            told: flag("told"),
        });
    }
    Ok(Queue { jobs })
}

fn text_of(value: &SealValue, key: &str) -> String {
    match value.get(key) {
        Some(SealValue::String(s)) => s.clone(),
        _ => String::new(),
    }
}

struct Http<'a> {
    url: &'a str,
}

impl Api for Http<'_> {
    fn upload(&mut self, slot: &str, saved_at: u64, body: &str) -> Result<Up, Fail> {
        let payload =
            format!("{slot}\n{saved_at}\n{}", seal_graph::crypto::encode_b64(body.as_bytes()));
        let text = http(self.url, "POST", "/upload", &payload)?;
        let text = text.trim();
        if text == "ok" || text.is_empty() {
            return Ok(Up::Stored);
        }
        if let Some(rest) = text.strip_prefix("conflict") {
            let at = rest.trim().parse::<u64>().unwrap_or(0);
            return Ok(Up::RemoteNewer(at));
        }
        Err(Fail::Bad(format!("the cloud endpoint replied {text:?}")))
    }

    fn download(&mut self, slot: &str) -> Result<Down, Fail> {
        let path = format!("/download?slot={}", encode_query(slot));
        let text = http(self.url, "GET", &path, "")?;
        let text = text.trim();
        if text == "missing" || text.is_empty() {
            return Ok(Down::Missing);
        }
        let mut lines = text.lines();
        let saved_at = lines
            .next()
            .and_then(|l| l.trim().strip_prefix("saved_at"))
            .map(str::trim)
            .and_then(|n| n.parse::<u64>().ok())
            .ok_or_else(|| Fail::Bad("the cloud download has no saved_at".into()))?;
        let b64: String = lines.collect::<Vec<_>>().join("");
        let bytes = seal_graph::crypto::decode_b64(b64.trim())
            .map_err(|_| Fail::Bad("the cloud download has a damaged body".into()))?;
        let body = String::from_utf8(bytes)
            .map_err(|_| Fail::Bad("the cloud download is not text".into()))?;
        Ok(Down::Found { saved_at, body })
    }
}

fn encode_query(slot: &str) -> String {
    let mut out = String::new();
    for b in slot.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b' ') {
            if b == b' ' {
                out.push_str("%20");
            } else {
                out.push(b as char);
            }
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn http(url: &str, method: &str, path: &str, body: &str) -> Result<String, Fail> {
    let (tls, host, port, prefix) = split_url(url).map_err(Fail::Bad)?;
    let full = format!("{prefix}{path}");
    let request = format!(
        "{method} {full} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let raw = exchange(tls, &host, port, request.as_bytes())?;
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or(("", text.as_ref()));
    let status = head.lines().next().unwrap_or("");
    if status.contains(" 200 ") || status.contains(" 204 ") {
        Ok(body.to_string())
    } else if status.contains(" 404 ") {
        Ok("missing".into())
    } else if status.is_empty() {
        Err(Fail::Offline("the cloud endpoint closed without a reply".into()))
    } else {
        Err(Fail::Bad(format!("the cloud endpoint replied {status}")))
    }
}

/// Send one request and read the whole reply, over TLS for `https`.
fn exchange(tls: bool, host: &str, port: u16, request: &[u8]) -> Result<Vec<u8>, Fail> {
    let host = host.to_string();
    let unreachable =
        |e: std::io::Error| Fail::Offline(format!("the cloud endpoint is unreachable ({e})"));
    // Host names resolve through the system resolver; the first address that answers is used.
    let addrs: Vec<std::net::SocketAddr> =
        std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port))
            .map_err(unreachable)?
            .collect();
    let mut last = None;
    let mut stream = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, Duration::from_secs(2)) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => last = Some(e),
        }
    }
    let mut stream = match stream {
        Some(s) => s,
        None => {
            return Err(unreachable(last.unwrap_or_else(|| std::io::Error::other("no address"))))
        }
    };
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
    let mut raw = Vec::new();
    if tls {
        let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|e| Fail::Bad(format!("TLS setup: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from(host.clone())
            .map_err(|e| Fail::Bad(format!("cloud host name: {e}")))?;
        let conn = rustls::ClientConnection::new(std::sync::Arc::new(config), name)
            .map_err(|e| Fail::Bad(format!("TLS setup: {e}")))?;
        let mut secure = rustls::StreamOwned::new(conn, stream);
        secure.write_all(request).map_err(|e| Fail::Offline(e.to_string()))?;
        // A server that closes without close_notify still sent a whole reply.
        match secure.read_to_end(&mut raw) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && !raw.is_empty() => {}
            Err(e) => return Err(Fail::Offline(e.to_string())),
        }
    } else {
        stream.write_all(request).map_err(|e| Fail::Offline(e.to_string()))?;
        stream.read_to_end(&mut raw).map_err(|e| Fail::Offline(e.to_string()))?;
    }
    Ok(raw)
}

/// Download `url` (following up to five redirects) into memory, at most `limit` bytes.
/// Used for editor tool downloads; handles fixed-length and chunked replies.
pub fn https_get(url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let mut url = url.to_string();
    for _ in 0..6 {
        let (tls, host, port, path) = split_url(&url)?;
        let path = if path.is_empty() { "/".to_string() } else { path };
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: Seal\r\nAccept: */*\r\nConnection: close\r\n\r\n"
        );
        let raw = exchange(tls, &host, port, request.as_bytes()).map_err(|f| match f {
            Fail::Offline(m) | Fail::Bad(m) => m,
        })?;
        let split =
            raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("the reply had no header")?;
        let head = String::from_utf8_lossy(&raw[..split]).to_string();
        let body = &raw[split + 4..];
        let status: u16 = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .ok_or("the reply had no status")?;
        let header = |name: &str| {
            head.lines().skip(1).find_map(|line| {
                let (k, v) = line.split_once(':')?;
                k.trim().eq_ignore_ascii_case(name).then(|| v.trim().to_string())
            })
        };
        if (300..400).contains(&status) {
            let to = header("location").ok_or("a redirect had no location")?;
            url = if to.starts_with("http") {
                to
            } else {
                format!("{}://{host}:{port}{to}", if tls { "https" } else { "http" })
            };
            continue;
        }
        if status != 200 {
            return Err(format!("the server replied {status}"));
        }
        let data = if header("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked"))
        {
            dechunk(body)?
        } else {
            body.to_vec()
        };
        if data.len() > limit {
            return Err("the download is larger than expected".into());
        }
        return Ok(data);
    }
    Err("too many redirects".into())
}

fn dechunk(mut body: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let line_end = body.windows(2).position(|w| w == b"\r\n").ok_or("a chunk was cut off")?;
        let size_text = String::from_utf8_lossy(&body[..line_end]);
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| "a chunk size was not a number".to_string())?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        if body.len() < size {
            return Err("a chunk was cut off".into());
        }
        out.extend_from_slice(&body[..size]);
        body = body.get(size + 2..).unwrap_or(&[]);
    }
}

/// (uses TLS, host, port, path prefix) of an `https://` or `http://` endpoint.
fn split_url(url: &str) -> Result<(bool, String, u16, String), String> {
    let url = url.trim();
    let (tls, rest) = match (url.strip_prefix("https://"), url.strip_prefix("http://")) {
        (Some(rest), _) => (true, rest),
        (None, Some(rest)) => (false, rest),
        _ => return Err("the cloud endpoint must be an https:// or http:// address".into()),
    };
    if rest.is_empty() {
        return Err("the cloud endpoint must be an https:// or http:// address".into());
    }
    let (hostport, path) = match rest.split_once('/') {
        Some((h, p)) => (h, format!("/{p}").trim_end_matches('/').to_string()),
        None => (rest, String::new()),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => {
            let port =
                p.parse::<u16>().map_err(|_| format!("the cloud port {p:?} is not a number"))?;
            (h.to_string(), port)
        }
        None => (hostport.to_string(), if tls { 443 } else { 80 }),
    };
    if host.is_empty() {
        return Err("the cloud endpoint has no host".into());
    }
    Ok((tls, host, port, path))
}

/// Read a local slot's timestamp, if the file loads.
pub fn local_saved_at(dir: &Path, slot: &str) -> Option<u64> {
    savegame::read(dir, slot).ok().map(|s| s.saved_at)
}

/// Write a downloaded save file as the provider sent it.
pub fn write_downloaded(dir: &Path, slot: &str, body: &str) -> Result<(), String> {
    let path = savegame::slot_path(dir, slot)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("sealsave.tmp");
    std::fs::write(&tmp, body)
        .and_then(|_| std::fs::rename(&tmp, &path))
        .map_err(|e| format!("cannot write the downloaded save: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Mock {
        remote: Vec<(String, u64, String)>,
        offline: bool,
    }

    impl Api for Mock {
        fn upload(&mut self, slot: &str, saved_at: u64, body: &str) -> Result<Up, Fail> {
            if self.offline {
                return Err(Fail::Offline("no network".into()));
            }
            if let Some(found) = self.remote.iter().find(|r| r.0 == slot) {
                if found.1 > saved_at {
                    return Ok(Up::RemoteNewer(found.1));
                }
            }
            self.remote.retain(|r| r.0 != slot);
            self.remote.push((slot.into(), saved_at, body.into()));
            Ok(Up::Stored)
        }
        fn download(&mut self, slot: &str) -> Result<Down, Fail> {
            if self.offline {
                return Err(Fail::Offline("no network".into()));
            }
            match self.remote.iter().find(|r| r.0 == slot) {
                Some(r) => Ok(Down::Found { saved_at: r.1, body: r.2.clone() }),
                None => Ok(Down::Missing),
            }
        }
    }

    #[test]
    fn queue_uploads_downloads_and_keeps_the_newer_save() {
        let mut q = Queue::default();
        enqueue_upload(&mut q, "Slot 1", 10, "local".into());
        let mut mock =
            Mock { remote: vec![("Slot 1".into(), 20, "remote".into())], offline: false };
        let notes = flush(&mut q, &|_| Some(10), Mode::Custom(&mut mock));
        assert!(matches!(&notes[0], Note::Conflict { kept, .. } if kept == "remote"));
        assert!(q.jobs.is_empty(), "a finished conflict leaves the queue");

        enqueue_download(&mut q, "Slot 1");
        let notes = flush(&mut q, &|_| Some(30), Mode::Custom(&mut mock));
        assert!(matches!(&notes[0], Note::Conflict { kept, .. } if kept == "local"));

        enqueue_download(&mut q, "Slot 1");
        let notes = flush(&mut q, &|_| Some(5), Mode::Custom(&mut mock));
        match &notes[0] {
            Note::Downloaded { body, .. } => assert_eq!(body, "remote"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_offline_provider_keeps_the_job_and_a_damaged_queue_is_refused() {
        let mut q = Queue::default();
        enqueue_upload(&mut q, "Slot 1", 4, "body".into());
        let mut mock = Mock { remote: Vec::new(), offline: true };
        let notes = flush(&mut q, &|_| None, Mode::Custom(&mut mock));
        assert!(
            matches!(&notes[0], Note::Waiting { message, .. } if message.contains("no network"))
        );
        assert_eq!(q.jobs.len(), 1);
        let again = flush(&mut q, &|_| None, Mode::Custom(&mut mock));
        assert!(again.is_empty(), "a job that already said it is waiting stays quiet");

        let text = queue_text(&q);
        let loaded = parse_queue(&text).unwrap();
        assert_eq!(loaded.jobs[0].body, "body");
        assert!(parse_queue("not a queue\n").is_err());
        assert!(parse_queue(
            "seal_cloud_queue = 1\njobs = [{ slot = \"A\", kind = \"upload\", body = \"!!!!\" }]\n"
        )
        .is_err());
    }

    #[test]
    fn a_script_hook_reports_the_job_and_a_reply_uses_the_timestamp() {
        let mut q = Queue::default();
        enqueue_upload(&mut q, "Slot 1", 8, "saved".into());
        let notes = flush(&mut q, &|_| None, Mode::Script);
        assert!(matches!(&notes[0], Note::Hook { kind, .. } if kind == "upload"));
        assert!(flush(&mut q, &|_| None, Mode::Script).is_empty());
        let note = reply(&mut q, "Slot 1", 3, "", Some(8)).unwrap();
        assert!(matches!(note, Note::Uploaded { .. }));
        enqueue_download(&mut q, "Slot 1");
        let _ = flush(&mut q, &|_| None, Mode::Script);
        let note = reply(&mut q, "Slot 1", 9, "newer", Some(8)).unwrap();
        match note {
            Note::Downloaded { body, .. } => assert_eq!(body, "newer"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn http_upload_and_download_round_trip_on_localhost() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            sock.set_read_timeout(Some(Duration::from_secs(2))).ok();
            let mut buf = [0u8; 4096];
            let n = sock.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..n]);
            assert!(req.starts_with("POST "), "{req}");
            let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
            sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .unwrap();
            let _ = sock.flush();
            body.to_string()
        });
        let mut http = Http { url: &format!("http://127.0.0.1:{port}") };
        let up = http.upload("Slot 1", 4, "hello").unwrap();
        assert!(matches!(up, Up::Stored));
        let sent = server.join().unwrap();
        assert!(sent.contains("Slot 1"));
        assert!(sent.contains(&seal_graph::crypto::encode_b64(b"hello")));
    }
}

#[cfg(test)]
mod endpoint_tests {
    use super::{http, split_url};

    #[test]
    fn https_and_http_endpoints_parse_with_default_ports() {
        assert_eq!(
            split_url("https://saves.example.com/api/").unwrap(),
            (true, "saves.example.com".into(), 443, "/api".into())
        );
        assert_eq!(
            split_url("http://127.0.0.1:8080").unwrap(),
            (false, "127.0.0.1".into(), 8080, String::new())
        );
        assert!(split_url("ftp://x").is_err());
        assert!(split_url("https://").is_err());
    }

    #[test]
    fn get_follows_a_redirect_and_reads_a_chunked_body() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for reply in [
                "HTTP/1.1 302 Found\r\nLocation: /file\r\n\r\n".to_string(),
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n".to_string(),
            ] {
                let (mut sock, _) = listener.accept().unwrap();
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf);
                sock.write_all(reply.as_bytes()).unwrap();
            }
        });
        let got = super::https_get(&format!("http://127.0.0.1:{port}/start"), 1024).unwrap();
        assert_eq!(got, b"hello world");
    }

    #[test]
    fn https_to_a_plain_server_fails_cleanly() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                use std::io::Write;
                let _ = sock.write_all(b"HTTP/1.1 200 OK\r\n\r\nplain");
            }
        });
        // Not a TLS server: the handshake fails and the save reports an error, never panics.
        assert!(http(&format!("https://localhost:{port}"), "GET", "/slot", "").is_err());
    }
}
