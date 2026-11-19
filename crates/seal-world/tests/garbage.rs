//! Loaders must turn broken files and packets into errors, never into a crash.

use seal_testkit::garbage::{survives, text};
use seal_world::net::{InputMsg, Snapshot};
use seal_world::particles::EmitterDef;
use seal_world::seal_scene::anim::Anim;
use seal_world::seal_scene::sequence::Sequence;
use seal_world::World;

#[test]
fn broken_scenes_are_refused_not_crashed_on() {
    let world = World::demo();
    survives("scene", world.to_text().as_bytes(), text(|t| drop(World::from_text(t))));
    survives(
        "legacy scene",
        world.to_legacy_text().as_bytes(),
        text(|t| drop(World::from_text(t))),
    );
    survives(
        "empty scene",
        World::empty_scene_text("Broken").as_bytes(),
        text(|t| drop(World::from_text(t))),
    );
}

#[test]
fn broken_sequences_animations_and_effects_are_refused_not_crashed_on() {
    let seq = "length 2\nobject Cube\ntrack position.x linear\nkey 0 0\nkey 2 8\ntrack visible step\nkey 1 0\nevent 1 half\n";
    survives("sequence", seq.as_bytes(), text(|t| drop(Sequence::parse(t))));
    let anim = "length 1\nloop true\ntrack position.y linear\nkey 0 0\nkey 1 2\ntrack sprite_frame step\nkey 0 0\nkey 0.5 1\n";
    survives("anim", anim.as_bytes(), text(|t| drop(Anim::parse(t))));
    let fx = "rate 40\nlifetime 1.2\nvelocity 0 2 0\nspread 0.5\ngravity 0 -3 0\nsize 0.2 0.05\ncolor 1 0.9 0.4 1 1 0.3 0.1 0\nmax 200\nseed 7\n";
    survives("effect", fx.as_bytes(), text(|t| drop(EmitterDef::parse(t))));
}

#[test]
fn broken_network_packets_are_refused_not_crashed_on() {
    let world = World::demo();
    let snapshot = Snapshot::capture(&world, 3, 1.5).encode();
    survives("snapshot", &snapshot, |b| drop(Snapshot::decode(b)));
    let input = InputMsg {
        seq: 9,
        input: seal_world::PlayInput { left: true, right: false, jump: true },
        keys: vec![("e".into(), true), ("space".into(), false)],
    }
    .encode();
    survives("input", &input, |b| drop(InputMsg::decode(b)));
}
