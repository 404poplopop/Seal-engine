//! Compiled graphs and the interpreter must agree on everything: results,
//! errors, variables, logs, world writes, pending delays and time.

use proptest::prelude::*;
use seal_graph::rt::{GraphInstance, MapHost, RunState, Value, V2, V3};
use seal_graph::{Elem, Graph, GraphError, Interp, Program};

const ME: u64 = (1 << 32) | 7;

fn program(name: &str) -> Program {
    let src = seal_graph_tests::SOURCES.iter().find(|(n, _)| *n == name).unwrap().1;
    Program::build(&Graph::from_text(src).unwrap()).unwrap()
}

#[derive(Clone, Debug)]
enum Step {
    Fire(&'static str, Vec<Value>),
    Advance(f64),
}

fn mover_host() -> MapHost {
    MapHost::default().with(ME, "mover", "pos", Value::Vec2(V2 { x: 0.0, y: 1.0 })).with(
        ME,
        "mover",
        "vel",
        Value::Vec2(V2 { x: 3.7, y: -0.3 }),
    )
}

type Outcome = Vec<Result<RunState, GraphError>>;

fn run(
    g: &mut dyn GraphInstance,
    host: &mut MapHost,
    steps: &[Step],
) -> (Outcome, Vec<Vec<(String, Value)>>) {
    let mut results = Vec::new();
    let mut states = Vec::new();
    for s in steps {
        results.push(match s {
            Step::Fire(e, a) => g.fire(e, a, host),
            Step::Advance(dt) => g.advance(*dt, host),
        });
        let mut v = g.vars();
        v.push(("@time".into(), Value::Float(g.time())));
        v.push(("@delays".into(), Value::Int(g.pending_delays() as i64)));
        states.push(v);
    }
    (results, states)
}

/// Run the same steps through both and compare every observable.
fn both(
    name: &str,
    host: MapHost,
    steps: &[Step],
    budget: Option<u64>,
) -> (MapHost, Outcome, Vec<Vec<(String, Value)>>) {
    let mut interp = Interp::new(program(name), ME);
    let mut compiled = seal_graph_tests::create(name, ME).unwrap();
    if let Some(b) = budget {
        interp.set_budget(b);
        compiled.set_budget(b);
    }
    let (mut h1, mut h2) = (host.clone(), host);
    let (r1, s1) = run(&mut interp, &mut h1, steps);
    let (r2, s2) = run(compiled.as_mut(), &mut h2, steps);
    assert_eq!(r1, r2, "results differ for {name}");
    assert_eq!(s1, s2, "state differs for {name}");
    assert_eq!(h1, h2, "world or logs differ for {name}");
    (h1, r1, s1)
}

fn var(state: &[(String, Value)], name: &str) -> Value {
    state.iter().find(|(n, _)| n == name).unwrap().1.clone()
}

#[test]
fn every_graph_is_compiled() {
    let names: Vec<&str> = seal_graph_tests::SOURCES.iter().map(|(n, _)| *n).collect();
    assert_eq!(seal_graph_tests::GRAPHS, names.as_slice());
    assert!(seal_graph_tests::create("nope", ME).is_none());
}

#[test]
fn counter_matches_and_announces_once() {
    let mut steps = vec![Step::Fire("start", vec![])];
    for i in 0..8 {
        steps.push(Step::Advance(0.1 + i as f64 * 0.01));
        steps.push(Step::Fire("hit", vec![Value::Int(2)]));
    }
    let (host, results, states) = both("counter", MapHost::default(), &steps, None);
    assert!(results.iter().all(|r| *r == Ok(RunState::Done)));
    assert_eq!(host.logs, vec!["counter started".to_string(), "done at 10".to_string()]);
    let last = states.last().unwrap();
    assert_eq!(var(last, "count"), Value::Int(16));
    assert_eq!(var(last, "announced"), Value::Bool(true));
}

#[test]
fn wrong_arguments_fail_the_same_way() {
    let steps = vec![
        Step::Fire("hit", vec![]),
        Step::Fire("hit", vec![Value::Float(1.0)]),
        Step::Fire("unknown", vec![Value::Int(1)]),
    ];
    let (_, results, _) = both("counter", MapHost::default(), &steps, None);
    assert!(results[0].as_ref().unwrap_err().message.contains("takes 1 arguments"));
    assert!(results[1].as_ref().unwrap_err().message.contains("argument 0 is int"));
    assert_eq!(results[2], Ok(RunState::Done));
}

#[test]
fn fizzbuzz_loop_calls_and_math() {
    let steps = vec![Step::Fire("run", vec![Value::Int(15)]), Step::Fire("done", vec![])];
    let (host, _, states) = both("fizz", MapHost::default(), &steps, None);
    let expect = "1 2 Fizz 4 Buzz Fizz 7 8 Fizz Buzz 11 Fizz 13 14 FizzBuzz";
    assert_eq!(host.logs[..15].join(" "), expect);
    assert_eq!(var(&states[0], "sum"), Value::Int(120));
    assert_eq!(var(&states[0], "line"), Value::Str(expect.replace(' ', "")));
    let ratio = (120.0f64 / 7.0).sqrt();
    assert_eq!(var(&states[1], "ratio"), Value::Float(ratio));
    // Integer division by zero gives 0.
    assert_eq!(host.logs[15], format!("{ratio}0"));
}

#[test]
fn endless_loop_stops_at_the_same_step() {
    let steps = vec![Step::Fire("spin", vec![]), Step::Fire("spin", vec![])];
    let (_, results, states) = both("fizz", MapHost::default(), &steps, Some(1000));
    let err = results[0].as_ref().unwrap_err();
    assert!(err.message.contains("budget"), "{err}");
    assert!(var(&states[0], "spins").int() > 100);
    assert_eq!(var(&states[1], "spins").int(), 2 * var(&states[0], "spins").int());
}

#[test]
fn deep_recursion_hits_the_budget_identically() {
    let steps = vec![
        Step::Fire("dive", vec![Value::Int(50)]),
        Step::Fire("dive", vec![Value::Int(1_000_000)]),
    ];
    let (_, results, states) = both("fizz", MapHost::default(), &steps, Some(5000));
    assert_eq!(results[0], Ok(RunState::Done));
    assert_eq!(var(&states[0], "depth"), Value::Int(50));
    assert!(results[1].is_err());
}

#[test]
fn game_rules_and_ai_orders_compile_and_match() {
    let (host, results, _) =
        both("rules", MapHost::default(), &[Step::Fire("start", vec![])], None);
    assert_eq!(results, [Ok(RunState::Done)]);
    assert_eq!(host.logs, ["0false"], "a host without rules ignores them and reads defaults");
}

#[test]
fn mover_moves_bounces_and_blinks() {
    let steps: Vec<Step> = (0..120).map(|_| Step::Advance(1.0 / 60.0)).collect();
    let (host, results, states) = both("mover", mover_host(), &steps, None);
    assert!(results.iter().all(|r| *r == Ok(RunState::Done)));
    let last = states.last().unwrap();
    assert!(var(last, "bounces").int() >= 1, "{last:?}");
    // A delay of 0.25 s retriggered every tick fires about four times in two seconds.
    let blinks = var(last, "blinks").int();
    assert!((6..=8).contains(&blinks), "blinks {blinks}");
    assert!(host.writes > 120);
}

#[test]
fn missing_world_data_is_an_error_at_the_node() {
    let (_, results, _) = both("mover", MapHost::default(), &[Step::Advance(0.1)], None);
    let err = results[0].as_ref().unwrap_err();
    // `pos` (node 3) is read before `vel`, so it reports first, in both.
    assert_eq!(err.node, Some(3));
}

fn arb_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        (0.0f64..0.5).prop_map(Step::Advance),
        (-50i64..50).prop_map(|n| Step::Fire("hit", vec![Value::Int(n)])),
        (-3i64..40).prop_map(|n| Step::Fire("run", vec![Value::Int(n)])),
        (-2i64..30).prop_map(|n| Step::Fire("dive", vec![Value::Int(n)])),
        Just(Step::Fire("done", vec![])),
        Just(Step::Fire("start", vec![])),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn random_runs_agree(steps in proptest::collection::vec(arb_step(), 1..40), budget in 50u64..3000) {
        for name in ["counter", "fizz", "mover"] {
            both(name, mover_host(), &steps, Some(budget));
        }
    }
}

