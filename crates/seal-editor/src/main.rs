#![cfg_attr(windows, windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod sound;

use std::env;
use std::process::ExitCode;

use seal_editor_core::{EditorLock, USAGE};
use seal_log::{install, Level, LogMode, Logger};

fn main() -> ExitCode {
    seal_editor_shell::mark_process_start();
    let raw: Vec<String> = env::args().skip(1).collect();
    if raw.first().map(String::as_str) == Some("mcp") {
        let port = raw
            .iter()
            .position(|a| a == "--port")
            .and_then(|i| raw.get(i + 1))
            .and_then(|p| p.parse().ok())
            .unwrap_or_else(seal_editor_core::agent_link::port);
        let stdin = std::io::stdin();
        return match seal_editor_core::agent_link::bridge(port, stdin.lock(), std::io::stdout()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("seal-editor mcp: {err}");
                ExitCode::from(1)
            }
        };
    }
    let request = match seal_editor_core::parse_cli(env::args().skip(1)) {
        Ok(request) => request,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(2);
        }
    };
    let seal_editor_core::CliRequest::Launch(options) = request else {
        return match seal_editor_core::execute_cli(&request) {
            Ok(output) => {
                if output.text.ends_with('\n') {
                    print!("{}", output.text);
                } else if !output.text.is_empty() {
                    println!("{}", output.text);
                }
                if output.success {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(err) => {
                eprintln!("{err}");
                ExitCode::from(2)
            }
        };
    };
    if options.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if options.self_check {
        return self_check();
    }
    if let Some(image) = options.screenshot.clone() {
        let dir = env::temp_dir().join(format!("seal-screenshot-{}", std::process::id()));
        let mut shot = options.clone();
        shot.screenshot = None;
        shot.no_restore = true;
        shot.reset_layout = true;
        let result = seal_editor_shell::run_screenshot(&dir, image, shot);
        let _ = std::fs::remove_dir_all(&dir);
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("{err}");
                ExitCode::FAILURE
            }
        };
    }
    let mut options = options;
    // A packaged game: game.txt beside the program names the project to play.
    let packaged = env::current_exe()
        .ok()
        .and_then(|exe| seal_editor_core::package::game_project_beside(&exe));
    if let Some(project) = packaged {
        if options.project.is_none() {
            options.project = Some(project);
            options.game = true;
        }
    }
    if options.run_tests {
        let Some(project) = options.project.clone() else {
            eprintln!("--run-tests needs a project path");
            return ExitCode::from(2);
        };
        return match seal_editor_core::functional_tests::run_for_ci(
            &project,
            options.tests_report.as_deref(),
        ) {
            Ok((ok, text)) => {
                print!("{text}");
                if ok {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(err) => {
                eprintln!("{err}");
                ExitCode::from(2)
            }
        };
    }
    if options.game {
        // A packaged game draws its small pictures from the atlas pages packed with it.
        if let Some(project) = options.project.as_deref() {
            seal_ui::sprites::use_atlas(project);
        }
        options.no_restore = true;
        options.reset_layout = true;
        if options.user_data.is_none() {
            // A packaged game's project folder is always `data`, so name the
            // state after the game folder instead and two games never share it.
            let name = options
                .project
                .as_deref()
                .map(|p| match p.parent() {
                    Some(dir) if dir.join(seal_editor_core::package::GAME_MARKER).is_file() => dir,
                    _ => p,
                })
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            options.user_data = Some(env::temp_dir().join(format!("seal-game-{name}")));
        }
    }
    let user_data = options.user_data.clone().unwrap_or_else(seal_editor_core::user_state_dir);
    if options.game {
        let _ = std::fs::create_dir_all(&user_data);
    }
    match EditorLock::acquire(&user_data) {
        Ok(lock) => std::mem::forget(lock),
        Err(err) => {
            match seal_editor_core::write_focus_request(&user_data) {
                Ok(()) => eprintln!("{err}"),
                Err(write_err) => eprintln!("{err}; {write_err}"),
            }
            return ExitCode::FAILURE;
        }
    }
    let mut logger =
        Logger::new(LogMode::Editor).with_file(user_data.join("logs").join("editor.log"));
    if let Some(name) = &options.log_level {
        match Level::parse(name) {
            Some(level) => logger = logger.with_ceiling(level),
            None => {
                eprintln!("unknown log level: {name}");
                return ExitCode::from(2);
            }
        }
    }
    install(logger);
    seal_editor_core::install_panic_hook(user_data.join("logs"));
    seal_editor_shell::set_sound_sink(Box::new(|project, world, cues| sound::play(project, world, cues)));
    seal_editor_shell::set_music_sink(Box::new(sound::music));
    seal_editor_shell::set_preview_sink(Box::new(sound::preview));
    seal_editor_shell::set_mix_sink(Box::new(sound::apply_project_audio));
    match seal_editor_shell::run_with(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn self_check() -> ExitCode {
    let dir = std::env::temp_dir().join(format!("seal-self-check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(err) = std::fs::create_dir_all(&dir) {
        eprintln!("{err}");
        return ExitCode::FAILURE;
    }
    if let Err(err) = seal_editor_shell::check_persistence(&dir) {
        eprintln!("{err}");
        return ExitCode::FAILURE;
    }
    let text = seal_ui::commit_ime("a", "b");
    if text != "ab" {
        eprintln!("text input check failed");
        return ExitCode::FAILURE;
    }
    if !seal_ui::accesskit_has_nodes("Welcome") {
        eprintln!("accessibility tree was empty");
        return ExitCode::FAILURE;
    }
    let layout = seal_editor_core::Layout::default();
    if seal_ui::layout_roundtrip(&layout).root != layout.root {
        eprintln!("dock roundtrip failed");
        return ExitCode::FAILURE;
    }
    let (background, _, _) = seal_editor_core::theme_rgb("contrast");
    if background != [0, 0, 0] {
        eprintln!("theme check failed");
        return ExitCode::FAILURE;
    }
    if seal_editor_core::write_diagnostics(&dir, "self-check").is_err() {
        eprintln!("diagnostics check failed");
        return ExitCode::FAILURE;
    }
    let _ = std::fs::remove_dir_all(&dir);
    println!("self-check ok");
    ExitCode::SUCCESS
}
