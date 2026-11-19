//! `seal-agent [--prefabs DIR] [scene.sealscene]`: an MCP server on stdin and stdout.
//! DIR holds prefab or class files (`*.sealprefab`, `*.sealclass`).
use std::io::{BufRead, Write};

use seal_agent::Session;
use seal_ecs::{Prefab, PrefabLibrary, World};

fn fail(message: String) -> ! {
    eprintln!("seal-agent: {message}");
    std::process::exit(2);
}

fn load_prefabs(dir: &str) -> PrefabLibrary {
    let mut library = PrefabLibrary::new();
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| fail(format!("{dir}: {e}")));
    let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if !matches!(path.extension().and_then(|e| e.to_str()), Some("sealprefab" | "sealclass")) {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| fail(format!("{}: {e}", path.display())));
        let prefab =
            Prefab::from_text(&text).unwrap_or_else(|e| fail(format!("{}: {e}", path.display())));
        library.insert(prefab);
    }
    let names: Vec<String> = library.names().map(String::from).collect();
    for name in &names {
        if let Err(message) = library.resolve(name) {
            fail(format!("prefab {name}: {message}"));
        }
    }
    eprintln!("seal-agent: {} prefabs from {dir}", names.len());
    library
}

fn main() {
    let mut world = World::new();
    seal_scene::register_scene(&mut world);
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut library = PrefabLibrary::new();
    if let Some(i) = args.iter().position(|a| a == "--prefabs") {
        let dir =
            args.get(i + 1).cloned().unwrap_or_else(|| fail("--prefabs needs a folder".into()));
        library = load_prefabs(&dir);
        args.drain(i..i + 2);
    }
    if let Some(path) = args.first().cloned() {
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| world.load_scene_with(&library, &t))
        {
            Ok(ids) => eprintln!("seal-agent: loaded {} entities from {path}", ids.len()),
            Err(message) => {
                eprintln!("seal-agent: {path}: {message}");
                std::process::exit(2);
            }
        }
    }
    let mut session = Session::new(world);
    session.library = library;
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str(&line) {
            Ok(message) => session.handle(&message),
            Err(e) => Some(serde_json::json!({
                "jsonrpc": "2.0", "id": null,
                "error": { "code": -32700, "message": format!("parse error: {e}") },
            })),
        };
        if let Some(reply) = reply {
            if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
}
