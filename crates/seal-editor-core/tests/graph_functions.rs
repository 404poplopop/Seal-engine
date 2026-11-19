//! Function edits in the graph document: save, undo, calls and the node search.

use seal_editor_core::graph_doc::{GraphDoc, PinSide};

fn open(name: &str) -> (GraphDoc, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("seal-gf-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("logic").join("hello.graph");
    let doc = GraphDoc::open(&path, "logic/hello.graph").expect("open");
    (doc, path)
}

#[test]
fn adding_a_function_saves_and_reopens_on_that_function() {
    let (mut doc, path) = open("add");
    let name = doc.add_function().expect("function");
    assert_eq!(doc.page.as_deref(), Some(name.as_str()));
    assert!(doc.problems.is_empty(), "{:?}", doc.problems);
    assert!(doc.can_undo());
    drop(doc);
    let doc = GraphDoc::open(&path, "logic/hello.graph").expect("reopen");
    assert_eq!(doc.graph.functions.len(), 1);
    assert_eq!(doc.page.as_deref(), Some("function"));
    assert_eq!(doc.graph.functions[0].inputs[0].name, "value");
    assert_eq!(doc.graph.functions[0].outputs[0].name, "result");
    let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
}

#[test]
fn undo_removes_the_function_from_the_file() {
    let (mut doc, path) = open("undo");
    doc.add_function().unwrap();
    assert!(doc.undo());
    assert!(doc.graph.functions.is_empty());
    assert!(doc.page.is_none());
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("functions"));
    assert!(doc.redo());
    assert_eq!(doc.graph.functions.len(), 1);
    let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
}

#[test]
fn the_body_stays_off_the_event_graph_and_the_search_offers_call() {
    let (mut doc, path) = open("body");
    doc.add_function().unwrap();
    let log = doc.add_node("log", [20.0, 20.0]).unwrap();
    assert!(doc.graph.nodes.iter().all(|n| n.id != log));
    assert!(doc.graph.functions[0].nodes.iter().any(|n| n.id == log));
    let rows = doc.palette("");
    assert!(rows.iter().any(|r| r.kind == "fn_return"));
    assert!(rows.iter().any(|r| r.kind == "call:function"));
    assert!(rows.iter().all(|r| r.kind != "event"));
    doc.show_events();
    assert!(doc.node(log).is_none());
    let rows = doc.palette("");
    assert!(rows.iter().any(|r| r.kind == "event"));
    assert!(rows.iter().any(|r| r.kind == "call:function"));
    assert!(rows.iter().all(|r| r.kind != "fn_return"));
    let call = doc.add_node("call:function", [0.0, 0.0]).unwrap();
    doc.rename_function("function", "double").unwrap();
    assert!(doc.settings(call).iter().any(|(key, value)| key == "name" && value == "double"));
    doc.show_function("double");
    assert!(doc.node(log).is_some());
    let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
}

#[test]
fn renaming_a_pin_relinks_and_a_type_change_is_reported() {
    let (mut doc, path) = open("sig");
    doc.add_function().unwrap();
    doc.show_events();
    let call = doc.add_node("call:function", [0.0, 0.0]).unwrap();
    let text = doc.add_node("to_str", [220.0, 0.0]).unwrap();
    assert!(doc.set_setting(text, "type", "int"));
    doc.connect((call, "result", PinSide::Out), (text, "a", PinSide::In)).unwrap();
    doc.set_output_name("function", 0, "total").unwrap();
    assert!(doc.graph.links.iter().any(|l| l.from.pin == "total"), "{:?}", doc.graph.links);
    assert!(doc.problems.is_empty(), "{:?}", doc.problems);
    doc.set_output_type("function", 0, "float").unwrap();
    assert!(doc.problems.iter().any(|(_, message)| message.contains("into")), "{:?}", doc.problems);
    assert!(doc.undo());
    assert!(doc.problems.is_empty(), "undo restores the old type: {:?}", doc.problems);
    let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
}
