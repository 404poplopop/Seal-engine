use std::collections::HashMap;

use proptest::prelude::*;
use seal_ecs::{Entity, Ticks, World};
use seal_jobs::JobPool;

#[derive(Clone, Debug, PartialEq, Default)]
struct Pos(i64);
#[derive(Clone, Debug, PartialEq, Default)]
struct Vel(i64);
#[derive(Clone, Debug, PartialEq, Default)]
struct Name(String);
#[derive(Clone, Debug, PartialEq, Default)]
struct Transform(i32);
#[derive(Clone, Debug, PartialEq, Default)]
struct Collider(i32);
#[derive(Debug, Default)]
struct Rigidbody;

#[test]
fn stale_ids_never_reach_a_reused_slot() {
    let mut world = World::new();
    let a = world.spawn((Pos(1),));
    assert!(world.despawn(a));
    let b = world.spawn((Pos(2),));
    assert_eq!(a.index(), b.index());
    assert_ne!(a, b);
    assert!(world.get::<Pos>(a).is_none());
    assert!(!world.despawn(a));
    assert_eq!(world.get::<Pos>(b), Some(&Pos(2)));
    assert_eq!(Entity::from_bits(b.to_bits()), b);
}

#[test]
fn insert_and_remove_move_entities_between_tables() {
    let mut world = World::new();
    let e = world.spawn((Pos(1), Vel(2)));
    let other = world.spawn((Pos(10), Vel(20)));
    world.insert(e, Name("ship".into()));
    assert_eq!(world.get::<Name>(e).map(|n| n.0.as_str()), Some("ship"));
    assert_eq!(world.remove::<Vel>(e), Some(Vel(2)));
    assert!(!world.has::<Vel>(e));
    assert_eq!(world.get::<Pos>(e), Some(&Pos(1)));
    assert_eq!(world.get::<Vel>(other), Some(&Vel(20)));
    world.check_integrity().unwrap();
}

#[test]
fn queries_match_every_table_with_the_components_and_respect_filters() {
    let mut world = World::new();
    world.spawn((Pos(1), Vel(1)));
    world.spawn((Pos(2), Vel(2), Name("b".into())));
    world.spawn((Pos(3),));
    world.query::<(&mut Pos, &Vel)>().for_each(|(mut p, v)| p.0 += v.0 * 10);
    let mut all = world.query::<&Pos>().collect(|p| p.0);
    all.sort();
    assert_eq!(all, vec![3, 11, 22]);
    assert_eq!(world.query::<&Pos>().without::<Name>().count(), 2);
    assert_eq!(world.query::<&Pos>().with::<Name>().count(), 1);
}

#[test]
fn only_written_components_are_marked_changed() {
    let mut world = World::new();
    let a = world.spawn((Pos(0),));
    let b = world.spawn((Pos(5),));
    let seen = world.tick();
    world.advance_tick();
    world.query::<(Entity, &mut Pos)>().for_each(|(e, mut p)| {
        if e == a {
            p.0 += 1;
        } else {
            p.set_if_neq(Pos(5)); // same value: not a change
        }
    });
    assert_eq!(world.changed_since::<Pos>(seen), vec![a]);
    let ticks = world.query::<(Entity, Ticks<Pos>)>().collect(|(e, t)| (e, t.changed_since(seen)));
    assert!(ticks.contains(&(a, true)) && ticks.contains(&(b, false)));
}

#[test]
fn required_components_are_added_with_defaults() {
    let mut world = World::new();
    world.require::<Rigidbody, Transform>();
    world.require::<Rigidbody, Collider>();
    let e = world.spawn((Rigidbody,));
    assert_eq!(world.get::<Transform>(e), Some(&Transform(0)));
    assert_eq!(world.get::<Collider>(e), Some(&Collider(0)));
    let kept = world.spawn((Transform(7),));
    world.insert(kept, Rigidbody);
    assert_eq!(world.get::<Transform>(kept), Some(&Transform(7)));
    world.check_integrity().unwrap();
}

#[test]
fn parallel_queries_give_the_same_result_as_serial_ones() {
    let build = || {
        let mut world = World::new();
        world.spawn_batch((0..10_000).map(|i| (Pos(i), Vel(i % 7))));
        world.spawn_batch((0..5_000).map(|i| (Pos(i), Vel(3), Name(String::new()))));
        world
    };
    let mut serial = build();
    serial.query::<(&mut Pos, &Vel)>().for_each(|(mut p, v)| p.0 = p.0 * 3 + v.0);
    let expected = serial.query::<&Pos>().collect(|p| p.0);
    for threads in [0, 1, 4] {
        let pool = JobPool::new(threads);
        let mut world = build();
        world
            .query::<(&mut Pos, &Vel)>()
            .par_for_each(&pool, 333, |(mut p, v)| p.0 = p.0 * 3 + v.0);
        assert_eq!(world.query::<&Pos>().collect(|p| p.0), expected);
        let sums =
            world.query::<&Pos>().par_map_chunks(&pool, 500, |rows| rows.map(|p| p.0).sum::<i64>());
        assert_eq!(sums.iter().sum::<i64>(), expected.iter().sum::<i64>());
    }
}

