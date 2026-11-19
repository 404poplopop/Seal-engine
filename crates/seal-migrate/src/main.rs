//! `seal-migrate <export.txt>... [out.graph] [--name NAME]`
//!
//! Imports copied visual-script nodes into one Seal graph and prints the
//! coverage report. Pass the event graph and each function graph as separate
//! files. Exits with 2 when some nodes could not be mapped.

use seal_migrate::{import_all, Options};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut name = None;
    if let Some(i) = args.iter().position(|a| a == "--name") {
        args.remove(i);
        if i < args.len() {
            name = Some(args.remove(i));
        }
    }
    let out = if args.len() > 1 && args.last().is_some_and(|a| a.ends_with(".graph")) {
        args.pop()
    } else {
        None
    };
    if args.is_empty() {
        eprintln!("usage: seal-migrate <export.txt>... [out.graph] [--name NAME]");
        std::process::exit(64);
    }
    let mut texts = Vec::new();
    for input in &args {
        match std::fs::read_to_string(input) {
            Ok(t) => texts.push(t),
            Err(e) => {
                eprintln!("cannot read {input}: {e}");
                std::process::exit(66);
            }
        }
    }
    let stem =
        std::path::Path::new(&args[0]).file_stem().and_then(|s| s.to_str()).unwrap_or("imported");
    let opts = Options {
        name: name.unwrap_or_else(|| seal_migrate::ident(stem)),
        var_defaults: Vec::new(),
    };
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let result = match import_all(&refs, &opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(65);
        }
    };
    print!("{}", result.report.to_text());
    if let Err(errors) = seal_graph::Program::build(&result.graph) {
        for e in errors {
            eprintln!("graph check: {e:?}");
        }
    }
    if let Some(out) = out {
        if let Err(e) = std::fs::write(&out, result.graph.to_text()) {
            eprintln!("cannot write {out}: {e}");
            std::process::exit(73);
        }
        println!("Wrote {out}");
    }
    if !result.report.complete() {
        std::process::exit(2);
    }
}
