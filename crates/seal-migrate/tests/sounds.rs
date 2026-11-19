//! Playing sounds: PlaySound2D, PlaySoundAtLocation and SpawnSound2D start
//! the chosen sound file; keeping the spawned sound component is reported.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value, V3};
use seal_migrate::{import, sound_path, Options};

#[test]
fn sound_calls_play_the_chosen_files() {
    let imp = import(
        include_str!("fixtures/sounds.txt"),
        &Options { name: "sounds".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost { alive: [1].into(), ..MapHost::default() };
    h.fields.insert(
        (1, "transform".into(), "translation".into()),
        Value::Vec3(V3 { x: 3.0, y: 4.0, z: 5.0 }),
    );
    vm.fire("Boom", &[], &mut h).unwrap();
    vm.fire("Ding", &[], &mut h).unwrap();
    assert_eq!(h.sounds, ["Audio/Boom 0.5 1", "Audio/Hit 1 1 at 3 4 5", "UI/Ding 1 1"]);
}

#[test]
fn keeping_the_spawned_sound_is_reported() {
    let imp = import(
        include_str!("fixtures/sounds_bad.txt"),
        &Options { name: "keep".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(!imp.report.complete(), "{text}");
    assert!(text.contains("spawned sound component"), "{text}");
}

#[test]
fn sound_paths_drop_the_game_root_and_object_name() {
    assert_eq!(sound_path("/Game/Audio/Jump.Jump"), "Audio/Jump");
    assert_eq!(sound_path("SoundWave'/Game/UI/Ding.Ding'"), "UI/Ding");
    assert_eq!(sound_path("\"/Game/A/B/C.C\""), "A/B/C");
}