#[test]
fn returns_hand_values_back_and_leave_loops() {
    let steps = vec![Step::Fire("go", vec![]), Step::Fire("double", vec![Value::Int(5)])];
    let (host, results, states) = both("funcs", MapHost::default(), &steps, None);
    assert!(results.iter().all(|r| r.is_ok()), "{results:?}");
    // 21 doubled; a search that stops at 4; a search that never returns gives 0.
    assert_eq!(host.logs, ["42", "0"]);
    assert_eq!(var(&states[0], "found"), Value::Int(4));
    // The first search left its loop after 4 rounds, the second ran all 10.
    assert_eq!(var(&states[0], "iters"), Value::Int(14));
    // Firing a returning event directly just ends it.
    assert_eq!(var(&states[1], "iters"), Value::Int(14));
}

#[test]
fn self_calls_keep_their_own_parameters() {
    let steps: Vec<Step> =
        [1, 5, 10, 20].iter().map(|n| Step::Fire("show_fact", vec![Value::Int(*n)])).collect();
    let (host, results, _) = both("funcs", MapHost::default(), &steps, None);
    assert!(results.iter().all(|r| r.is_ok()), "{results:?}");
    assert_eq!(host.logs, ["1", "120", "3628800", "2432902008176640000"]);
}

#[test]
fn deep_self_calls_stop_at_the_same_step() {
    let steps = vec![
        Step::Fire("show_fact", vec![Value::Int(100_000)]),
        Step::Fire("show_fact", vec![Value::Int(6)]),
    ];
    let (host, results, _) = both("funcs", MapHost::default(), &steps, Some(5000));
    assert!(results[0].is_err());
    // The next run starts clean.
    assert_eq!(results[1], Ok(seal_graph::rt::RunState::Done));
    assert_eq!(host.logs, ["720"]);
}

