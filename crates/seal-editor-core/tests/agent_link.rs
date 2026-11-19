//! AI tools reach the running editor over the assistant link and every change waits for the user.

use seal_editor_core::agent_link::poll;
use seal_editor_core::EditorSession;
use seal_editor_core::Layout;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

struct Client {
    out: TcpStream,
    input: BufReader<TcpStream>,
    next: u64,
}

impl Client {
    fn send(&mut self, method: &str, params: Value) {
        self.next += 1;
        let line = json!({ "jsonrpc": "2.0", "id": self.next, "method": method, "params": params });
        writeln!(self.out, "{line}").unwrap();
    }

    /// Poll the editor until an answer arrives, or `None` after `wait`.
    fn answer(&mut self, session: &mut EditorSession, wait: Duration) -> Option<Value> {
        let end = Instant::now() + wait;
        let mut line = String::new();
        while Instant::now() < end {
            poll(session);
            match self.input.read_line(&mut line) {
                Ok(n) if n > 0 && line.ends_with('\n') => {
                    return Some(serde_json::from_str(&line).unwrap())
                }
                _ => {}
            }
        }
        None
    }

    fn call(&mut self, session: &mut EditorSession, method: &str, params: Value) -> Value {
        self.send(method, params);
        self.answer(session, Duration::from_secs(5)).expect("an answer")
    }
}

fn text(answer: &Value) -> (String, bool) {
    let r = &answer["result"];
    (
        r["content"][0]["text"].as_str().unwrap_or("").to_string(),
        r["isError"].as_bool().unwrap_or(false),
    )
}

#[test]
fn an_assistant_lists_commands_and_runs_one_only_after_the_user_allows_it() {
    std::env::set_var("SEAL_AGENT_PORT", "0");
    let mut session = EditorSession::new(Layout::default());
    poll(&mut session);
    assert!(session.agent_link.is_none(), "off until switched on");
    session.foundation.settings.set_user("agent.bus.enabled", "true").unwrap();
    poll(&mut session);
    let port = session.agent_link.as_ref().expect("listening").port;
    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_millis(20))).unwrap();
    let mut c = Client { out: stream.try_clone().unwrap(), input: BufReader::new(stream), next: 0 };

    let init = c.call(&mut session, "initialize", json!({}));
    assert_eq!(init["result"]["serverInfo"]["name"], "seal-editor");
    let tools = c.call(&mut session, "tools/list", json!({}));
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 3);
    let (list, _) = text(&c.call(&mut session, "tools/call", json!({ "name": "list_commands" })));
    let list: Value = serde_json::from_str(&list).unwrap();
    assert!(list.as_array().unwrap().iter().any(|c| c["id"] == "app.toggle_fullscreen"));
    assert!(
        !list.as_array().unwrap().iter().any(|c| c["id"] == "app.reset_state"),
        "unsafe commands are not offered"
    );
    let (scene, _) = text(&c.call(&mut session, "tools/call", json!({ "name": "scene" })));
    assert!(scene.contains("\"objects\""));

    // Asking first is the default: the call waits until the user decides.
    c.send(
        "tools/call",
        json!({ "name": "run_command", "arguments": { "id": "app.toggle_fullscreen" } }),
    );
    assert!(c.answer(&mut session, Duration::from_millis(300)).is_none());
    assert_eq!(session.agent_pending.len(), 1);
    assert!(!session.foundation.fullscreen);
    let serial = session.agent_pending[0].serial;
    session.agent_decide(serial, true);
    let (said, failed) = text(&c.answer(&mut session, Duration::from_secs(5)).unwrap());
    assert!(!failed && said.contains("ran"), "{said}");
    assert!(session.foundation.fullscreen);

    c.send(
        "tools/call",
        json!({ "name": "run_command", "arguments": { "id": "app.toggle_fullscreen" } }),
    );
    assert!(c.answer(&mut session, Duration::from_millis(200)).is_none());
    let serial = session.agent_pending[0].serial;
    session.agent_decide(serial, false);
    let (said, failed) = text(&c.answer(&mut session, Duration::from_secs(5)).unwrap());
    assert!(failed && said.contains("denied"), "{said}");
    assert!(session.foundation.fullscreen, "denied calls change nothing");

    // With approvals off a safe command runs at once; unsafe ones never do.
    session.foundation.settings.set_user("agent.bus.require_approval", "false").unwrap();
    let (_, failed) = text(&c.call(
        &mut session,
        "tools/call",
        json!({ "name": "run_command", "arguments": { "id": "app.toggle_fullscreen" } }),
    ));
    assert!(!failed);
    assert!(!session.foundation.fullscreen);
    let (said, failed) = text(&c.call(
        &mut session,
        "tools/call",
        json!({ "name": "run_command", "arguments": { "id": "app.reset_state" } }),
    ));
    assert!(failed && said.contains("not open"), "{said}");
    let bad = c.call(&mut session, "nope", json!({}));
    assert_eq!(bad["error"]["code"], -32601);

    let outcomes: Vec<bool> = session.agent_log.iter().map(|e| e.ok).collect();
    assert_eq!(outcomes, vec![true, false, true, false]);

    // Switching the link off answers anything still waiting and stops listening.
    session.foundation.settings.set_user("agent.bus.enabled", "false").unwrap();
    poll(&mut session);
    assert!(session.agent_link.is_none());
}

#[test]
fn the_bridge_answers_with_a_clear_error_when_the_editor_is_closed() {
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let out = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    struct W(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for W {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let input = b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"initialize\"}\n" as &[u8];
    seal_editor_core::agent_link::bridge(port, input, W(out.clone())).unwrap();
    let text = String::from_utf8(out.lock().unwrap().clone()).unwrap();
    assert!(text.contains("\"id\":7") && text.contains("not reachable"), "{text}");
}
