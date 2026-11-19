//! The Do N macro: runs N times, then stays shut until reset.

use seal_graph::{GraphInstance, Interp, MapHost, Program};
use seal_migrate::{import, Options};

const DON: &str = include_str!("fixtures/don.txt");

#[test]
fn do_n_runs_n_times_until_reset() {
    let imp =
        import(DON, &Options { name: "don".into(), ..Options::default() }).expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let program = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    let (mut vm, mut host) = (Interp::new(program, 1), MapHost::default());
    let mut fire = |ev: &str| {
        let before = host.logs.len();
        vm.fire(ev, &[], &mut host).unwrap();
        host.logs[before..].to_vec()
    };
    assert_eq!(fire("Hit"), ["1"]);
    assert_eq!(fire("Hit"), ["2"]);
    assert_eq!(fire("Hit"), ["3"]);
    assert!(fire("Hit").is_empty());
    assert!(fire("Hit").is_empty());
    assert!(fire("Rearm").is_empty());
    assert_eq!(fire("Hit"), ["1"]);
}