proptest! {
    #[test]
    fn trig_rounding_and_vectors_match(a in proptest::num::f64::ANY, x in -1e6f64..1e6, y in -1e6f64..1e6, p in -1e3f64..1e3, q in -1e3f64..1e3) {
        let v = |x: f64, y: f64| Value::Vec2(V2 { x, y });
        let steps = vec![
            Step::Fire("calc", vec![Value::Float(a), v(x, y), v(p, q)]),
            Step::Fire("calc", vec![Value::Float(x), v(0.0, 0.0), v(p, q)]),
        ];
        both("mathops", MapHost::default(), &steps, None);
    }
}

#[test]
fn trig_and_rounding_values() {
    let v = |x: f64, y: f64| Value::Vec2(V2 { x, y });
    let steps = vec![Step::Fire("calc", vec![Value::Float(-2.5), v(3.0, 4.0), v(2.0, -1.0)])];
    let (_, _, states) = both("mathops", MapHost::default(), &steps, None);
    let st = &states[0];
    assert_eq!(var(st, "f"), Value::Int(-3));
    assert_eq!(var(st, "r"), Value::Int(-3));
    assert_eq!(var(st, "u"), Value::Int(-2));
    assert_eq!(var(st, "d"), Value::Float(2.0));
    assert_eq!(var(st, "n"), v(0.6, 0.8));
    let (Value::Float(s), Value::Float(c)) = (var(st, "s"), var(st, "c")) else { panic!() };
    assert!((s - (-2.5f64).sin()).abs() < 1e-12 && (c - (-2.5f64).cos()).abs() < 1e-12, "{s} {c}");
}

#[test]
fn text_values() {
    let steps = vec![
        Step::Fire("say", vec![Value::Str("héllo Ünï".into())]),
        Step::Fire("say", vec![Value::Str("".into())]),
    ];
    let (host, _, states) = both("strops", MapHost::default(), &steps, None);
    assert_eq!(host.logs, ["HÉLLO ÜNÏ", "9", "", "0"]);
    assert_eq!(var(&states[0], "n"), Value::Int(9));
    assert_eq!(var(&states[0], "lo"), Value::Str("héllo ünï".into()));
}

