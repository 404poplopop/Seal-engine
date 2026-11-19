//! File dialogs, encryption, and cloud calls go through the host.

use seal_graph::rt::Value;
use seal_graph::{Graph, GraphInstance, Host, Interp, Node, Program, Ty};
use seal_sealtext::SealValue;

struct Mock {
    secret: String,
    opens: Vec<(String, String)>,
    saves: Vec<(String, String)>,
    clouds: Vec<String>,
    path: String,
    logs: Vec<String>,
    button: String,
    text: String,
}

impl Host for Mock {
    fn get(&mut self, _: u64, _: &str, _: &str, _: Ty) -> Result<Value, String> {
        Err("none".into())
    }
    fn set(&mut self, _: u64, _: &str, _: &str, _: Value) -> Result<(), String> {
        Ok(())
    }
    fn log(&mut self, message: &str) {
        self.logs.push(message.to_string());
    }
    fn project_secret(&mut self) -> String {
        self.secret.clone()
    }
    fn open_file_dialog(&mut self, title: &str, filter: &str) -> Result<String, String> {
        self.opens.push((title.into(), filter.into()));
        Ok(self.path.clone())
    }
    fn save_file_dialog(&mut self, title: &str, name: &str) -> Result<String, String> {
        self.saves.push((title.into(), name.into()));
        Ok(self.path.clone())
    }
    fn cloud_upload(&mut self, slot: &str) -> Result<(), String> {
        self.clouds.push(format!("upload {slot}"));
        Ok(())
    }
    fn dialog_button(&mut self) -> String {
        self.button.clone()
    }
    fn dialog_text(&mut self) -> String {
        self.text.clone()
    }
}

fn s(text: &str) -> SealValue {
    SealValue::String(text.into())
}

fn defaults(text: &str) -> SealValue {
    seal_sealtext::parse(text).unwrap()
}

#[test]
fn file_dialogs_ask_the_host_and_return_the_path() {
    let g = Graph::new("files")
        .node(Node::new(1, "event").with("name", s("start")))
        .node(
            Node::new(2, "open_file_dialog")
                .with("defaults", defaults("title = \"Open\"\nfilter = \"png\"")),
        )
        .node(Node::new(3, "log"))
        .node(
            Node::new(4, "save_file_dialog")
                .with("defaults", defaults("title = \"Save\"\nname = \"slot.txt\"")),
        )
        .node(Node::new(5, "log"))
        .link(1, "then", 2, "in")
        .link(2, "then", 3, "in")
        .link(2, "path", 3, "message")
        .link(3, "then", 4, "in")
        .link(4, "then", 5, "in")
        .link(4, "path", 5, "message");
    let prog = Program::build(&g).unwrap();
    let mut host = Mock {
        secret: String::new(),
        opens: Vec::new(),
        saves: Vec::new(),
        clouds: Vec::new(),
        path: "/tmp/picked.png".into(),
        logs: Vec::new(),
        button: String::new(),
        text: String::new(),
    };
    Interp::new(prog, 1).fire("start", &[], &mut host).unwrap();
    assert_eq!(host.opens, vec![("Open".into(), "png".into())]);
    assert_eq!(host.saves, vec![("Save".into(), "slot.txt".into())]);
    assert_eq!(host.logs, vec!["/tmp/picked.png".to_string(), "/tmp/picked.png".to_string()]);
}

#[test]
fn encrypt_round_trip_and_a_changed_seal_fails() {
    let g = Graph::new("seal")
        .node(Node::new(1, "event").with("name", s("start")))
        .node(Node::new(2, "encrypt").with("defaults", defaults("text = \"hello\"")))
        .node(Node::new(3, "decrypt"))
        .node(Node::new(4, "log"))
        .link(1, "then", 2, "in")
        .link(2, "then", 3, "in")
        .link(2, "text", 3, "data")
        .link(3, "then", 4, "in")
        .link(3, "text", 4, "message");
    let prog = Program::build(&g).unwrap();
    let mut host = Mock {
        secret: "project key".into(),
        opens: Vec::new(),
        saves: Vec::new(),
        clouds: Vec::new(),
        path: String::new(),
        logs: Vec::new(),
        button: String::new(),
        text: String::new(),
    };
    Interp::new(prog, 1).fire("start", &[], &mut host).unwrap();
    assert_eq!(host.logs, vec!["hello".to_string()]);

    let sealed = seal_graph::crypto::seal("hello", "project key").unwrap();
    let mut bad = sealed.clone();
    // Flip one character of the encrypted payload (the nonce is random, so pick by position).
    let at = bad.find("data ").unwrap() + "data ".len() + 1;
    let flipped = if &bad[at..at + 1] == "A" { "B" } else { "A" };
    bad.replace_range(at..at + 1, flipped);
    let g = Graph::new("bad")
        .node(Node::new(1, "event").with("name", s("start")))
        .node(Node::new(2, "decrypt").with(
            "defaults",
            SealValue::Map(seal_sealtext::SealMap::new(vec![(
                "data".into(),
                SealValue::String(bad),
            )])),
        ))
        .link(1, "then", 2, "in");
    let err =
        Interp::new(Program::build(&g).unwrap(), 1).fire("start", &[], &mut host).unwrap_err();
    assert!(err.to_string().contains("changed or the project key"), "{err}");
}

#[test]
fn dialog_answer_and_cloud_upload_reach_the_host() {
    let g = Graph::new("ui")
        .node(Node::new(1, "event").with("name", s("dialog_answered")))
        .node(Node::new(2, "dialog_answer"))
        .node(Node::new(3, "log"))
        .node(Node::new(4, "cloud_upload").with("defaults", defaults("slot = \"Slot 1\"")))
        .link(1, "then", 3, "in")
        .link(2, "button", 3, "message")
        .link(3, "then", 4, "in");
    let mut host = Mock {
        secret: String::new(),
        opens: Vec::new(),
        saves: Vec::new(),
        clouds: Vec::new(),
        path: String::new(),
        logs: Vec::new(),
        button: "yes".into(),
        text: "typed".into(),
    };
    Interp::new(Program::build(&g).unwrap(), 1).fire("dialog_answered", &[], &mut host).unwrap();
    assert_eq!(host.logs, vec!["yes".to_string()]);
    assert_eq!(host.clouds, vec!["upload Slot 1".to_string()]);
}
