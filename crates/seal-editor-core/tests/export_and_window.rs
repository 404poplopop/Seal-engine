//! Export presets and the game window settings the runner reads.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use seal_editor_core::export_preset::{
    parse_book, planned_files, remove_stale, to_text, ExportBook, ExportPreset, ExportStep,
};
use seal_editor_core::window_game::{self, frame_wait, parse_runner, GameWindow};
use seal_editor_core::{CommandArgs, EditorSession, Layout};

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-export-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn sample(root: &std::path::Path) -> (PathBuf, PathBuf) {
    let project = root.join("Hop");
    fs::create_dir_all(project.join("sprites")).unwrap();
    fs::create_dir_all(project.join("notes")).unwrap();
    fs::create_dir_all(project.join("legal")).unwrap();
    let id = format!("{:08x}-0000-7000-9044-000000000016", std::process::id());
    fs::write(
        project.join("project.seal"),
        format!(
            "schema_version = 1\nname = \"Hop\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
        ),
    )
    .unwrap();
    fs::write(project.join("scene.seal"), "scene\n").unwrap();
    fs::write(project.join("sprites").join("hero.png"), [1u8, 2, 3]).unwrap();
    fs::write(project.join("notes").join("credits.txt"), "thanks\n").unwrap();
    fs::write(project.join("notes.bak"), "old\n").unwrap();
    fs::write(project.join("legal").join("eula.txt"), "eula\n").unwrap();
    let exe = root.join("player.sh");
    fs::write(&exe, "#!/bin/sh\ntouch launched.txt\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = fs::metadata(&exe).unwrap().permissions();
        perm.set_mode(0o755);
        fs::set_permissions(&exe, perm).unwrap();
    }
    (project, exe)
}

fn full_book() -> ExportBook {
    let mut preset = ExportPreset::named("Windows", "");
    preset.include = vec!["*.seal".into(), "sprites/**".into()];
    preset.exclude = vec!["*.bak".into()];
    preset.extra = vec![("notes/credits.txt".into(), "credits.txt".into())];
    preset.clean = true;
    preset.pre = vec![ExportStep::Copy { from: "legal/eula.txt".into(), to: "eula.txt".into() }];
    preset.post = vec![ExportStep::Zip { folder: String::new() }];
    ExportBook { selected: "Windows".into(), presets: vec![preset] }
}

#[test]
fn presets_round_trip() {
    let book = full_book();
    let again = parse_book(&to_text(&book)).unwrap();
    assert_eq!(again, book);
}

#[test]
fn include_exclude_and_extra_files_list_what_ships() {
    let preset = &full_book().presets[0];
    let files = [
        "scene.seal".into(),
        "sprites/hero.png".into(),
        "notes/credits.txt".into(),
        "notes.bak".into(),
        "legal/eula.txt".into(),
    ];
    assert_eq!(
        planned_files(&files, preset),
        vec![
            "credits.txt".to_string(),
            "data/scene.seal".to_string(),
            "data/sprites/hero.png".to_string(),
        ]
    );
}

#[test]
fn clean_removes_stale_files() {
    let root = scratch("stale");
    let out = root.join("out");
    fs::create_dir_all(out.join("data")).unwrap();
    fs::write(out.join("stale.txt"), "old").unwrap();
    fs::write(out.join("data").join("scene.seal"), "scene").unwrap();
    let removed = remove_stale(&out, &["data/scene.seal".into()]).unwrap();
    assert_eq!(removed, vec!["stale.txt".to_string()]);
    assert!(!out.join("stale.txt").exists());
    assert!(out.join("data").join("scene.seal").is_file());
}

#[test]
fn damaged_presets_are_refused() {
    assert!(parse_book("selected = 1\n").is_err());
    assert!(parse_book("presets = [{ name = \"A\", profile = \"fast\" }]\n").is_err());
    assert!(parse_book("presets = [{ name = \"A\", clean = \"yes\" }]\n").is_err());
    assert!(parse_book(
        "presets = [{ name = \"A\", pre = [{ action = \"delete\", from = \"a\", to = \"b\" }] }]\n"
    )
    .is_err());
    assert!(parse_book("presets = [{ name = \"A\", extra = [{ from = \"only\" }] }]\n").is_err());
    assert!(parse_book("selected = \"Missing\"\npresets = []\n").is_err());
    assert!(parse_book("presets = [{ folder = \"out\" }]\n").is_err());
}

