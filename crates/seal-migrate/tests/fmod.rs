//! Float modulo keeps the dividend's sign and gives 0 for a zero divisor;
//! FMod also returns the whole quotient toward zero.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

#[test]
fn float_modulo_matches_the_classic_nodes() {
    let imp = import(
        include_str!("fixtures/fmod.txt"),
        &Options { name: "fmod".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost::default();
    for (a, b) in [(7.5, 2.0), (-7.5, 2.0), (7.5, -2.0), (5.0, 0.0), (1.0, 1e-9)] {
        vm.fire("Mod", &[Value::Float(a), Value::Float(b)], &mut h).unwrap();
    }
    assert_eq!(
        h.logs,
        [
            "1.5",
            "3",
            "1.5",
            "-1.5",
            "-3",
            "-1.5",
            "1.5",
            "-3",
            "1.5",
            "0.0",
            "0",
            "0.0",
            "0.0",
            "999999999",
            "0.0"
        ]
    );
}
