//! Random Integer In Range, Random Float In Range and Random Float.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

const RANDOM: &str = include_str!("fixtures/random.txt");

fn program() -> Program {
    let imp = import(RANDOM, &Options { name: "random".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"))
}

fn rolls(me: u64) -> Vec<(i64, f64, f64, i64, bool)> {
    let mut vm = Interp::new(program(), me);
    let mut h = MapHost::default();
    (0..100)
        .map(|_| {
            vm.fire("Roll", &[], &mut h).unwrap();
            let get = |n: &str| vm.var(n).unwrap().clone();
            match (get("Die"), get("Spread"), get("Unit"), get("Pick"), get("Coin")) {
                (
                    Value::Int(d),
                    Value::Float(s),
                    Value::Float(u),
                    Value::Int(p),
                    Value::Bool(c),
                ) => (d, s, u, p, c),
                other => panic!("{other:?}"),
            }
        })
        .collect()
}

#[test]
fn rolls_stay_in_range_and_repeat() {
    let a = rolls(7);
    for &(d, s, u, p, _) in &a {
        assert!(
            (1..=6).contains(&d) && (2.0..3.0).contains(&s) && (0.0..1.0).contains(&u),
            "{d} {s} {u}"
        );
        assert!((0..4).contains(&p), "Random Integer stays below Max: {p}");
    }
    assert!((0..4).all(|k| a.iter().any(|r| r.3 == k)), "every pick comes up");
    assert!(a.iter().any(|r| r.4) && a.iter().any(|r| !r.4), "the coin lands both ways");
    assert_eq!(a, rolls(7), "the same instance seed gives the same rolls");
    assert_ne!(a, rolls(8), "another entity rolls its own sequence");
}