#[test]
fn an_export_writes_the_file_list_runs_steps_and_can_launch() {
    let root = scratch("ship");
    let (project, exe) = sample(&root);
    let mut preset = full_book().presets.remove(0);
    preset.folder = root.join("Hop Game").display().to_string();
    let window = GameWindow {
        title: "Hop".into(),
        icon: "sprites/hero.png".into(),
        mode: "fullscreen".into(),
        width: 800,
        height: 600,
        resizable: false,
        always_on_top: true,
        vsync: false,
        target_fps: 30,
        hdr_output: false,
        paper_white_nits: 203.0,
    };
    let report = seal_editor_core::export_preset::export_preset(
        &project,
        "Hop",
        &exe,
        &preset,
        &window.runner_text("Hop"),
        &seal_editor_core::render_runner_text(&seal_editor_core::RenderQuality::default()),
    )
    .unwrap();
    let out = &report.folder;
    assert!(out.join("data").join("scene.seal").is_file());
    assert!(out.join("data").join("sprites").join("hero.png").is_file());
    assert!(!out.join("data").join("notes.bak").exists());
    assert!(!out.join("data").join("notes").exists(), "credits is copied as an extra file");
    assert_eq!(fs::read_to_string(out.join("credits.txt")).unwrap(), "thanks\n");
    assert_eq!(fs::read_to_string(out.join("eula.txt")).unwrap(), "eula\n");
    let marker = fs::read_to_string(out.join("game.txt")).unwrap();
    assert!(marker.contains("profile release"));
    let runner = parse_runner(&fs::read_to_string(out.join("window.txt")).unwrap()).unwrap();
    assert_eq!(runner.title, "Hop");
    assert_eq!(runner.mode, "fullscreen");
    assert_eq!((runner.width, runner.height), (800, 600));
    assert!(!runner.resizable && runner.always_on_top && !runner.vsync);
    assert_eq!(runner.target_fps, 30);
    assert_eq!(runner.icon, "sprites/hero.png");
    let zip = fs::read(root.join("Hop Game.zip")).unwrap();
    assert!(zip.windows(b"credits.txt".len()).any(|w| w == b"credits.txt"));
    // A second export with clean off leaves a file the preset does not write.
    fs::write(out.join("stale.txt"), "keep").unwrap();
    preset.clean = false;
    seal_editor_core::export_preset::export_preset(
        &project,
        "Hop",
        &exe,
        &preset,
        &window.runner_text("Hop"),
        "",
    )
    .unwrap();
    assert!(out.join("stale.txt").is_file(), "clean off leaves files already there");
    preset.clean = true;
    seal_editor_core::export_preset::export_preset(
        &project,
        "Hop",
        &exe,
        &preset,
        &window.runner_text("Hop"),
        "",
    )
    .unwrap();
    assert!(!out.join("stale.txt").exists(), "clean removes files the preset does not write");
    #[cfg(unix)]
    {
        let pid = seal_editor_core::export_preset::launch_game(&report.exe).unwrap();
        assert!(pid > 0);
        let flag = out.join("launched.txt");
        for _ in 0..50 {
            if flag.is_file() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(flag.is_file(), "build and run starts the packaged program");
    }
}

#[test]
fn the_export_commands_save_a_preset_and_package_it() {
    let root = scratch("cmd");
    let (project, exe) = sample(&root);
    let mut session = EditorSession::new(Layout::default());
    let path = project.display().to_string();
    session.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    session
        .dispatch(
            "settings.set",
            &CommandArgs::new(&[("id", "display.title"), ("value", "Hop Window")]),
        )
        .unwrap();
    session
        .dispatch(
            "settings.set",
            &CommandArgs::new(&[("id", "display.target_fps"), ("value", "45")]),
        )
        .unwrap();
    let book = full_book();
    let mut preset = book.presets[0].clone();
    preset.folder = root.join("Out").display().to_string();
    preset.pre.clear();
    preset.post.clear();
    let stored = ExportBook { selected: preset.name.clone(), presets: vec![preset.clone()] };
    session
        .dispatch("export.write", &CommandArgs::new(&[("text", to_text(&stored).as_str())]))
        .unwrap();
    let loaded = seal_editor_core::export_preset::load_book(&project).unwrap();
    assert_eq!(loaded.selected, "Windows");
    let exe_s = exe.display().to_string();
    session
        .dispatch(
            "project.export_go",
            &CommandArgs::new(&[("preset", "Windows"), ("exe", exe_s.as_str())]),
        )
        .unwrap();
    seal_editor_core::import_pipeline::pump_all(&mut session).unwrap();
    let out = root.join("Out");
    assert!(out.join("data").join("scene.seal").is_file());
    let runner = parse_runner(&fs::read_to_string(out.join("window.txt")).unwrap()).unwrap();
    assert_eq!(runner.title, "Hop Window");
    assert_eq!(runner.target_fps, 45);
    session.dispatch("project.export", &CommandArgs::default()).unwrap();
    assert_eq!(session.foundation.focused_panel_id, "export");
}

#[test]
fn window_settings_reach_the_runner_config_and_the_frame_cap_waits() {
    let window = GameWindow {
        title: String::new(),
        icon: "icon.png".into(),
        mode: "borderless".into(),
        width: 640,
        height: 480,
        resizable: true,
        always_on_top: false,
        vsync: true,
        target_fps: 60,
        hdr_output: true,
        paper_white_nits: 250.0,
    };
    let text = window.runner_text("Hop");
    assert!(text.starts_with("title Hop\n"));
    let runner = parse_runner(&text).unwrap();
    assert_eq!(runner.title, "Hop");
    assert_eq!(runner.mode, "borderless");
    assert_eq!(runner.target_fps, 60);
    assert!(parse_runner("mode banana\n").is_err());
    assert!(parse_runner("fps lots\n").is_err());
    assert!(parse_runner("glow 1\n").is_err());
    assert!(parse_runner("").is_err());
    let frame = Duration::from_secs(1) / 60;
    assert_eq!(frame_wait(0, Duration::from_millis(5)), Duration::ZERO);
    assert_eq!(frame_wait(60, Duration::ZERO), frame);
    assert_eq!(frame_wait(60, frame / 2), frame - frame / 2);
    assert_eq!(frame_wait(60, frame), Duration::ZERO);
    assert_eq!(frame_wait(30, Duration::from_millis(100)), Duration::ZERO);
}

#[test]
fn play_in_window_is_remembered_until_stop() {
    let mut session = EditorSession::new(Layout::default());
    session.dispatch("play.start", &CommandArgs::new(&[("window", "1")])).unwrap();
    assert!(window_game::play_in_window(&session));
    session.dispatch("play.stop", &CommandArgs::default()).unwrap();
    assert!(!window_game::play_in_window(&session));
}

#[test]
fn a_project_without_presets_exports_the_default_preset_shown_in_the_panel() {
    let root = scratch("default-preset");
    let (project, exe) = sample(&root);
    let mut session = EditorSession::new(Layout::default());
    let path = project.display().to_string();
    session.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    let name = session.project.as_ref().unwrap().name.clone();
    let exe_s = exe.display().to_string();
    session
        .dispatch(
            "project.export_go",
            &CommandArgs::new(&[("preset", name.as_str()), ("exe", exe_s.as_str())]),
        )
        .unwrap();
    seal_editor_core::import_pipeline::pump_all(&mut session).unwrap();
    let out = seal_editor_core::package::default_package_folder(&project, &name);
    assert!(out.join("data").join("scene.seal").is_file(), "{}", out.display());
}

#[test]
fn new_particle_effects_get_a_free_name_and_default_settings() {
    let root = scratch("fx-new");
    let (project, _) = sample(&root);
    let mut session = EditorSession::new(Layout::default());
    let path = project.display().to_string();
    session.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    let args = CommandArgs::new(&[("path", "fx/effect"), ("unique", "yes")]);
    session.dispatch("asset.fx_new", &args).unwrap();
    session.dispatch("asset.fx_new", &args).unwrap();
    let first = fs::read_to_string(project.join("fx/effect.fx")).unwrap();
    assert_eq!(first, seal_world::particles::EmitterDef::default().to_text());
    assert!(project.join("fx/effect_2.fx").is_file());
    let taken = CommandArgs::new(&[("path", "fx/effect")]);
    assert!(session.dispatch("asset.fx_new", &taken).is_err());
}
