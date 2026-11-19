//! Compile every graph in `graphs/` to Rust, the way a game build does.

use seal_graph::{codegen, Graph, Program};
use std::path::Path;

fn main() {
    let dir = Path::new("graphs");
    println!("cargo:rerun-if-changed=graphs");
    let mut files: Vec<_> =
        std::fs::read_dir(dir).expect("graphs dir").flatten().map(|e| e.path()).collect();
    files.sort();
    let mut programs = Vec::new();
    for f in files.iter().filter(|f| f.extension().is_some_and(|e| e == "graph")) {
        println!("cargo:rerun-if-changed={}", f.display());
        let text = std::fs::read_to_string(f).expect("read graph");
        let graph = Graph::from_text(&text).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let program = Program::build(&graph).unwrap_or_else(|errs| {
            let list: Vec<String> = errs.iter().map(|e| e.to_string()).collect();
            panic!("{}:\n{}", f.display(), list.join("\n"))
        });
        programs.push(program);
    }
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("graphs.rs");
    std::fs::write(out, codegen::compile_all(&programs)).expect("write graphs.rs");
}