#[test]
fn text_search() {
    let fire = |s: &str, t: &str| {
        vec![Step::Fire("find", vec![Value::Str(s.into()), Value::Str(t.into())])]
    };
    let (host, _, _) = both("strsearch", MapHost::default(), &fire("héllo Ünï", "Ün"), None);
    assert_eq!(host.logs, ["true", "false", "false"]);
    let (host, _, _) = both("strsearch", MapHost::default(), &fire("seal", "se"), None);
    assert_eq!(host.logs, ["true", "true", "false"]);
    let (host, _, _) = both("strsearch", MapHost::default(), &fire("seal", ""), None);
    assert_eq!(host.logs, ["true", "true", "true"]);
    let (host, _, _) = both("strsearch", MapHost::default(), &fire("seal", "al"), None);
    assert_eq!(host.logs, ["true", "false", "true"]);
}

#[test]
fn lists() {
    let int = Value::Int;
    let steps = vec![
        Step::Fire("add", vec![int(9)]),
        Step::Fire("sum", vec![]),
        Step::Fire("query", vec![int(7)]),
        Step::Fire("query", vec![int(4)]),
        // Set index 1 to 0 and insert 0 at 1: [5, 0, 0, 9]; out of range does nothing.
        Step::Fire("edit", vec![int(1), int(0)]),
        Step::Fire("edit", vec![int(9), int(3)]),
        Step::Fire("sum", vec![]),
        // Remove index 0, then every 0: [9].
        Step::Fire("drop", vec![int(0)]),
        Step::Fire("sum", vec![]),
        Step::Fire("drop", vec![int(-1)]),
        Step::Fire("make", vec![Value::Float(0.25)]),
    ];
    let (host, _, states) = both("lists", MapHost::default(), &steps, None);
    assert_eq!(
        host.logs,
        ["3", "21", "1", "true", "0", "-1", "false", "0", "14", "9", "3", "0.25"]
    );
    let ints = |v: &[i64]| Value::List(Elem::Int, v.iter().map(|x| Value::Int(*x)).collect());
    assert_eq!(var(&states[0], "items"), ints(&[5, 7, 9]));
    assert_eq!(var(&states[4], "items"), ints(&[5, 0, 0, 9]));
    assert_eq!(var(&states[9], "items"), ints(&[9]));
    let floats =
        Value::List(Elem::Float, vec![Value::Float(1.5), Value::Float(0.25), Value::Float(-2.0)]);
    assert_eq!(var(&states[10], "floats"), floats);
}

#[test]
fn powers_logs_and_inverse_trig() {
    let steps = vec![Step::Fire("calc", vec![Value::Float(1.0), Value::Float(0.5)])];
    let (_, _, states) = both("transc", MapHost::default(), &steps, None);
    let f = |n: &str| match var(&states[0], n) {
        Value::Float(f) => f,
        v => panic!("{v:?}"),
    };
    let near = |a: f64, b: f64| (a - b).abs() < 1e-14;
    assert!(near(f("e"), std::f64::consts::E));
    assert!(near(f("l"), 0.5f64.ln()));
    assert!(near(f("p"), 0.5));
    assert!(near(f("t"), std::f64::consts::FRAC_PI_4));
    assert!(near(f("s"), std::f64::consts::FRAC_PI_6));
    assert!(near(f("c"), std::f64::consts::FRAC_PI_3));
    assert!(near(f("w"), 2f64.atan()));
}

proptest! {
    #[test]
    fn transcendental_nodes_match(a in -50.0f64..50.0, b in -2.0f64..2.0) {
        both("transc", MapHost::default(), &[Step::Fire("calc", vec![Value::Float(a), Value::Float(b)])], None);
    }

    #[test]
    fn list_nodes_match(ops in proptest::collection::vec((0u8..5, -3i64..6, -2i64..4), 0..24)) {
        let steps: Vec<Step> = ops
            .into_iter()
            .map(|(k, a, b)| match k {
                0 => Step::Fire("add", vec![Value::Int(a)]),
                1 => Step::Fire("edit", vec![Value::Int(a), Value::Int(b)]),
                2 => Step::Fire("query", vec![Value::Int(a)]),
                3 => Step::Fire("drop", vec![Value::Int(a)]),
                _ => Step::Fire("sum", vec![]),
            })
            .collect();
        both("lists", MapHost::default(), &steps, None);
    }

    #[test]
    fn search_nodes_match(s in ".*", t in ".{0,3}") {
        both("strsearch", MapHost::default(), &[Step::Fire("find", vec![Value::Str(s), Value::Str(t)])], None);
    }

    #[test]
    fn text_nodes_match(s in ".*") {
        both("strops", MapHost::default(), &[Step::Fire("say", vec![Value::Str(s)])], None);
    }
}

