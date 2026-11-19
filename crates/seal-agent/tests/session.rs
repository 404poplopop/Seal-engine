use seal_agent::Session;
use seal_ecs::World;
use serde_json::{json, Value};

fn session() -> Session {
    let mut w = World::new();
    w.register_core();
    Session::new(w)
}

fn nums(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|n| n.as_f64().unwrap()).collect()
}

fn call(s: &mut Session, tool: &str, args: Value) -> Result<Value, String> {
    s.call(tool, &args)
}

#[test]
fn speaks_mcp() {
    let mut s = session();
    let init = s
        .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }))
        .unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "seal-agent");
    assert!(s
        .handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .is_none());
    let list = s.handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).unwrap();
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"batch") && names.contains(&"set_field"));
    let bad = s.handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "get_entity", "arguments": { "id": "nope" } } })).unwrap();
    assert_eq!(bad["result"]["isError"], true);
    assert!(bad["result"]["content"][0]["text"].as_str().unwrap().contains("5v1"));
    let unknown =
        s.handle(&json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/list" })).unwrap();
    assert_eq!(unknown["error"]["code"], -32601);
}

#[test]
fn builds_and_edits_a_scene() {
    let mut s = session();
    let room = call(&mut s, "spawn", json!({ "components": { "name": "Room" } })).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let lamp = call(
        &mut s,
        "spawn",
        json!({
            "components": { "name": "Lamp", "transform": { "translation": [0, 250, 0] } },
            "parent": room,
        }),
    )
    .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let got = call(&mut s, "get_entity", json!({ "id": lamp })).unwrap();
    assert_eq!(got["parent"], room.as_str());
    assert_eq!(
        nums(&got["components"]["transform"]["scale"]),
        [1.0; 3],
        "missing fields take defaults"
    );
    let set = call(
        &mut s,
        "set_field",
        json!({ "id": lamp, "component": "transform", "field": "scale", "value": [2, 2, 2] }),
    )
    .unwrap();
    assert_eq!(nums(&set["value"]["translation"]), [0.0, 250.0, 0.0]);
    let found =
        call(&mut s, "find_entities", json!({ "with": ["transform"], "name_contains": "lam" }))
            .unwrap();
    assert_eq!(found["total"], 1);
    let err = call(&mut s, "set_parent", json!({ "id": room, "parent": lamp })).unwrap_err();
    assert!(err.contains("WouldCycle"));
    // Undo the scale change, then the lamp itself.
    assert_eq!(call(&mut s, "undo", json!({})).unwrap()["undone"], true);
    let got = call(&mut s, "get_entity", json!({ "id": lamp })).unwrap();
    assert_eq!(nums(&got["components"]["transform"]["scale"]), [1.0; 3]);
    call(&mut s, "undo", json!({})).unwrap();
    assert!(call(&mut s, "get_entity", json!({ "id": lamp })).is_err());
    call(&mut s, "redo", json!({})).unwrap();
    assert!(call(&mut s, "get_entity", json!({ "id": lamp })).is_ok());
}

#[test]
fn a_failed_step_leaves_no_trace() {
    let mut s = session();
    let before = call(&mut s, "save_scene", json!({})).unwrap()["text"].clone();
    let err = call(
        &mut s,
        "batch",
        json!({ "steps": [
        { "tool": "spawn", "arguments": { "components": { "name": "A" } } },
        { "tool": "spawn", "arguments": { "components": { "name": "B" } } },
        { "tool": "spawn", "arguments": { "components": { "transform": { "scale": "big" } } } },
    ] }),
    )
    .unwrap_err();
    assert!(err.contains("step 2 (spawn)") && err.contains("nothing was changed"), "{err}");
    assert_eq!(call(&mut s, "save_scene", json!({})).unwrap()["text"], before);
    assert_eq!(call(&mut s, "world_info", json!({})).unwrap()["can_undo"], false);
    // A good batch is one undo step.
    call(
        &mut s,
        "batch",
        json!({ "steps": [
        { "tool": "spawn", "arguments": { "components": { "name": "A" } } },
        { "tool": "spawn", "arguments": { "components": { "name": "B" } } },
    ] }),
    )
    .unwrap();
    assert_eq!(call(&mut s, "world_info", json!({})).unwrap()["entities"], 2);
    call(&mut s, "undo", json!({})).unwrap();
    assert_eq!(call(&mut s, "world_info", json!({})).unwrap()["entities"], 0);
}

