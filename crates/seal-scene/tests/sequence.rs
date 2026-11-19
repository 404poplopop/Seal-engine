use seal_scene::sequence::Sequence;

const TEXT: &str = "length 4\nobject A\ntrack position.x linear\nkey 0 0\nkey 2 10\nobject B\ntrack visible step\nkey 0 1\nkey 1 0\nevent 1 one\nevent 3 three\nevent 0 start\n";

#[test]
fn it_samples_each_objects_tracks() {
    let s = Sequence::parse(TEXT).unwrap();
    assert_eq!(s.tracks.len(), 2);
    let at1 = s.sample(1.0);
    assert!(at1.contains(&("A", "position.x", 5.0)));
    assert!(at1.contains(&("B", "visible", 0.0)));
    assert!(s.sample(0.5).contains(&("B", "visible", 1.0)));
    assert!(s.sample(9.0).contains(&("A", "position.x", 10.0)));
}

#[test]
fn events_fire_once_in_order_and_not_when_scrubbing_back() {
    let s = Sequence::parse(TEXT).unwrap();
    assert_eq!(s.events_between(-1.0, 0.0), vec!["start"]);
    assert_eq!(s.events_between(0.0, 1.0), vec!["one"]);
    assert_eq!(s.events_between(0.0, 4.0), vec!["one", "three"]);
    assert!(s.events_between(3.0, 1.0).is_empty());
    assert!(s.events_between(1.0, 1.0).is_empty());
}

#[test]
fn a_looping_sequence_wraps_events() {
    let mut s = Sequence::parse(TEXT).unwrap();
    s.looping = true;
    assert_eq!(s.events_between(2.5, 5.5), vec!["three", "start", "one"]);
}

#[test]
fn bad_files_give_line_errors() {
    assert!(Sequence::parse("track x linear").unwrap_err().contains("line 1"));
    assert!(Sequence::parse("object A\ntrack x bogus").is_err());
    assert!(Sequence::parse("object A\ntrack x linear\nkey 1 0\nkey 1 2").is_err());
}
