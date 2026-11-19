use seal_scene::sequence::{Ease, PropValue, Sequence};

fn num(v: f32) -> PropValue {
    PropValue::Number(v)
}

fn close(got: f32, want: f32) {
    let wrapped = (got - want).rem_euclid(360.0);
    let delta = if wrapped > 180.0 { wrapped - 360.0 } else { wrapped };
    assert!(delta.abs() < 1e-3, "{got} != {want}");
}

fn at(seq: &Sequence, path: &str, t: f32) -> PropValue {
    let value = seq
        .properties
        .iter()
        .find(|track| track.path == path)
        .and_then(|track| track.sample(seq.local_time(t)));
    value.unwrap_or_else(|| panic!("no sample for {path} at {t}"))
}

#[test]
fn numbers_vectors_colours_and_angles_follow_the_easing() {
    let text = r#"
length = 1
loop = false
properties = [
  { entity = "A", path = "n", keys = [
    { time = 0, ease = "linear", value = { type = "number", value = 0 } }
    { time = 1, ease = "linear", value = { type = "number", value = 10 } }
  ] }
  { entity = "A", path = "in", keys = [
    { time = 0, ease = "linear", value = { type = "number", value = 0 } }
    { time = 1, ease = "ease-in", value = { type = "number", value = 10 } }
  ] }
  { entity = "A", path = "out", keys = [
    { time = 0, ease = "linear", value = { type = "number", value = 0 } }
    { time = 1, ease = "ease-out", value = { type = "number", value = 10 } }
  ] }
  { entity = "A", path = "inout", keys = [
    { time = 0, ease = "linear", value = { type = "number", value = 0 } }
    { time = 1, ease = "ease-in-out", value = { type = "number", value = 10 } }
  ] }
  { entity = "A", path = "hold", keys = [
    { time = 0, ease = "step", value = { type = "number", value = 1 } }
    { time = 1, ease = "step", value = { type = "number", value = 9 } }
  ] }
  { entity = "A", path = "vec", keys = [
    { time = 0, ease = "linear", value = { type = "vector", value = [0, 10, -2] } }
    { time = 1, ease = "linear", value = { type = "vector", value = [10, 0, 2] } }
  ] }
  { entity = "A", path = "col", keys = [
    { time = 0, ease = "linear", value = { type = "color", value = [0, 0, 0, 1] } }
    { time = 1, ease = "linear", value = { type = "color", value = [1, 0, 0, 0] } }
  ] }
  { entity = "A", path = "turn", keys = [
    { time = 0, ease = "linear", value = { type = "angle", value = 350 } }
    { time = 1, ease = "linear", value = { type = "angle", value = 10 } }
  ] }
  { entity = "A", path = "turns", keys = [
    { time = 0, ease = "linear", value = { type = "angles", value = [350, 0, 90] } }
    { time = 1, ease = "linear", value = { type = "angles", value = [10, 0, 180] } }
  ] }
]
"#;
    let seq = Sequence::parse(text).unwrap();
    assert_eq!(at(&seq, "n", 0.5), num(5.0));
    assert_eq!(at(&seq, "in", 0.5), num(2.5));
    assert_eq!(at(&seq, "out", 0.5), num(7.5));
    assert_eq!(at(&seq, "inout", 0.5), num(5.0));
    assert_eq!(at(&seq, "hold", 0.9), num(1.0));
    assert_eq!(at(&seq, "hold", 1.0), num(9.0));
    assert_eq!(at(&seq, "vec", 0.5), PropValue::Vector(vec![5.0, 5.0, 0.0]));
    assert_eq!(at(&seq, "col", 0.5), PropValue::Color(vec![0.5, 0.0, 0.0, 0.5]));
    match at(&seq, "turn", 0.5) {
        PropValue::Angle(v) => close(v, 0.0),
        other => panic!("angle became {other:?}"),
    }
    match at(&seq, "turns", 0.5) {
        PropValue::Angles(v) => {
            close(v[0], 0.0);
            close(v[1], 0.0);
            close(v[2], 135.0);
        }
        other => panic!("angles became {other:?}"),
    }
    let _ = Ease::Linear;
}

#[test]
fn booleans_enums_and_text_always_step() {
    let text = r#"
length = 1
properties = [
  { entity = "A", path = "flag", keys = [
    { time = 0, ease = "linear", value = { type = "bool", value = false } }
    { time = 1, ease = "ease-in-out", value = { type = "bool", value = true } }
  ] }
  { entity = "A", path = "mode", keys = [
    { time = 0, ease = "linear", value = { type = "text", value = "loop" } }
    { time = 0.4, ease = "linear", value = { type = "text", value = "once" } }
  ] }
]
"#;
    let seq = Sequence::parse(text).unwrap();
    assert_eq!(at(&seq, "flag", 0.99), PropValue::Bool(false));
    assert_eq!(at(&seq, "flag", 1.0), PropValue::Bool(true));
    assert_eq!(seq.properties[0].keys[1].ease, Ease::Step);
    assert_eq!(at(&seq, "mode", 0.39), PropValue::Text("loop".into()));
    assert_eq!(at(&seq, "mode", 0.4), PropValue::Text("once".into()));
}

#[test]
fn sealtext_round_trips_and_old_files_still_load() {
    let old = "length 4\nloop false\nobject Door\ntrack position.x cubic\nkey 0 0\nkey 2 10\ntrack visible step\nkey 0 1\nkey 1 0\nevent 1 opened\n";
    let parsed = Sequence::parse(old).unwrap();
    assert!(parsed.properties.is_empty());
    assert!(parsed.sample(1.0).contains(&("Door", "position.x", 5.0)));
    let again = Sequence::parse(&parsed.to_text()).unwrap();
    assert_eq!(again.tracks, parsed.tracks);
    assert_eq!(again.events, parsed.events);
    assert_eq!(again.length, parsed.length);
    assert_eq!(again.looping, parsed.looping);

    let typed = r#"
length = 2
loop = true
tracks = [
  { object = "Door", target = "position.x", easing = "linear", keys = [
    { time = 0, value = 0 }
    { time = 2, value = 8 }
  ] }
]
properties = [
  { entity = "Door", path = "camera2d.zoom", keys = [
    { time = 0, ease = "linear", value = { type = "number", value = 1 } }
    { time = 2, ease = "ease-in", value = { type = "number", value = 3 } }
  ] }
]
events = [ { time = 1, name = "opened" } ]
"#;
    let seq = Sequence::parse(typed).unwrap();
    let back = Sequence::parse(&seq.to_text()).unwrap();
    assert_eq!(back, seq);
    assert!(seq.sample(1.0).contains(&("Door", "position.x", 4.0)));
}

#[test]
fn a_damaged_sequence_file_is_an_error() {
    let broken = [
        "length = \n",
        "length = true\n",
        "length = 1\nproperties = 4\n",
        "length = 1\nproperties = [ { entity = \"A\", path = \"camera2d.zoom\", keys = [ { time = 1, ease = \"bogus\", value = { type = \"number\", value = 1 } } ] } ]\n",
        "length = 1\nproperties = [ { entity = \"A\", path = \"camera2d.zoom\", keys = [ { time = 1, ease = \"linear\", value = { type = \"number\", value = 1 } }, { time = 1, ease = \"linear\", value = { type = \"number\", value = 2 } } ] } ]\n",
        "length = 1\ntracks = 1\n",
    ];
    for text in broken {
        let err = Sequence::parse(text).expect_err(text);
        assert!(!err.is_empty(), "{text}");
    }
    assert!(Sequence::parse("track x linear").unwrap_err().contains("line 1"));
}
