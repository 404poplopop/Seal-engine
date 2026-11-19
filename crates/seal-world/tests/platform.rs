//! Encrypted saves, the built-in dialog, and the cloud queue.

use seal_world::dialog_ui::{self, Ask};
use seal_world::savegame::{self, SaveGame};
use seal_world::{PlayMode, World};

fn project(name: &str, settings: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-platform-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    std::fs::write(
        dir.join("project.seal"),
        format!("schema_version = 1\nname = \"P\"\nsettings = {{ {settings} }}\n"),
    )
    .unwrap();
    dir
}

fn playing(dir: &std::path::Path, graph: &str) -> World {
    std::fs::write(dir.join("logic/game.graph"), graph).unwrap();
    let mut world = World::starter();
    world.entities[0].script = "logic/game.graph".into();
    world.project_dir = Some(dir.to_path_buf());
    world.play();
    world.game_screens.resize([800.0, 600.0]);
    assert_eq!(world.mode, PlayMode::Playing);
    world
}

#[test]
fn a_save_is_sealed_on_disk_and_a_change_is_refused() {
    let dir = project("enc", "save_encrypt = true\nencryption_key = \"project key\"");
    let mut world = World::starter();
    world.project_dir = Some(dir.clone());
    world.play_time = 3.0;
    world.save_game(&dir, "Slot 1").unwrap();
    let path = dir.join("saved/games/Slot 1.sealsave");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("seal enc 1"), "{text}");
    assert!(!text.contains("seal_save"), "the plain save must not sit on disk");
    let loaded = savegame::read(&dir, "Slot 1").unwrap();
    assert_eq!(loaded.slot, "Slot 1");
    assert!(loaded.play_time > 0.0);

    let mut damaged = text.clone();
    // A character in the middle of the sealed text: the last base64 character can carry
    // only padding bits, and changing those leaves the bytes the same.
    let at = (damaged.len() / 2..damaged.len())
        .find(|&i| damaged.as_bytes()[i].is_ascii_alphanumeric())
        .unwrap();
    damaged.replace_range(at..at + 1, if damaged.as_bytes()[at] == b'A' { "B" } else { "A" });
    std::fs::write(&path, damaged).unwrap();
    let err = savegame::read(&dir, "Slot 1").unwrap_err();
    assert!(err.contains("changed or the project key"), "{err}");

    std::fs::write(&path, "seal enc 1\nthis is not a seal\n").unwrap();
    assert!(savegame::read(&dir, "Slot 1").is_err());
    let listed = savegame::list(&dir);
    assert!(listed[0].problem.is_some(), "{listed:?}");

    // Turning encryption off still reads an older plain save.
    let plain = SaveGame {
        slot: "Plain".into(),
        format: 1,
        game_version: 1,
        saved_at: 5,
        entities: world.entities.clone(),
        ..SaveGame::default()
    };
    // write() would seal it while the flag is on, so write the text directly.
    let plain_path = dir.join("saved/games/Plain.sealsave");
    std::fs::write(&plain_path, plain.to_text()).unwrap();
    assert_eq!(savegame::read(&dir, "Plain").unwrap().saved_at, 5);
}

#[test]
fn the_dialog_is_themable_and_the_answer_is_an_event() {
    let custom = seal_world::gameui::Theme { button: [1.0, 0.0, 0.4, 1.0], ..Default::default() };
    let ask = Ask {
        gen: 2,
        kind: "confirm".into(),
        title: "Quit".into(),
        message: "Leave?".into(),
        placeholder: String::new(),
        theme: String::new(),
    };
    let mut screens = seal_world::gameui::Screens::default();
    screens.resize([800.0, 600.0]);
    dialog_ui::install(&mut screens, &ask, None).unwrap();
    screens.shown[0].1.theme = custom.clone();
    let draws = screens.draws(0.0);
    assert!(
        draws.iter().any(
            |d| matches!(d, seal_world::gameui::Draw::Fill { color, .. } if *color == custom.button)
        ),
        "the dialog uses the theme button color"
    );

    let dir = project("dialog", "");
    let graph = r#"
schema_version = 1
seal_graph = "dialog"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "show_dialog", defaults = { kind = "confirm", title = "Quit", message = "Leave?", placeholder = "", theme = "" } },
  { id = 3, kind = "event", name = "dialog_answered" },
  { id = 4, kind = "dialog_answer" },
  { id = 5, kind = "log" },
]
links = [
  "1.then -> 2.in",
  "3.then -> 5.in", "4.button -> 5.message",
]
"#;
    let mut world = playing(&dir, graph);
    world.step(0.1);
    assert!(world.game_screens.shown.iter().any(|(w, _)| w == dialog_ui::WIDGET));
    assert!(world.game_screens.key("Enter"), "Yes is focused");
    world.step(0.1);
    assert!(
        world.play_log.iter().any(|l| l == "yes"),
        "dialog_answered should log the button: {:?}",
        world.play_log
    );
}

#[test]
fn cloud_queue_uses_a_mock_provider_and_survives_on_disk() {
    let dir = project("cloud", "cloud_save = true\ncloud_provider = \"script\"");
    let mut world = World::starter();
    world.project_dir = Some(dir.clone());
    world.save_game(&dir, "Slot 1").unwrap();
    assert!(dir.join("saved/games/Slot 1.sealsave").is_file());
    // The upload is queued on the next play step. Drive the queue directly so
    // the conflict rule is checked without a network.
    let body = std::fs::read_to_string(dir.join("saved/games/Slot 1.sealsave")).unwrap();
    let mut queue = seal_world::cloud::Queue::default();
    seal_world::cloud::enqueue_upload(&mut queue, "Slot 1", 10, body);
    seal_world::cloud::store(&dir, &queue).unwrap();
    let loaded = seal_world::cloud::load(&dir).unwrap();
    assert_eq!(loaded.jobs.len(), 1);
    assert!(seal_world::cloud::parse_queue("nope\n").is_err());

    struct Mock {
        remote_at: u64,
        offline: bool,
    }
    impl seal_world::cloud::Api for Mock {
        fn upload(
            &mut self,
            _: &str,
            saved_at: u64,
            _: &str,
        ) -> Result<seal_world::cloud::Up, seal_world::cloud::Fail> {
            if self.offline {
                return Err(seal_world::cloud::Fail::Offline("offline".into()));
            }
            if self.remote_at > saved_at {
                Ok(seal_world::cloud::Up::RemoteNewer(self.remote_at))
            } else {
                Ok(seal_world::cloud::Up::Stored)
            }
        }
        fn download(
            &mut self,
            _: &str,
        ) -> Result<seal_world::cloud::Down, seal_world::cloud::Fail> {
            if self.offline {
                return Err(seal_world::cloud::Fail::Offline("offline".into()));
            }
            Ok(seal_world::cloud::Down::Found {
                saved_at: self.remote_at,
                body: "remote-save".into(),
            })
        }
    }
    let mut mock = Mock { remote_at: 50, offline: true };
    let mut q = loaded;
    let notes =
        seal_world::cloud::flush(&mut q, &|_| Some(10), seal_world::cloud::Mode::Custom(&mut mock));
    assert!(matches!(&notes[0], seal_world::cloud::Note::Waiting { .. }));
    assert_eq!(q.jobs.len(), 1, "offline keeps the job");
    mock.offline = false;
    let notes =
        seal_world::cloud::flush(&mut q, &|_| Some(10), seal_world::cloud::Mode::Custom(&mut mock));
    assert!(
        matches!(&notes[0], seal_world::cloud::Note::Conflict { kept, .. } if kept == "remote")
    );
}
