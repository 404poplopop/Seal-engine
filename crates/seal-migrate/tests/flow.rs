//! Flow-control macros, operators with extra inputs, and functions copied
//! from their own graphs: the import must behave like the original.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import_all, Options, Status};

const FLOW: &str = include_str!("fixtures/flow.txt");
const GREET: &str = include_str!("fixtures/greet.txt");
const DOUBLE: &str = include_str!("fixtures/double.txt");

fn flow() -> seal_migrate::Import {
    import_all(&[FLOW, GREET, DOUBLE], &Options { name: "flow".into(), ..Options::default() })
        .expect("fixtures parse")
}

#[test]
fn report_counts_macros_and_functions() {
    let r = flow().report;
    let text = r.to_text();
    assert_eq!(r.nodes.len(), 31, "{text}");
    assert_eq!(r.unsupported(), 0, "{text}");
    assert_eq!(r.dropped_links, 0, "{text}");
    // Function ends become `return` nodes, so values reach the caller.
    let ends: Vec<_> =
        r.nodes.iter().filter(|n| n.class.ends_with("K2Node_FunctionResult")).collect();
    assert!(!ends.is_empty());
    assert!(ends.iter().all(|n| n.status == Status::Mapped("return".into())), "{text}");
    // Names repeat across selections; the report keeps them apart.
    assert!(r.nodes.iter().any(|n| n.name == "2/K2Node_FunctionEntry_0"));
    assert!(r.nodes.iter().any(|n| n.name == "3/K2Node_FunctionEntry_0"));
}

fn run() -> (Interp, MapHost) {
    let g = flow().graph;
    let program = Program::build(&g).unwrap_or_else(|e| panic!("{e:?}\n{}", g.to_text()));
    (Interp::new(program, 1), MapHost::default())
}

fn fire(vm: &mut Interp, host: &mut MapHost, event: &str, args: &[Value]) -> Vec<String> {
    let before = host.logs.len();
    vm.fire(event, args, host).unwrap();
    host.logs[before..].to_vec()
}

#[test]
fn do_once_runs_once_until_reset() {
    let (mut vm, mut host) = run();
    assert_eq!(fire(&mut vm, &mut host, "Ping", &[]), ["once"]);
    assert!(fire(&mut vm, &mut host, "Ping", &[]).is_empty());
    assert!(fire(&mut vm, &mut host, "Rearm", &[]).is_empty());
    assert_eq!(fire(&mut vm, &mut host, "Ping", &[]), ["once"]);
}

#[test]
fn flip_flop_alternates_starting_with_a() {
    let (mut vm, mut host) = run();
    assert_eq!(fire(&mut vm, &mut host, "Flip", &[]), ["true"]);
    assert_eq!(fire(&mut vm, &mut host, "Flip", &[]), ["B"]);
    assert_eq!(fire(&mut vm, &mut host, "Flip", &[]), ["true"]);
}

#[test]
fn gate_starts_closed_and_opens_and_toggles() {
    let (mut vm, mut host) = run();
    assert!(fire(&mut vm, &mut host, "Enter", &[]).is_empty());
    fire(&mut vm, &mut host, "OpenGate", &[]);
    assert_eq!(fire(&mut vm, &mut host, "Enter", &[]), ["through"]);
    fire(&mut vm, &mut host, "ToggleGate", &[]);
    assert!(fire(&mut vm, &mut host, "Enter", &[]).is_empty());
    fire(&mut vm, &mut host, "ToggleGate", &[]);
    assert_eq!(fire(&mut vm, &mut host, "Enter", &[]), ["through"]);
}

#[test]
fn extra_inputs_and_function_calls() {
    let (mut vm, mut host) = run();
    let args = [Value::Int(1), Value::Int(20), Value::Int(300)];
    assert_eq!(fire(&mut vm, &mut host, "Sum3", &args), ["321"]);
    assert_eq!(fire(&mut vm, &mut host, "Hello", &[]), ["hi Seal"]);
    assert_eq!(fire(&mut vm, &mut host, "UseDouble", &[]), ["4"]);
}