#[test]
fn vec3_values() {
    let v = |x: f64, y: f64, z: f64| Value::Vec3(V3 { x, y, z });
    let steps =
        vec![Step::Fire("calc", vec![v(1.0, 2.0, 2.0), v(0.0, 1.0, 0.0), Value::Float(2.0)])];
    let (host, _, states) = both("space", MapHost::default(), &steps, None);
    let st = &states[0];
    assert_eq!(var(st, "sum"), v(1.0, 3.0, 2.0));
    assert_eq!(var(st, "diff"), v(1.0, 1.0, 2.0));
    assert_eq!(var(st, "neg"), v(-1.0, -2.0, -2.0));
    assert_eq!(var(st, "cr"), v(-2.0, 0.0, 1.0));
    assert_eq!(var(st, "d"), Value::Float(2.0));
    assert_eq!(var(st, "len"), Value::Float(3.0));
    assert_eq!(var(st, "n"), v(1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0));
    assert_eq!(var(st, "sc"), v(2.0, 4.0, 4.0));
    assert_eq!(var(st, "mix"), v(1.0, 1.0, 0.25));
    assert_eq!(var(st, "same"), Value::Bool(false));
    assert_eq!(host.logs, ["(-2, 0, 1)"]);
}

proptest! {
    #[test]
    fn vec3_runs_match(x in -1e6f64..1e6, y in -1e6f64..1e6, z in -1e6f64..1e6, s in proptest::num::f64::ANY) {
        let v = |x: f64, y: f64, z: f64| Value::Vec3(V3 { x, y, z });
        let steps = vec![
            Step::Fire("calc", vec![v(x, y, z), v(z, x, y), Value::Float(s)]),
            Step::Fire("calc", vec![v(0.0, 0.0, 0.0), v(x, x, x), Value::Float(x)]),
        ];
        both("space", MapHost::default(), &steps, None);
    }
}

#[test]
fn stop_ends_a_loop_early() {
    for (n, last) in [(0, -1), (1, 0), (7, 6)] {
        let steps = vec![Step::Fire("run", vec![Value::Int(n)])];
        let (_, _, states) = both("early", MapHost::default(), &steps, Some(1000));
        assert_eq!(var(&states[0], "hits"), Value::Int(n));
        assert_eq!(var(&states[0], "last"), Value::Int(last));
    }
}

proptest! {
    #[test]
    fn stopped_loops_match(n in -3i64..200, budget in 1u64..900) {
        let steps = vec![Step::Fire("run", vec![Value::Int(n)]), Step::Fire("run", vec![Value::Int(n / 2)])];
        both("early", MapHost::default(), &steps, Some(budget));
    }
}

#[test]
fn timers_loop_catch_up_replace_and_clear() {
    let arm = |s: f64, l: bool, lim: i64| {
        Step::Fire("arm", vec![Value::Float(s), Value::Bool(l), Value::Int(lim)])
    };
    let steps = vec![
        arm(0.5, false, 100),
        Step::Advance(0.4),
        Step::Advance(0.2), // one shot fires: 1
        Step::Advance(5.0), // nothing more
        arm(1.0, true, 100),
        Step::Advance(3.5),  // catches up: 2, 3, 4
        arm(2.0, true, 100), // replaces the 1 s timer
        Step::Advance(1.9),
        Step::Advance(0.2), // 5
        Step::Fire("disarm", vec![]),
        Step::Advance(10.0),
        arm(0.25, true, 8), // stops itself at 8 even inside one long frame
        Step::Advance(100.0),
        arm(0.0, true, 100), // zero seconds: no timer
        Step::Advance(1.0),
    ];
    let (host, _, states) = both("timers", MapHost::default(), &steps, None);
    assert_eq!(host.logs, ["1", "2", "3", "4", "5", "6", "7", "8"]);
    assert_eq!(var(states.last().unwrap(), "pings"), Value::Int(8));
}

