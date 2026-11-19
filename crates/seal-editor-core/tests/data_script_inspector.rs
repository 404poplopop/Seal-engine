//! Data assets in the Inspector and graph script inspector layouts.

use seal_editor_core::{create_project, script_inspector, EditorSession, Layout, CommandArgs};
use std::fs;

#[test]
fn data_inspector_commands_round_trip_and_undo() {
    let dir = std::env::temp_dir().join(format!("seal-data-inspector-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    create_project(&dir, "Game").unwrap();
    let type_text = "\
seal_datatype = 1
name = Pack
field num number default=1
field tag text
field ok bool default=no
field mode choice options=on|off default=on
";
    fs::create_dir_all(dir.join("data/types")).unwrap();
    fs::write(dir.join("data/types/pack.datatype"), type_text).unwrap();
    let mut session = EditorSession::new(Layout::default());
    session
        .dispatch("file.open_project", &CommandArgs::new(&[("path", dir.to_str().unwrap())]))
        .unwrap();
    session
        .dispatch(
            "asset.data_new",
            &CommandArgs::new(&[
                ("path", "data/item"),
                ("type", "data/types/pack.datatype"),
                ("unique", "yes"),
            ]),
        )
        .unwrap();
    let rel = "data/item.data";
    let mut set = |field: &str, value: &str| {
        session
            .dispatch(
                "asset.data_set",
                &CommandArgs::new(&[("path", rel), ("field", field), ("value", value)]),
            )
            .unwrap();
    };
    set("num", "9");
    set("tag", "hero");
    set("ok", "yes");
    set("mode", "off");
    let text = fs::read_to_string(dir.join(rel)).unwrap();
    assert!(text.contains("value num = 9"));
    assert!(text.contains("value tag = hero"));
    assert!(text.contains("value ok = yes"));
    assert!(text.contains("value mode = off"));
    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    let undone = fs::read_to_string(dir.join(rel)).unwrap();
    assert!(!undone.contains("value mode = off"));
}

#[test]
fn custom_layout_order_hide_and_range() {
    use seal_graph::inspector_layout::{InspectorFieldRule, InspectorLayout, plan_rows};
    let mut layout = InspectorLayout::default();
    layout.fields.push(InspectorFieldRule {
        field: "walk_speed".into(),
        section: None,
        order: 0,
        hidden: false,
        min: Some(0.0),
        max: Some(10.0),
        help: Some("Walk speed".into()),
    });
    layout.fields.push(InspectorFieldRule {
        field: "bounce".into(),
        section: None,
        order: 2,
        hidden: true,
        min: None,
        max: None,
        help: None,
    });
    let plan = plan_rows(script_inspector::SAMPLE_FIELDS, Some(&layout));
    assert!(plan.iter().any(|r| r.field == "bounce" && r.hidden));
    assert_eq!(
        plan.iter().find(|r| r.field == "walk_speed").unwrap().min,
        Some(0.0)
    );
    let baseline = script_inspector::plan_sample(None);
    assert_eq!(baseline.len(), script_inspector::SAMPLE_FIELDS.len());
}

#[test]
fn damaged_datatype_still_refused() {
    assert!(seal_world::data_asset::DataTypeDef::parse("not a datatype").is_err());
}