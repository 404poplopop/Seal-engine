//! `seal-run graph check|compile|run|replay`: work with visual logic graphs from the
//! command line.

use seal_graph::replay::{state_hash, Recording};
use seal_graph::rt::{GraphInstance, MapHost, RunState};
use seal_graph::{Graph, Interp, Program};
use std::process::ExitCode;

fn load(path: &str) -> Result<Program, Vec<String>> {
    let text = std::fs::read_to_string(path).map_err(|e| vec![format!("{path}: {e}")])?;
    let graph = Graph::from_text(&text).map_err(|e| vec![format!("{path}: {e}")])?;
    Program::build(&graph).map_err(|errs| errs.iter().map(|e| format!("{path}: {e}")).collect())
}

fn usage() -> ExitCode {
    eprintln!("usage: seal-run graph check FILE...");
    eprintln!("       seal-run graph compile --out FILE.rs FILE...");
    eprintln!("       seal-run graph run FILE [--frames N] [--dt SECONDS] [--player ID]... [--fire EVENT[:ARG,...]]... [--record OUT]");
    eprintln!("       seal-run graph replay FILE RECORDING");
    ExitCode::from(2)
}

pub fn main(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("check") if args.len() > 1 => check(&args[1..]),
        Some("compile") => compile(&args[1..]),
        Some("run") if args.len() > 1 => run(&args[1..]),
        Some("replay") if args.len() == 3 => replay(&args[1], &args[2]),
        _ => usage(),
    }
}