#[test]
fn scenes_go_out_and_come_back() {
    let mut s = session();
    call(
        &mut s,
        "spawn",
        json!({ "components": { "name": "Crate", "transform": { "translation": [1, 2, 3] } } }),
    )
    .unwrap();
    let text = call(&mut s, "save_scene", json!({})).unwrap()["text"].as_str().unwrap().to_string();
    let mut other = session();
    let ids = call(&mut other, "load_scene", json!({ "text": text })).unwrap();
    let id = ids["ids"][0].as_str().unwrap();
    let got = call(&mut other, "get_entity", json!({ "id": id })).unwrap();
    assert_eq!(got["components"]["name"], "Crate");
}

#[test]
fn the_binary_answers_over_stdio() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new(env!("CARGO_BIN_EXE_seal-agent"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{}}}}"#).unwrap();
    writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"spawn","arguments":{{"components":{{"name":"Hi"}}}}}}}}"#).unwrap();
    writeln!(stdin, "not json").unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let lines: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[1]["result"]["isError"], false);
    assert_eq!(lines[2]["error"]["code"], -32700);
}

#[test]
fn works_with_prefabs_in_both_vocabularies() {
    let mut s = session();
    call(&mut s, "add_prefab", json!({ "text": "seal_class = 1\nclass = \"Unit\"\ndefaults = { name = \"Unit\", transform = { scale = [1, 1, 1] } }\n" })).unwrap();
    let added = call(&mut s, "add_prefab", json!({ "text": "seal_prefab = 1\nprefab = \"Guard\"\nbase = \"Unit\"\ncomponents = { name = \"Guard\" }\nchildren = [{ slot = \"hat\", components = { name = \"Hat\" } }]\n" })).unwrap();
    assert_eq!(added["chain"], json!(["Unit", "Guard"]));
    assert!(call(
        &mut s,
        "add_prefab",
        json!({ "text": "seal_prefab = 1\nprefab = \"Loop\"\nbase = \"Loop\"\n" })
    )
    .is_err());
    assert_eq!(
        call(&mut s, "list_prefabs", json!({})).unwrap().as_array().unwrap().len(),
        2,
        "a bad prefab is not kept"
    );

    let guard = call(&mut s, "instantiate", json!({ "prefab": "Guard" })).unwrap();
    let id = guard["id"].as_str().unwrap().to_string();
    assert_eq!(guard["children"].as_array().unwrap().len(), 1);
    assert_eq!(call(&mut s, "get_overrides", json!({ "id": id })).unwrap()["overrides"], json!({}));
    call(
        &mut s,
        "set_field",
        json!({ "id": id, "component": "transform", "field": "scale", "value": [2, 2, 2] }),
    )
    .unwrap();
    let o = call(&mut s, "get_overrides", json!({ "id": id })).unwrap();
    assert_eq!(nums(&o["overrides"]["transform"]["scale"]), [2.0, 2.0, 2.0]);

    let classic = call(&mut s, "save_scene", json!({ "compact": true, "vocabulary": "classic" }))
        .unwrap()["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        classic.contains("instances = ")
            && classic.contains("class = \"Guard\"")
            && !classic.contains("Hat"),
        "{classic}"
    );

    call(&mut s, "revert_to_prefab", json!({ "id": id, "component": "transform" })).unwrap();
    assert_eq!(call(&mut s, "get_overrides", json!({ "id": id })).unwrap()["overrides"], json!({}));
    call(&mut s, "undo", json!({})).unwrap();
    call(&mut s, "undo", json!({})).unwrap();
    call(&mut s, "undo", json!({})).unwrap();
    assert_eq!(
        call(&mut s, "world_info", json!({})).unwrap()["entities"],
        0,
        "undo removes the instance and its parts"
    );

    let loaded = call(&mut s, "load_scene", json!({ "text": classic })).unwrap();
    assert_eq!(loaded["ids"].as_array().unwrap().len(), 2, "the hat comes back from the prefab");
}
