//! Hooks, observers and relationships.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use seal_ecs::{Entity, Relation, World};

#[derive(Clone, Debug, PartialEq)]
struct Health(i32);
#[derive(Clone, Debug, PartialEq, Default)]
struct Log(Vec<String>);

struct Owns;
impl Relation for Owns {
    const DESPAWN_TARGETS_WITH_SOURCE: bool = true;
}
struct Targeting;
impl Relation for Targeting {
    const EXCLUSIVE: bool = true;
}
struct Likes;
impl Relation for Likes {}

#[test]
fn hooks_fire_on_add_and_before_remove() {
    let mut w = World::new();
    w.insert_resource(Log::default());
    w.on_add::<Health>(|w, e| {
        let hp = w.get::<Health>(e).unwrap().0;
        w.resource_mut::<Log>().unwrap().0.push(format!("add {hp}"));
    });
    w.on_remove::<Health>(|w, e| {
        let hp = w.get::<Health>(e).unwrap().0; // still readable
        w.resource_mut::<Log>().unwrap().0.push(format!("remove {hp}"));
    });
    let a = w.spawn((Health(10),));
    w.insert(a, Health(5)); // replacing is not adding
    let b = w.spawn(());
    w.insert(b, Health(7));
    assert_eq!(w.remove::<Health>(b), Some(Health(7)));
    w.despawn(a);
    assert_eq!(w.resource::<Log>().unwrap().0, ["add 10", "add 7", "remove 7", "remove 5"]);
    w.check_integrity().unwrap();
}

#[test]
fn hooks_can_change_the_world_and_be_removed() {
    let mut w = World::new();
    // Dying at zero health despawns the entity from inside a hook.
    w.on_add::<Health>(|w, e| {
        if w.get::<Health>(e).unwrap().0 <= 0 {
            w.despawn(e);
        }
    });
    let dead = w.spawn((Health(0),));
    let alive = w.spawn((Health(3),));
    assert!(!w.contains(dead) && w.contains(alive));
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let id = w.on_remove::<Health>(move |_, _| {
        c.fetch_add(1, Ordering::Relaxed);
    });
    w.remove::<Health>(alive);
    assert!(w.unobserve(id) && !w.unobserve(id));
    w.insert(alive, Health(1));
    w.remove::<Health>(alive);
    assert_eq!(count.load(Ordering::Relaxed), 1);
    w.check_integrity().unwrap();
}

struct Damage {
    target: Entity,
    amount: i32,
}

#[test]
fn observers_receive_triggered_events() {
    let mut w = World::new();
    w.observe::<Damage>(|w, hit| {
        if let Some(h) = w.get_mut::<Health>(hit.target) {
            h.0 -= hit.amount;
        }
    });
    let e = w.spawn((Health(10),));
    assert_eq!(w.trigger(Damage { target: e, amount: 4 }), 1);
    assert_eq!(w.trigger(42u32), 0);
    assert_eq!(w.get::<Health>(e), Some(&Health(6)));
}

#[test]
fn relationships_stay_in_step_when_either_end_dies() {
    let mut w = World::new();
    w.register_relation::<Likes>();
    let (a, b, c) = (w.spawn(()), w.spawn(()), w.spawn(()));
    w.relate::<Likes>(a, b);
    w.relate::<Likes>(a, c);
    w.relate::<Likes>(c, b);
    assert_eq!(w.targets::<Likes>().of(&w, a), &[b, c]);
    assert_eq!(w.targets::<Likes>().sources_of(&w, b), &[a, c]);
    w.despawn(b);
    assert_eq!(w.targets::<Likes>().of(&w, a), &[c]);
    assert!(w.targets::<Likes>().of(&w, c).is_empty());
    w.despawn(a);
    assert!(w.targets::<Likes>().sources_of(&w, c).is_empty());
    assert!(!w.unrelate::<Likes>(c, c));
    w.check_integrity().unwrap();
}

#[test]
fn ownership_cascades_and_exclusive_links_replace() {
    let mut w = World::new();
    w.register_relation::<Owns>();
    w.register_relation::<Targeting>();
    let player = w.spawn(());
    let sword = w.spawn(());
    let gem = w.spawn(());
    let enemy1 = w.spawn(());
    let enemy2 = w.spawn(());
    w.relate::<Owns>(player, sword);
    w.relate::<Owns>(sword, gem);
    w.relate::<Targeting>(player, enemy1);
    w.relate::<Targeting>(player, enemy2);
    assert_eq!(w.targets::<Targeting>().of(&w, player), &[enemy2]);
    assert!(w.targets::<Targeting>().sources_of(&w, enemy1).is_empty());
    w.despawn(player);
    assert!(!w.contains(sword) && !w.contains(gem), "owned items go with their owner");
    assert!(w.targets::<Targeting>().sources_of(&w, enemy2).is_empty());
    assert_eq!(w.len(), 2);
    w.check_integrity().unwrap();
}

#[test]
fn relationships_survive_snapshots() {
    let mut w = World::new();
    w.register_relation::<Likes>();
    let (a, b) = (w.spawn(()), w.spawn(()));
    let snap = w.snapshot().unwrap();
    w.relate::<Likes>(a, b);
    w.restore(&snap);
    assert!(w.targets::<Likes>().of(&w, a).is_empty());
    w.relate::<Likes>(a, b);
    let snap = w.snapshot().unwrap();
    w.despawn(b);
    w.restore(&snap);
    assert_eq!(w.targets::<Likes>().of(&w, a), &[b]);
}