fn check(paths: &[String]) -> ExitCode {
    let mut failed = false;
    for path in paths {
        match load(path) {
            Ok(p) => {
                let events: Vec<&str> = p.events.iter().map(|e| e.name.as_str()).collect();
                println!(
                    "{path}: graph {} ok, {} nodes, {} variables, events: {}",
                    p.name,
                    p.nodes.len(),
                    p.vars.len(),
                    events.join(", ")
                );
            }
            Err(errs) => {
                failed = true;
                for e in errs {
                    eprintln!("{e}");
                }
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn compile(args: &[String]) -> ExitCode {
    let mut out = None;
    let mut paths = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out = it.next().cloned(),
            _ => paths.push(a.clone()),
        }
    }
    let (Some(out), false) = (out, paths.is_empty()) else { return usage() };
    let mut programs = Vec::new();
    let mut failed = false;
    for path in &paths {
        match load(path) {
            Ok(p) => programs.push(p),
            Err(errs) => {
                failed = true;
                errs.iter().for_each(|e| eprintln!("{e}"));
            }
        }
    }
    if failed {
        return ExitCode::FAILURE;
    }
    let source = seal_graph::codegen::compile_all(&programs);
    if let Err(e) = std::fs::write(&out, source) {
        eprintln!("seal-run graph: {out}: {e}");
        return ExitCode::FAILURE;
    }
    println!("wrote {out} ({} graphs)", programs.len());
    ExitCode::SUCCESS
}

/// Parse `name:1,2.5,true,"text"` into an event name and its arguments,
/// typed by the event's declared parameters.
fn parse_fire(p: &Program, spec: &str) -> Result<(String, Vec<seal_graph::rt::Value>), String> {
    let (name, rest) = spec.split_once(':').unwrap_or((spec, ""));
    let event = p.events.iter().find(|e| e.name == name).ok_or(format!("no event {name}"))?;
    let raw: Vec<&str> = if rest.is_empty() { Vec::new() } else { rest.split(',').collect() };
    if raw.len() != event.params.len() {
        return Err(format!("event {name} takes {} arguments", event.params.len()));
    }
    let mut values = Vec::new();
    for (text, (pname, ty)) in raw.iter().zip(&event.params) {
        let v =
            seal_sealtext::parse(&format!("v = {text}")).map_err(|e| format!("{pname}: {e}"))?;
        let v = v.get("v").ok_or(format!("{pname}: no value"))?;
        values.push(
            seal_graph::value_from_text(*ty, v)
                .ok_or(format!("{pname}: expected {}", ty.name()))?,
        );
    }
    Ok((name.to_string(), values))
}

fn run(args: &[String]) -> ExitCode {
    let path = &args[0];
    let program = match load(path) {
        Ok(p) => p,
        Err(errs) => {
            errs.iter().for_each(|e| eprintln!("{e}"));
            return ExitCode::FAILURE;
        }
    };
    let (mut frames, mut dt, mut fires) = (0u32, 1.0 / 60.0, Vec::new());
    let mut players: Vec<u64> = Vec::new();
    let mut record: Option<String> = None;
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        let v = it.next();
        let ok = match (a.as_str(), v) {
            ("--frames", Some(v)) => v.parse().map(|n| frames = n).is_ok(),
            ("--dt", Some(v)) => v.parse().map(|n| dt = n).is_ok(),
            ("--player", Some(v)) => v.parse().map(|n| players.push(n)).is_ok(),
            ("--record", Some(v)) => {
                record = Some(v.clone());
                true
            }
            ("--fire", Some(v)) => match parse_fire(&program, v) {
                Ok(f) => {
                    fires.push(f);
                    true
                }
                Err(e) => {
                    eprintln!("seal-run graph: {e}");
                    return ExitCode::from(2);
                }
            },
            _ => false,
        };
        if !ok {
            return usage();
        }
    }
    let has_start = program.events.iter().any(|e| e.name == "start");
    let mut g = Interp::new(program, 0);
    let mut host = MapHost::default();
    for &p in &players {
        host.players.push(p);
        host.alive.insert(p);
    }
    let name = std::path::Path::new(path)
        .file_stem()
        .map_or("graph".into(), |n| n.to_string_lossy().into_owned());
    let mut rec = Recording::new(&name).with_players(&players);
    let mut result = Ok(RunState::Done);
    if has_start {
        result = rec.fire(&mut g, "start", &[], &mut host);
    }
    for (name, values) in &fires {
        if result.is_ok() {
            result = rec.fire(&mut g, name, values, &mut host);
        }
    }
    for _ in 0..frames {
        if result.is_err() {
            break;
        }
        result = rec.advance(&mut g, dt, &mut host);
    }
    if let Some(out) = &record {
        if let Err(e) = std::fs::write(out, rec.to_text()) {
            eprintln!("seal-run graph: {out}: {e}");
            return ExitCode::FAILURE;
        }
        println!(
            "recorded {} inputs to {out}, state hash {:016x}",
            rec.inputs.len(),
            state_hash(&g, &host)
        );
    }
    for line in &host.logs {
        println!("log: {line}");
    }
    for (name, value) in g.vars() {
        println!("var {name} = {value}");
    }
    println!("time {:.3}s, {} delays waiting", g.time(), g.pending_delays());
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("seal-run graph: {path}: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Play a recording back and print the state hash after the last input, so
/// two machines (or two builds) can compare runs.
fn replay(path: &str, rec_path: &str) -> ExitCode {
    let program = match load(path) {
        Ok(p) => p,
        Err(errs) => {
            errs.iter().for_each(|e| eprintln!("{e}"));
            return ExitCode::FAILURE;
        }
    };
    let rec = match std::fs::read_to_string(rec_path)
        .map_err(|e| e.to_string())
        .and_then(|t| Recording::from_text(&t))
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("seal-run graph: {rec_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut g = Interp::new(program, 0);
    let mut host = MapHost::default();
    let hashes = rec.play(&mut g, &mut host);
    for line in &host.logs {
        println!("log: {line}");
    }
    println!(
        "replayed {} inputs, state hash {:016x}",
        hashes.len(),
        hashes.last().copied().unwrap_or_else(|| state_hash(&g, &host))
    );
    ExitCode::SUCCESS
}