#[test]
fn resources_are_singletons() {
    let mut world = World::new();
    world.insert_resource(Vel(9));
    world.resource_mut::<Vel>().unwrap().0 += 1;
    assert_eq!(world.resource::<Vel>(), Some(&Vel(10)));
    assert_eq!(world.remove_resource::<Vel>(), Some(Vel(10)));
    assert!(world.resource::<Vel>().is_none());
}

#[derive(Clone, Debug)]
enum Op {
    Spawn(u8, i64),
    Despawn(usize),
    InsertVel(usize, i64),
    RemoveVel(usize),
    InsertName(usize),
    RemovePos(usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0u8..4, any::<i64>()).prop_map(|(k, v)| Op::Spawn(k, v)),
        any::<usize>().prop_map(Op::Despawn),
        (any::<usize>(), any::<i64>()).prop_map(|(i, v)| Op::InsertVel(i, v)),
        any::<usize>().prop_map(Op::RemoveVel),
        any::<usize>().prop_map(Op::InsertName),
        any::<usize>().prop_map(Op::RemovePos),
    ]
}

#[derive(Clone, Default, Debug, PartialEq)]
struct Model {
    pos: Option<i64>,
    vel: Option<i64>,
    name: bool,
}

proptest! {
    /// Random spawn / despawn / insert / remove sequences against a plain model.
    #[test]
    fn the_world_matches_a_simple_model(ops in prop::collection::vec(op(), 1..200)) {
        let mut world = World::new();
        let mut model: HashMap<Entity, Model> = HashMap::new();
        let mut ids: Vec<Entity> = Vec::new();
        for op in ops {
            let pick = |i: usize| (!ids.is_empty()).then(|| ids[i % ids.len()]);
            match op {
                Op::Spawn(kind, v) => {
                    let (e, m) = match kind {
                        0 => (world.spawn((Pos(v),)), Model { pos: Some(v), ..Default::default() }),
                        1 => (world.spawn((Pos(v), Vel(-v))), Model { pos: Some(v), vel: Some(-v), name: false }),
                        2 => (world.spawn((Name(String::new()),)), Model { name: true, ..Default::default() }),
                        _ => (world.spawn((Vel(v), Name(String::new()), Pos(1))), Model { pos: Some(1), vel: Some(v), name: true }),
                    };
                    ids.push(e);
                    model.insert(e, m);
                }
                Op::Despawn(i) => if let Some(e) = pick(i) {
                    prop_assert_eq!(world.despawn(e), model.remove(&e).is_some());
                },
                Op::InsertVel(i, v) => if let Some(e) = pick(i) {
                    prop_assert_eq!(world.insert(e, Vel(v)), model.contains_key(&e));
                    if let Some(m) = model.get_mut(&e) { m.vel = Some(v); }
                },
                Op::RemoveVel(i) => if let Some(e) = pick(i) {
                    let expected = model.get_mut(&e).and_then(|m| m.vel.take());
                    prop_assert_eq!(world.remove::<Vel>(e).map(|v| v.0), expected);
                },
                Op::InsertName(i) => if let Some(e) = pick(i) {
                    world.insert(e, Name(String::new()));
                    if let Some(m) = model.get_mut(&e) { m.name = true; }
                },
                Op::RemovePos(i) => if let Some(e) = pick(i) {
                    let expected = model.get_mut(&e).and_then(|m| m.pos.take());
                    prop_assert_eq!(world.remove::<Pos>(e).map(|p| p.0), expected);
                },
            }
            prop_assert!(world.check_integrity().is_ok(), "{:?}", world.check_integrity());
        }
        prop_assert_eq!(world.len(), model.len());
        for (e, m) in &model {
            let actual = Model {
                pos: world.get::<Pos>(*e).map(|p| p.0),
                vel: world.get::<Vel>(*e).map(|v| v.0),
                name: world.has::<Name>(*e),
            };
            prop_assert_eq!(&actual, m);
        }
        let count = world.query::<&Pos>().count();
        prop_assert_eq!(count, model.values().filter(|m| m.pos.is_some()).count());
    }
}
