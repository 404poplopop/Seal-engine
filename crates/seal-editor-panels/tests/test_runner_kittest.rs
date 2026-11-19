//! egui_kittest drives the Test Runner: Run all, then pass and fail are on screen.

use std::fs;
use std::path::PathBuf;

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use seal_editor_core::functional_tests::{AssertNode, Compare, TestScript};
use seal_editor_core::{CommandArgs, EditorSession, Layout};
use seal_editor_panels::PanelEvent;

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-kittest-runner-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("project.seal"),
        "schema_version = 1\nname = \"Tests\"\nid = \"11111111-0000-7000-8000-000000000023\"\ncreated = 1\neditor_version = \"0\"\nmodules = []\nsettings = {}\nignore = [\".seal-editor/\"]\n",
    )
    .unwrap();
    fs::write(dir.join("scene.seal"), "scene\n").unwrap();
    fs::write(dir.join("marker.scene"), seal_editor_core::World::empty_scene_text("Marker"))
        .unwrap();
    fs::create_dir_all(dir.join("tests")).unwrap();
    let pass = TestScript {
        name: "marker holds".into(),
        scene: "marker.scene".into(),
        frames: 1,
        nodes: vec![AssertNode {
            id: 1,
            object: "Marker".into(),
            property: "name".into(),
            op: Compare::Eq,
            expected: "Marker".into(),
        }],
    };
    let fail = TestScript {
        name: "marker renamed".into(),
        scene: "marker.scene".into(),
        frames: 1,
        nodes: vec![AssertNode {
            id: 1,
            object: "Marker".into(),
            property: "name".into(),
            op: Compare::Eq,
            expected: "Other".into(),
        }],
    };
    fs::write(dir.join("tests/holds.sealtest"), pass.to_text()).unwrap();
    fs::write(dir.join("tests/misses.sealtest"), fail.to_text()).unwrap();
    dir
}

fn open(dir: &std::path::Path) -> EditorSession {
    let mut session = EditorSession::new(Layout::default());
    let path = dir.to_string_lossy().to_string();
    session.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    session
}

fn apply(session: &mut EditorSession, event: PanelEvent) {
    match event {
        PanelEvent::Command(id, args) => {
            let _ = session.dispatch(&id, &args);
        }
        PanelEvent::Commands(list) => {
            for (id, args) in list {
                let _ = session.dispatch(&id, &args);
            }
        }
        PanelEvent::None | PanelEvent::Scratch(_) => {}
    }
}

#[test]
fn kittest_runs_the_panel_and_shows_the_failure() {
    let dir = scratch();
    let mut harness =
        Harness::<EditorSession>::builder().with_size(egui::vec2(960.0, 1400.0)).build_ui_state(
            |ui, session| {
                let mut frame = seal_ui::PanelFrame::new(ui);
                let event = seal_editor_panels::show(&mut frame, "test-runner", session);
                apply(session, event);
            },
            open(&dir),
        );
    harness.get_by_label("Run all").click();
    harness.run();
    harness.run();
    let stored = harness
        .state()
        .models
        .panel_states
        .get("test-runner.results")
        .cloned()
        .expect("results saved");
    let report = seal_editor_core::functional_tests::SuiteReport::from_state(&stored).unwrap();
    assert!(report.cases.iter().any(|case| case.passed && case.name == "marker holds"));
    let failed = report.cases.iter().find(|case| case.name == "marker renamed").unwrap();
    assert!(!failed.passed, "{}", failed.message);
    assert!(failed.message.contains("got Marker"), "{}", failed.message);
    assert!(harness.get_all_by_label_contains("got Marker").next().is_some());
    let _ = fs::remove_dir_all(&dir);
}
