//! Graph logic lowered to a kernel gives the same bits as running the graph
//! on each entity, on any number of threads.

use seal_graph::kernel::lower;
use seal_graph::kernel::JobPool;
use seal_graph::rt::{GraphInstance, MapHost, Value, V2};
use seal_graph::{Graph, Interp, Program};

fn program(name: &str) -> Program {
    let src = seal_graph_tests::SOURCES.iter().find(|(n, _)| *n == name).unwrap().1;
    Program::build(&Graph::from_text(src).unwrap()).unwrap()
}

fn world(n: u64) -> (Vec<u64>, MapHost) {
    let mut host = MapHost::default();
    let mut ids = Vec::new();
    for i in 0..n {
        let e = (1 << 32) | i;
        let f = i as f64;
        host = host
            .with(e, "agent", "pos", Value::Vec2(V2 { x: (f * 0.37).sin() * 9.0, y: f * 0.01 }))
            .with(e, "agent", "vel", Value::Vec2(V2 { x: (f * 1.3).cos() * 30.0, y: -1.0 }))
            .with(e, "agent", "bounces", Value::Int(0))
            .with(e, "agent", "dist", Value::Float(0.0));
        ids.push(e);
    }
    (ids, host)
}

#[test]
fn swarm_tick_runs_as_a_kernel_with_identical_results() {
    let lowered = lower(&program("swarm"), "tick").unwrap();
    assert_eq!(lowered.kernel.kernel.columns.len(), 6);
    let (ids, start) = world(2000);
    let dts = [1.0 / 60.0, 1.0 / 30.0, 0.013, 0.02];

    let mut by_graph = start.clone();
    let mut instances: Vec<Interp> =
        ids.iter().map(|e| Interp::new(program("swarm"), *e)).collect();
    for dt in dts.iter().cycle().take(120) {
        for g in &mut instances {
            g.fire("tick", &[Value::Float(*dt)], &mut by_graph).unwrap();
        }
    }
    for threads in [1, 4] {
        let pool = JobPool::new(threads);
        let mut by_kernel = start.clone();
        for dt in dts.iter().cycle().take(120) {
            lowered.run(&ids, &[Value::Float(*dt)], &mut by_kernel, &pool).unwrap();
        }
        assert_eq!(by_kernel.fields, by_graph.fields, "threads {threads}");
    }
    let bounced =
        by_graph.fields.iter().filter(|((_, _, f), v)| f == "bounces" && v.int() > 0).count();
    assert!(bounced > 500, "{bounced}");
}

#[test]
fn compiled_swarm_matches_too() {
    let lowered = lower(&program("swarm"), "tick").unwrap();
    let (ids, start) = world(300);
    let mut a = start.clone();
    let mut b = start;
    let mut compiled: Vec<_> =
        ids.iter().map(|e| seal_graph_tests::create("swarm", *e).unwrap()).collect();
    let pool = JobPool::new(2);
    for _ in 0..50 {
        for g in &mut compiled {
            g.fire("tick", &[Value::Float(0.05)], &mut a).unwrap();
        }
        lowered.run(&ids, &[Value::Float(0.05)], &mut b, &pool).unwrap();
    }
    assert_eq!(a.fields, b.fields);
}

#[test]
fn graphs_that_cannot_be_kernels_say_why() {
    let why = |name: &str, event: &str| lower(&program(name), event).unwrap_err();
    assert!(
        why("mover", "tick").contains("branches cannot run as a kernel"),
        "{}",
        why("mover", "tick")
    );
    assert!(why("counter", "tick").contains("graph variables"), "{}", why("counter", "tick"));
    assert!(why("fizz", "run").contains("loops cannot run as a kernel"), "{}", why("fizz", "run"));
    assert!(why("swarm", "nope").contains("no event"));
}

#[test]
fn swarm_patterns_are_reported_by_name() {
    let lowered = lower(&program("swarm"), "tick").unwrap();
    let found = lowered.patterns();
    eprintln!("PATTERNS {found:?}");
    for (name, _) in &found {
        assert!(name.starts_with("agent."), "{name}");
    }
}

#[test]
fn pulse_timer_is_proven_and_the_wheel_matches_the_kernel() {
    use seal_predict::{Pattern, TimerBank};
    let lowered = lower(&program("pulse"), "tick").unwrap();
    let found = lowered.patterns();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "agent.t");
    let Pattern::Timer { .. } = found[0].1 else { panic!("{:?}", found[0].1) };

    let ids: Vec<u64> = (0..50).map(|i| (1 << 32) | i).collect();
    let mut host = MapHost::default();
    for (i, e) in ids.iter().enumerate() {
        host = host.with(*e, "agent", "t", Value::Int((i as i64 * 7) % 20));
    }
    let starts: Vec<u64> = (0..50).map(|i| (i * 7) % 20).collect();
    let mut bank = TimerBank::new(&found[0].1, &starts, &[]).expect("bank");
    let pool = JobPool::new(2);
    for tick in 1..=57u64 {
        lowered.run(&ids, &[], &mut host, &pool).unwrap();
        bank.advance(1);
        let want: Vec<u64> = ids
            .iter()
            .map(|e| host.fields[&(*e, "agent".to_string(), "t".to_string())].int() as u64)
            .collect();
        assert_eq!(bank.values(), want, "tick {tick}");
    }
}

#[test]
fn wheel_run_replaces_per_tick_work_with_the_same_world() {
    use seal_graph::rt::Host;
    let lowered = lower(&program("pulse"), "tick").unwrap();
    let ids: Vec<u64> = (0..40).map(|i| (1 << 32) | i).collect();
    let mut start = MapHost::default();
    for (i, e) in ids.iter().enumerate() {
        start = start.with(*e, "agent", "t", Value::Int((i as i64 * 5) % 20));
    }
    let pool = JobPool::new(2);
    let mut per_tick = start.clone();
    let mut on_wheel = start.clone();
    let mut run = lowered.wheel_run(&ids, &mut on_wheel).unwrap().expect("wheel");
    for ticks in [1u64, 4, 9, 30] {
        for _ in 0..ticks {
            lowered.run(&ids, &[], &mut per_tick, &pool).unwrap();
        }
        run.advance(&lowered, &ids, ticks, &mut on_wheel).unwrap();
        assert_eq!(per_tick.fields, on_wheel.fields, "after {ticks}");
    }
    let swarm = lower(&program("swarm"), "tick").unwrap();
    assert!(swarm.wheel_run(&ids, &mut start.clone()).unwrap().is_none());
    let _ = start.get(ids[0], "agent", "t", seal_graph::Ty::Int);
}