proptest! {
    #[test]
    fn timer_runs_match(ops in proptest::collection::vec((0u8..4, 0.0f64..3.0, any::<bool>(), 0i64..40), 1..30)) {
        let steps: Vec<Step> = ops
            .into_iter()
            .map(|(k, x, b, n)| match k {
                0 => Step::Fire("arm", vec![Value::Float(x - 0.5), Value::Bool(b), Value::Int(n)]),
                1 => Step::Fire("disarm", vec![]),
                _ => Step::Advance(x),
            })
            .collect();
        both("timers", MapHost::default(), &steps, None);
    }
}

fn coin_host() -> MapHost {
    MapHost::default().prefab("coin", &[("coin", "value", Value::Int(10))])
}

#[test]
fn spawn_write_despawn_and_check() {
    let f = |e: &'static str| Step::Fire(e, vec![]);
    let steps = vec![
        f("check"),
        f("make"),
        f("make"),
        f("check"),
        f("kill"),
        f("check"),
        f("make"),
        f("check"),
    ];
    let (host, out, states) = both("spawner", coin_host(), &steps, None);
    assert_eq!(host.logs, ["false", "10", "11", "true", "false", "12", "true"]);
    assert_eq!(host.alive.len(), 2, "one of three coins was despawned");
    assert_eq!(var(states.last().unwrap(), "made"), Value::Int(3));
    assert!(out.iter().all(|r| r.is_ok()), "{out:?}");
}

fn kinds_host() -> MapHost {
    MapHost::default()
        .prefab("enemy", &[])
        .prefab("boss", &[])
        .based_on("boss", "enemy")
        .prefab("coin", &[])
}

#[test]
fn is_a_follows_the_prefab_base_chain() {
    let f = |e: &'static str| Step::Fire(e, vec![]);
    let steps = vec![
        f("check"),
        f("boss"),
        f("check"),
        f("coin"),
        f("check"),
        f("boss"),
        f("kill"),
        f("check"),
    ];
    let (host, out, _) = both("kinds", kinds_host(), &steps, None);
    assert_eq!(host.logs, ["falsefalse", "truetrue", "falsefalse", "falsefalse"]);
    assert!(out.iter().all(|r| r.is_ok()), "{out:?}");
}

#[test]
fn find_all_lists_entities_by_prefab_base() {
    let f = |e: &'static str| Step::Fire(e, vec![]);
    let steps = vec![
        f("count"),
        f("boss"),
        f("coin"),
        f("boss"),
        f("count"),
        f("kill"),
        f("count"),
        f("kill"),
        f("count"),
    ];
    let (host, out, _) = both("census", kinds_host(), &steps, None);
    assert_eq!(host.logs, ["00", "21", "11", "01"]);
    assert!(out.iter().all(|r| r.is_ok()), "{out:?}");
}

#[test]
fn find_first_takes_the_lowest_id_or_nothing() {
    let f = |e: &'static str| Step::Fire(e, vec![]);
    let steps = vec![
        f("first"),
        f("coin"),
        f("boss"),
        f("boss"),
        f("first"),
        f("count"),
        f("first"),
        f("count"),
    ];
    let (host, out, _) = both("census", kinds_host(), &steps, None);
    assert_eq!(host.logs, ["11", "01"]);
    assert!(out.iter().all(|r| r.is_ok()), "destroying no entity does nothing: {out:?}");
}

