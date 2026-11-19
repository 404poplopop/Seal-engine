//! Phases, system order, fixed steps and Tick sugar.
use std::sync::{Arc, Mutex};

use seal_ecs::{Phase, Prefab, PrefabLibrary, Schedule, Time, World};

fn log_system(
    log: &Arc<Mutex<Vec<String>>>,
    name: &str,
) -> impl FnMut(&mut World) + Send + 'static {
    let log = log.clone();
    let name = name.to_string();
    move |_| log.lock().unwrap().push(name.clone())
}

#[test]
fn phases_and_dependencies_set_the_order() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut s = Schedule::new();
    s.add_system("late", Phase::LateUpdate, log_system(&log, "late"));
    s.add_system("move", Phase::PreSimulation, log_system(&log, "move")).after("aim");
    s.add_system("aim", Phase::PreSimulation, log_system(&log, "aim"));
    s.add_system("input", Phase::Input, log_system(&log, "input")).before("late");
    let mut w = World::new();
    s.run_frame(&mut w, 0.0).unwrap();
    assert_eq!(*log.lock().unwrap(), ["input", "aim", "move", "late"]);

    let mut bad = Schedule::new();
    bad.add_system("a", Phase::Input, |_| {}).after("b");
    bad.add_system("b", Phase::Input, |_| {}).after("a");
    assert!(bad.build().unwrap_err().contains("loop"));
    let mut bad = Schedule::new();
    bad.add_system("a", Phase::Input, |_| {}).after("ghost");
    assert!(bad.build().unwrap_err().contains("unknown"));
    let mut bad = Schedule::new();
    bad.add_system("a", Phase::LateUpdate, |_| {}).before("b");
    bad.add_system("b", Phase::Input, |_| {});
    assert!(bad.build().unwrap_err().contains("comes after"));
}

#[test]
fn fixed_steps_follow_real_time() {
    let steps = Arc::new(Mutex::new(0u32));
    let mut s = Schedule::new();
    let counter = steps.clone();
    s.add_system("sim", Phase::FixedSimulation, move |w| {
        assert_eq!(w.resource::<Time>().unwrap().dt(), 1.0 / 60.0);
        *counter.lock().unwrap() += 1;
    });
    let mut w = World::new();
    assert_eq!(s.run_frame(&mut w, 0.05).unwrap(), 3);
    let t = w.resource::<Time>().unwrap();
    assert!((t.alpha - 0.0).abs() < 1e-6 || (t.alpha - 1.0).abs() < 1e-6, "alpha {}", t.alpha);
    assert_eq!(s.run_frame(&mut w, 0.01).unwrap(), 0);
    assert!((w.resource::<Time>().unwrap().alpha - 0.6).abs() < 1e-6);
    w.resource_mut::<Time>().unwrap().paused = true;
    assert_eq!(s.run_frame(&mut w, 1.0).unwrap(), 0);
    let t = w.resource_mut::<Time>().unwrap();
    t.paused = false;
    t.scale = 2.0;
    t.accumulator = 0.0;
    assert_eq!(s.run_frame(&mut w, 0.05).unwrap(), 6, "time dilation doubles the steps");
    // A huge hitch is capped and the backlog dropped.
    assert_eq!(s.run_frame(&mut w, 10.0).unwrap(), 8);
    assert!(w.resource::<Time>().unwrap().accumulator < 1.0 / 60.0);
    assert_eq!(*steps.lock().unwrap(), 17);
    assert_eq!(w.resource::<Time>().unwrap().frame, 5);
}

#[derive(Clone, Debug, Default)]
struct Spinner(f64);
seal_ecs::text_component!(Spinner as "spinner" newtype);

#[derive(Clone, Debug, Default)]
struct Ticked(u32);

