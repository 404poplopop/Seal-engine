//! The importer on a full copied selection: the graph it builds must check,
//! run with the behaviour the original graph had, and round-trip as SealText.

use seal_graph::{Graph, GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options, Status};

const SCORE: &str = include_str!("fixtures/score.txt");

fn score() -> seal_migrate::Import {
    let opts = Options {
        name: "score".into(),
        var_defaults: vec![("Elapsed Time".into(), Value::Float(1.0))],
    };
    import(SCORE, &opts).expect("fixture parses")
}

#[test]
fn report_lists_every_node() {
    let r = score().report;
    assert_eq!(r.nodes.len(), 34);
    assert_eq!(r.mapped(), 31, "{}", r.to_text());
    assert_eq!(r.unsupported(), 1);
    let miss = r.nodes.iter().find(|n| matches!(n.status, Status::Unsupported(_))).unwrap();
    assert_eq!(miss.class, "K2Node_SpawnActorFromClass");
    // The branch's false output led to the unsupported node.
    assert_eq!(r.dropped_links, 1, "{}", r.to_text());
    assert!(!r.complete());
    assert!((r.coverage() - 31.0 / 32.0).abs() < 1e-9);
    let text = r.to_text();
    assert!(
        text.starts_with("Coverage: 96.9% (31 mapped, 1 unsupported, 2 skipped, 1 links dropped)"),
        "{text}"
    );
    assert!(text.contains("1 x K2Node_SpawnActorFromClass"));
}

#[test]
fn imported_graph_checks_and_round_trips() {
    let g = score().graph;
    let program = Program::build(&g).unwrap_or_else(|e| panic!("{e:?}\n{}", g.to_text()));
    let names: Vec<_> = program.events.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["start", "AddPoints", "tick", "Count_To"]);
    let vars: Vec<_> = g.vars.iter().map(|v| (v.name.as_str(), v.default.clone())).collect();
    assert_eq!(
        vars,
        [
            ("Score", Value::Int(0)),
            ("bWon", Value::Bool(false)),
            ("Elapsed_Time", Value::Float(1.0)),
            ("Sum", Value::Int(0)),
        ]
    );
    let text = g.to_text();
    assert_eq!(Graph::from_text(&text).unwrap(), g);
}

#[test]
fn imported_graph_behaves_like_the_original() {
    let mut vm = Interp::new(Program::build(&score().graph).unwrap(), 1);
    let mut host = MapHost::default();
    vm.fire("start", &[], &mut host).unwrap();
    assert_eq!(host.logs, ["ready"]);
    vm.advance(0.4, &mut host).unwrap();
    assert_eq!(host.logs, ["ready"], "the delay holds for half a second");
    vm.advance(0.2, &mut host).unwrap();
    // Count To(4) runs the loop 1..=4, then prints the sum.
    assert_eq!(host.logs, ["ready", "late", "10"]);
    for amount in [4, 7, 1] {
        vm.fire("AddPoints", &[Value::Int(amount)], &mut host).unwrap();
    }
    assert_eq!(host.logs, ["ready", "late", "10", "won at 11"], "announced exactly once");
    assert_eq!(vm.var("Score"), Some(&Value::Int(12)));
    assert_eq!(vm.var("bWon"), Some(&Value::Bool(true)));
    vm.fire("tick", &[Value::Float(0.25)], &mut host).unwrap();
    vm.fire("tick", &[Value::Float(0.25)], &mut host).unwrap();
    // Advancing time fires tick too, so every step added its dt in order.
    let expected = [0.4, 0.2, 0.25, 0.25].iter().fold(1.0, |acc, dt| acc + dt);
    assert_eq!(vm.var("Elapsed_Time"), Some(&Value::Float(expected)));
}

#[test]
fn partial_selections_report_missing_links() {
    // Only the print node: its exec input came from a node that was not copied.
    let start = SCORE.find("Begin Object Class=/Script/BlueprintGraph.K2Node_CallFunction Name=\"K2Node_CallFunction_0\"").unwrap();
    let end = start + SCORE[start..].find("End Object").unwrap() + "End Object".len();
    let r = import(&SCORE[start..end], &Options::default()).unwrap();
    assert_eq!(r.report.mapped(), 1);
    assert_eq!(r.report.dropped_links, 1, "{}", r.report.to_text());
    assert!(r.report.notes[0].contains("not in the selection"));
}