#[test]
fn random_rolls_repeat_and_stay_in_range() {
    let steps: Vec<Step> = (0..200).map(|_| Step::Fire("roll", vec![])).collect();
    let (host, out, states) = both("dice", MapHost::default(), &steps, None);
    assert!(out.iter().all(|r| r.is_ok()), "{out:?}");
    let mut seen = [0usize; 7];
    for s in &states {
        let Value::Int(d) = var(s, "die") else { panic!() };
        assert!((1..=6).contains(&d), "die {d}");
        seen[d as usize] += 1;
        let Value::Float(f) = var(s, "f") else { panic!() };
        assert!((2.0..3.0).contains(&f), "f {f}");
        assert_eq!(var(s, "odd"), Value::Int(6), "a range upside down gives its low end");
    }
    assert!(seen[1..].iter().all(|&c| c > 15), "every face comes up: {seen:?}");
    assert_eq!(host.logs.len(), 200);
    // The die logged is drawn again, so it is a fresh roll, still in range.
    assert!(host.logs.iter().all(|l| ["1", "2", "3", "4", "5", "6"].contains(&l.as_str())));
    let (again, _, _) = both("dice", MapHost::default(), &steps, None);
    assert_eq!(host.logs, again.logs, "the same seed gives the same rolls");
}

#[test]
fn numbers_are_read_from_the_start_of_text() {
    let cases = [
        "42",
        "  -17px",
        "+3.75e2x",
        "2.5e",
        ".5",
        "7.",
        "abc",
        "",
        "-",
        "1e3",
        "99999999999999999999999",
        "-0.0",
    ];
    let steps: Vec<Step> =
        cases.iter().map(|c| Step::Fire("read", vec![Value::Str((*c).into())])).collect();
    let (host, out, _) = both("numparse", MapHost::default(), &steps, None);
    assert!(out.iter().all(|r| r.is_ok()), "{out:?}");
    let want = [
        "42|42",
        "-17|-17",
        "3|375",
        "2|2.5",
        "0|0.5",
        "7|7",
        "0|0",
        "0|0",
        "0|0",
        "1|1000",
        "9223372036854775807|100000000000000000000000",
        "0|-0",
    ];
    assert_eq!(host.logs, want);
}

#[test]
fn numbers_show_grouped_with_three_decimals_at_most() {
    let cases: [(i64, f64); 8] = [
        (0, 0.0),
        (999, 2.718291),
        (1000, 1234.5),
        (-1234567, -0.0004),
        (i64::MIN, 1e6),
        (12, -1999.9996),
        (100, 0.1),
        (-5, 1.0 / 3.0),
    ];
    let steps: Vec<Step> = cases
        .iter()
        .map(|(i, f)| Step::Fire("show", vec![Value::Int(*i), Value::Float(*f)]))
        .collect();
    let (host, out, _) = both("numtext", MapHost::default(), &steps, None);
    assert!(out.iter().all(|r| r.is_ok()), "{out:?}");
    let want = [
        "0|0",
        "999|2.718",
        "1,000|1,234.5",
        "-1,234,567|0",
        "-9,223,372,036,854,775,808|1,000,000",
        "12|-2,000",
        "100|0.1",
        "-5|0.333",
    ];
    assert_eq!(host.logs, want);
}

#[test]
fn players_name_the_entity_they_control() {
    let host = MapHost { players: vec![7, 3], alive: [7, 3].into(), ..MapHost::default() };
    let steps: Vec<Step> =
        [0, 1, 2, -1].iter().map(|i| Step::Fire("who", vec![Value::Int(*i)])).collect();
    let (host, out, _) = both("players", host, &steps, None);
    assert!(out.iter().all(|r| r.is_ok()), "{out:?}");
    assert_eq!(host.logs, ["true7v0", "true3v0", "falsenone", "falsenone"]);
}

#[test]
fn game_time_and_frame_length() {
    let steps = vec![
        Step::Fire("show", vec![]),
        Step::Advance(0.25),
        Step::Advance(0.5),
        Step::Fire("show", vec![]),
    ];
    let (host, out, states) = both("clock", MapHost::default(), &steps, None);
    assert_eq!(host.logs, ["00", "0.750.5"]);
    let last = states.last().unwrap();
    assert_eq!(var(last, "t"), Value::Float(0.75));
    assert_eq!(var(last, "d"), Value::Float(0.5));
    assert_eq!(var(last, "same"), Value::Bool(true), "frame_dt matches the tick's dt");
    assert!(out.iter().all(|r| r.is_ok()), "{out:?}");
}