#[test]
fn tick_sugar_visits_every_instance() {
    let mut w = World::new();
    w.register_core();
    w.register_text::<Spinner>();
    let a = w.spawn((Spinner(0.0),));
    let b = w.spawn((Spinner(0.0),));
    let none = w.spawn(());
    let mut s = Schedule::new();
    s.add_tick::<Spinner>("spin", Phase::FixedSimulation, |w, e, dt| {
        w.get_mut::<Spinner>(e).unwrap().0 += dt
    });

    // Class Tick: every Unit, including subclasses, but not Cameras.
    let mut lib = PrefabLibrary::new();
    lib.insert(Prefab::from_text("seal_prefab = 1\nprefab = \"Unit\"\n").unwrap());
    lib.insert(
        Prefab::from_text("seal_prefab = 1\nprefab = \"Character\"\nbase = \"Unit\"\n").unwrap(),
    );
    lib.insert(Prefab::from_text("seal_prefab = 1\nprefab = \"Camera\"\n").unwrap());
    let unit = w.instantiate(&lib, "Unit").unwrap();
    let hero = w.instantiate(&lib, "Character").unwrap();
    let cam = w.instantiate(&lib, "Camera").unwrap();
    s.add_class_tick("pawn_tick", Phase::PostSimulation, Arc::new(lib), "Unit", |w, e, _| {
        let n = w.get::<Ticked>(e).map_or(0, |t| t.0);
        w.insert(e, Ticked(n + 1));
    });
    s.run_frame(&mut w, 2.0 / 60.0 + 1e-9).unwrap();
    assert!((w.get::<Spinner>(a).unwrap().0 - 2.0 / 60.0).abs() < 1e-12);
    assert!((w.get::<Spinner>(b).unwrap().0 - 2.0 / 60.0).abs() < 1e-12);
    assert!(w.get::<Spinner>(none).is_none());
    assert_eq!(w.get::<Ticked>(unit).unwrap().0, 1);
    assert_eq!(w.get::<Ticked>(hero).unwrap().0, 1);
    assert!(w.get::<Ticked>(cam).is_none());
}

#[test]
fn shuffling_keeps_declared_order_and_is_repeatable() {
    let mut s = Schedule::new();
    for name in ["a", "b", "c", "d", "e", "f"] {
        s.add_system(name, Phase::LateUpdate, |_| {});
    }
    s.add_system("last", Phase::LateUpdate, |_| {}).after("a").after("f");
    let plain: Vec<String> =
        s.order(Phase::LateUpdate).unwrap().into_iter().map(String::from).collect();
    let mut seen = std::collections::HashSet::new();
    for seed in 0..20 {
        s.shuffle_independent(Some(seed));
        let one: Vec<String> =
            s.order(Phase::LateUpdate).unwrap().into_iter().map(String::from).collect();
        s.shuffle_independent(Some(seed));
        let two: Vec<String> =
            s.order(Phase::LateUpdate).unwrap().into_iter().map(String::from).collect();
        assert_eq!(one, two);
        let pos = |n: &str| one.iter().position(|x| x == n).unwrap();
        assert!(pos("last") > pos("a") && pos("last") > pos("f"));
        seen.insert(one);
    }
    assert!(seen.len() > 5, "shuffling should actually vary the order");
    s.shuffle_independent(None);
    assert_eq!(s.order(Phase::LateUpdate).unwrap(), plain);
}

#[test]
fn systems_and_phases_show_up_in_traces() {
    let mut s = Schedule::new();
    s.add_system("traced_mover", Phase::PreSimulation, |_| {});
    let mut w = World::new();
    seal_trace::enable(seal_trace::Channel::CPU);
    s.run_frame(&mut w, 0.0).unwrap();
    seal_trace::enable(seal_trace::Channel::NONE);
    let names: Vec<_> = seal_trace::drain().summary().into_iter().map(|z| z.0).collect();
    assert!(names.contains(&"traced_mover"), "{names:?}");
    assert!(names.contains(&"PreSimulation"), "{names:?}");
}
