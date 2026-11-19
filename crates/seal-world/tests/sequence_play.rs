use seal_scene::sequence::Sequence;
use seal_world::World;

#[test]
fn a_sequence_drives_objects_fires_events_and_scrubs() {
    let mut w = World::demo();
    let (a, b) = (w.entities[1].id.clone(), w.entities[2].id.clone());
    let text = format!("length 1\nobject {a}\ntrack position.x linear\nkey 0 0\nkey 1 10\nobject {b}\ntrack visible step\nkey 0 1\nkey 0.5 0\nevent 0.5 half\nevent 1 end\n");
    let seq = Sequence::parse(&text).unwrap();
    let before = w.entities[1].translation;
    w.play();
    w.play_sequence(seq.clone());
    for _ in 0..30 {
        w.step(1.0 / 60.0);
    }
    assert!((w.entities[1].translation[0] - 5.0).abs() < 0.3);
    assert!(w.entities[2].hidden);
    assert_eq!(w.sequence_events, vec!["half"]);
    for _ in 0..60 {
        w.step(1.0 / 60.0);
    }
    assert_eq!(w.entities[1].translation[0], 10.0);
    assert_eq!(w.sequence_events, vec!["half", "end"]);
    assert!(w.sequences.is_empty());
    w.stop();
    assert_eq!(w.entities[1].translation, before);
    assert!(w.sequence_events.is_empty());
    w.scrub_sequence(&seq, 0.25);
    assert!((w.entities[1].translation[0] - 2.5).abs() < 1e-4);
    assert!(!w.entities[2].hidden);
}