#[test]
fn spawning_an_unknown_prefab_is_an_error() {
    let (_, out, _) = both("spawner", coin_host(), &[Step::Fire("bad", vec![])], None);
    assert!(format!("{out:?}").contains("no prefab nope"), "{out:?}");
}

proptest! {
    #[test]
    fn spawner_runs_match(ops in proptest::collection::vec(0u8..4, 1..40)) {
        let steps: Vec<Step> = ops
            .into_iter()
            .map(|k| Step::Fire(["make", "kill", "check", "make"][k as usize], vec![]))
            .collect();
        both("spawner", coin_host(), &steps, None);
    }
}

#[test]
fn state_machine_matches_priority_time_self_and_force() {
    let steps = vec![
        Step::Advance(0.5),
        Step::Fire("move", vec![]),
        Step::Advance(0.5),
        Step::Fire("do_bounce", vec![]),
        Step::Advance(0.5),
        Step::Fire("force_hop", vec![]),
        Step::Advance(0.5),
        Step::Fire("read", vec![]),
        Step::Fire("force_nope", vec![]),
        Step::Fire("read", vec![]),
    ];
    let (host, results, _) = both("states", MapHost::default(), &steps, None);
    assert!(results[..8].iter().all(|r| *r == Ok(RunState::Done)), "{results:?}");
    let err = results[8].as_ref().unwrap_err();
    assert!(err.message.contains("unknown state nope"), "{err}");
    assert_eq!(results[9], Ok(RunState::Done));
    assert_eq!(
        host.logs,
        vec![
            "enter idle".to_string(),
            "update idle".to_string(),
            "update idle".to_string(),
            "exit idle".to_string(),
            "enter run".to_string(),
            "update run".to_string(),
            "exit run".to_string(),
            "enter run".to_string(),
            "exit run".to_string(),
            "enter hop".to_string(),
            "update hop".to_string(),
            "hop".to_string(),
            "hop".to_string(),
        ]
    );
}

#[test]
fn vectors_print_with_three_fixed_decimals() {
    let v = |x, y, z| Value::Vec3(V3 { x, y, z });
    let w = |x, y| Value::Vec2(V2 { x, y });
    let steps = vec![
        Step::Fire("show", vec![v(1.0, -2.5, 1234.5678), w(0.0005, -0.0)]),
        Step::Fire("show", vec![v(0.0, 0.0, 0.0), w(1e6, 2.0 / 3.0)]),
    ];
    let (host, out, _) = both("vectext", MapHost::default(), &steps, None);
    assert!(out.iter().all(|r| r.is_ok()), "{out:?}");
    assert_eq!(
        host.logs,
        [
            "X=1.000 Y=-2.500 Z=1234.568",
            "X=0.001 Y=-0.000",
            "X=0.000 Y=0.000 Z=0.000",
            "X=1000000.000 Y=0.667"
        ]
    );
}

fn query_shape(entity: u64, x: f64, layer: u32) -> seal_graph::query::Shape {
    seal_graph::query::Shape {
        entity,
        kind: "box".into(),
        center: [x, 0.0],
        angle: 0.0,
        half: [0.5, 0.5],
        points: Vec::new(),
        layer,
        sensor: false,
    }
}

#[test]
fn physics_queries_match_on_both_backends() {
    let host = MapHost {
        shapes: vec![query_shape(1, 0.0, 1), query_shape(3, 1.5, 2), query_shape(2, 3.0, 1)],
        ..MapHost::default()
    };
    let (_, _, states) = both("queries", host, &[Step::Fire("ask", vec![])], None);
    let last = states.last().unwrap();
    let ids = |name: &str| var(last, name).list().iter().map(|v| v.entity()).collect::<Vec<_>>();
    assert_eq!(ids("ents"), vec![1, 3, 2]);
    assert!((var(last, "dist").float() - 1.5).abs() < 1e-6);
    assert!((var(last, "cast").float() - 1.25).abs() < 1e-3, "{}", var(last, "cast").float());
    assert_eq!(ids("inside"), vec![1]);
    assert_eq!(ids("boxed"), vec![1, 3]);
    assert_eq!(ids("circled"), vec![2]);
    assert_eq!(var(last, "bumped"), Value::Bool(true));
}
