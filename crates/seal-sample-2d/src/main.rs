//! Seal Hop. Files next to the program (all optional):
//! `data/input.map` rebinds controls, `data/level.txt` replaces the level.
//! Flags: `--record FILE` saves your input, `--replay FILE` plays it back,
//! `--scale N` sizes the window, `--vsync`, `--mute`, `--demo` lets a bot play.

use std::path::PathBuf;

use seal_app::input::InputRecording;
use seal_app::window::{run, RunOptions};
use seal_sample_2d::{config, SealHop, DEFAULT_LEVEL};

fn data_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("data")))
        .unwrap_or_else(|| PathBuf::from("data"))
}

fn main() {
    let mut options = RunOptions { scale: 2, ..Default::default() };
    let mut demo = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--record" => options.record = args.next().map(PathBuf::from),
            "--replay" => {
                let path = args.next().expect("--replay FILE");
                let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
                options.replay =
                    Some(InputRecording::parse(&text).unwrap_or_else(|e| panic!("{path}: {e:?}")));
            }
            "--scale" => options.scale = args.next().and_then(|s| s.parse().ok()).unwrap_or(2),
            "--vsync" => options.vsync = true,
            "--mute" => options.mute = true,
            "--demo" => demo = true,
            other => {
                eprintln!("unknown option {other}");
                std::process::exit(2);
            }
        }
    }
    let data = data_dir();
    options.input_override = std::fs::read_to_string(data.join("input.map")).ok();
    let level = std::fs::read_to_string(data.join("level.txt"))
        .unwrap_or_else(|_| DEFAULT_LEVEL.to_string());
    if let Err(e) = run(config(), options, move || {
        let mut game = SealHop::new(&level);
        if demo {
            game.demo = Some(Default::default());
        }
        game
    }) {
        eprintln!("Seal Hop: {e}");
        std::process::exit(1);
    }
}
