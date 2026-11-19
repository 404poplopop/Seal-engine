//! Replays are bit-identical across runs, between the interpreter and
//! compiled code, through a save and load, and across machines (the golden
//! hashes below are checked on Linux and Windows).

use seal_graph::replay::{Input, Recording};
use seal_graph::rt::{GraphInstance, MapHost, Value, V2, V3};
use seal_graph::{Elem, Graph, Interp, Program};

const ME: u64 = (3 << 32) | 11;

fn interp(name: &str) -> Interp {
    let src = seal_graph_tests::SOURCES.iter().find(|(n, _)| *n == name).unwrap().1;
    Interp::new(Program::build(&Graph::from_text(src).unwrap()).unwrap(), ME)
}

fn host() -> MapHost {
    MapHost::default()
        .prefab("coin", &[("coin", "value", Value::Int(10))])
        .prefab("enemy", &[])
        .prefab("boss", &[])
        .based_on("boss", "enemy")
        .with(ME, "mover", "pos", Value::Vec2(V2 { x: -1.25, y: 0.5 }))
        .with(ME, "mover", "vel", Value::Vec2(V2 { x: 2.9, y: 0.1 }))
        .with(ME, "agent", "pos", Value::Vec2(V2 { x: 3.0, y: -2.0 }))
        .with(ME, "agent", "vel", Value::Vec2(V2 { x: 41.0, y: 0.5 }))
        .with(ME, "agent", "bounces", Value::Int(0))
        .with(ME, "agent", "dist", Value::Float(0.0))
}

/// A long, varied session from a fixed-seed generator: uneven frame times,
/// events with arguments, and runs that hit the step budget.
fn session(name: &str) -> Recording {
    let mut g = seal_graph_tests::create(name, ME).unwrap();
    g.set_budget(4000);
    let mut h = host();
    let mut rec = Recording::new(name);
    let mut seed = 0x5ea1_u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        seed >> 33
    };
    for _ in 0..600 {
        let r = next();
        let _ = match r % 10 {
            0..=2 if name == "lists" => {
                rec.fire(g.as_mut(), "add", &[Value::Int((r % 9) as i64 - 2)], &mut h)
            }
            3 if name == "lists" => rec.fire(
                g.as_mut(),
                "edit",
                &[Value::Int((r % 7) as i64 - 1), Value::Int((r % 5) as i64)],
                &mut h,
            ),
            4 | 5 if name == "lists" => {
                rec.fire(g.as_mut(), "query", &[Value::Int((r % 8) as i64 - 1)], &mut h)
            }
            6 if name == "lists" => {
                rec.fire(g.as_mut(), "drop", &[Value::Int((r % 6) as i64 - 1)], &mut h)
            }
            7 if name == "lists" => rec.fire(g.as_mut(), "sum", &[], &mut h),
            8 if name == "lists" => {
                rec.fire(g.as_mut(), "make", &[Value::Float((r % 41) as f64 / 8.0 - 2.5)], &mut h)
            }
            0 if name == "kinds" => rec.fire(g.as_mut(), "boss", &[], &mut h),
            1 if name == "kinds" => rec.fire(g.as_mut(), "coin", &[], &mut h),
            2 if name == "kinds" => rec.fire(g.as_mut(), "kill", &[], &mut h),
            3..=5 if name == "kinds" => rec.fire(g.as_mut(), "check", &[], &mut h),
            1 | 4 if name == "dice" => rec.fire(g.as_mut(), "roll", &[], &mut h),
            3 if name == "clock" => rec.fire(g.as_mut(), "show", &[], &mut h),
            0 if name == "census" => rec.fire(g.as_mut(), "boss", &[], &mut h),
            1 if name == "census" => rec.fire(g.as_mut(), "coin", &[], &mut h),
            2 if name == "census" => rec.fire(g.as_mut(), "kill", &[], &mut h),
            6 if name == "census" => rec.fire(g.as_mut(), "first", &[], &mut h),
            3..=5 if name == "census" => rec.fire(g.as_mut(), "count", &[], &mut h),
            0 | 1 if name == "spawner" => rec.fire(g.as_mut(), "make", &[], &mut h),
            2 if name == "spawner" => rec.fire(g.as_mut(), "kill", &[], &mut h),
            3 if name == "spawner" => rec.fire(g.as_mut(), "check", &[], &mut h),
            3 if name == "timers" => rec.fire(
                g.as_mut(),
                "arm",
                &[
                    Value::Float((r % 300) as f64 / 100.0 - 0.2),
                    Value::Bool(r % 3 != 0),
                    Value::Int((r % 50) as i64),
                ],
                &mut h,
            ),
            9 if name == "timers" => rec.fire(g.as_mut(), "disarm", &[], &mut h),
            0 => rec.fire(g.as_mut(), "hit", &[Value::Int((r % 7) as i64 - 2)], &mut h),
            1 => rec.fire(g.as_mut(), "run", &[Value::Int((r % 30) as i64)], &mut h),
            2 => rec.fire(g.as_mut(), "dive", &[Value::Int((r % 3000) as i64)], &mut h),
            3 => rec.fire(g.as_mut(), "done", &[], &mut h),
            4 if r % 50 == 4 => rec.fire(g.as_mut(), "spin", &[], &mut h),
            5 if name == "funcs" => rec.fire(g.as_mut(), "go", &[], &mut h),
            6 if name == "funcs" => {
                rec.fire(g.as_mut(), "first_at_least", &[Value::Int((r % 12) as i64)], &mut h)
            }
            9 if name == "early" => {
                rec.fire(g.as_mut(), "run", &[Value::Int((r % 40) as i64 - 3)], &mut h)
            }
            8 if name == "space" => {
                let f = (r % 2001) as f64 / 37.0 - 27.0;
                let v = Value::Vec3(V3 { x: f, y: 1.0 - f, z: f * 0.5 });
                let w = Value::Vec3(V3 { x: -f, y: 2.0, z: f });
                rec.fire(g.as_mut(), "calc", &[v, w, Value::Float(f)], &mut h)
            }
            8 if name == "mathops" => {
                let f = (r % 2001) as f64 / 37.0 - 27.0;
                let v = Value::Vec2(V2 { x: f, y: 1.0 - f });
                rec.fire(g.as_mut(), "calc", &[Value::Float(f), v.clone(), v], &mut h)
            }
            8 if name == "transc" => {
                let f = (r % 2001) as f64 / 37.0 - 27.0;
                rec.fire(g.as_mut(), "calc", &[Value::Float(f), Value::Float(f / 20.0)], &mut h)
            }
            8 if name == "numparse" => {
                let words = ["42", " -7.5e1x", "abc", "", ".25", "+9"];
                rec.fire(g.as_mut(), "read", &[Value::Str(words[(r % 6) as usize].into())], &mut h)
            }
            8 if name == "numtext" => {
                let f = (r % 4_000_001) as f64 / 7.0 - 285_000.0;
                rec.fire(
                    g.as_mut(),
                    "show",
                    &[Value::Int(r as i64 - 2_000_000_000), Value::Float(f)],
                    &mut h,
                )
            }
            8 if name == "vectext" => {
                let f = (r % 2_000_001) as f64 / 7.0 - 140_000.0;
                let v = Value::Vec3(V3 { x: f, y: -f / 3.0, z: f * 1e-6 });
                rec.fire(g.as_mut(), "show", &[v, Value::Vec2(V2 { x: f / 11.0, y: -0.0 })], &mut h)
            }
            8 if name == "players" => {
                rec.fire(g.as_mut(), "who", &[Value::Int(r as i64 % 4 - 1)], &mut h)
            }
            8 if name == "strops" => {
                let words = ["", "Seal", "héllo Ünï", "ABC def", "ß", "x y z"];
                rec.fire(g.as_mut(), "say", &[Value::Str(words[(r % 6) as usize].into())], &mut h)
            }
            8 if name == "strsearch" => {
                let words = ["", "Seal", "héllo Ünï", "ABC def", "ß", "x y z"];
                let (s, t) = (words[(r % 6) as usize], words[((r / 6) % 6) as usize]);
                rec.fire(g.as_mut(), "find", &[Value::Str(s.into()), Value::Str(t.into())], &mut h)
            }
            7 if name == "funcs" => {
                rec.fire(g.as_mut(), "show_fact", &[Value::Int((r % 25) as i64)], &mut h)
            }
            _ => rec.advance(g.as_mut(), 1.0 / 60.0 + (r % 9) as f64 * 1e-4, &mut h),
        };
    }
    rec
}

fn play(rec: &Recording, g: &mut dyn GraphInstance) -> Vec<u64> {
    g.set_budget(4000);
    rec.play(g, &mut host())
}

#[test]
fn recordings_survive_save_and_load_exactly() {
    for name in seal_graph_tests::GRAPHS {
        let rec = session(name);
        let text = rec.to_text();
        let back = Recording::from_text(&text).unwrap();
        assert_eq!(back, rec);
        assert_eq!(back.to_text(), text);
    }
    let mut odd = Recording::new("x");
    odd.inputs.push(Input::Fire {
        event: "e".into(),
        args: vec![
            Value::Float(-0.0),
            Value::Float(f64::NAN),
            Value::Vec2(V2 { x: 1e300, y: -1e-300 }),
            Value::Entity(9),
            Value::List(Elem::Str, vec![Value::Str("a, ]\"b".into()), Value::Str(String::new())]),
            Value::List(Elem::Vec3, vec![]),
        ],
    });
    assert_eq!(Recording::from_text(&odd.to_text()).unwrap(), odd);
    assert!(Recording::from_text("schema_version = 2\nseal_replay = \"x\"\ninputs = []").is_err());
}

#[test]
fn replays_are_identical_every_time_and_in_both_backends() {
    for name in seal_graph_tests::GRAPHS {
        let rec = Recording::from_text(&session(name).to_text()).unwrap();
        let a = play(&rec, seal_graph_tests::create(name, ME).unwrap().as_mut());
        let b = play(&rec, seal_graph_tests::create(name, ME).unwrap().as_mut());
        let c = play(&rec, &mut interp(name));
        assert_eq!(a, b, "{name}: compiled replays differ");
        assert_eq!(a, c, "{name}: interpreter replay differs from compiled");
    }
}

#[test]
fn replay_hashes_match_on_every_machine() {
    let golden: Vec<(&str, u64)> = seal_graph_tests::GRAPHS
        .iter()
        .map(|name| (*name, *play(&session(name), &mut interp(name)).last().unwrap()))
        .collect();
    assert_eq!(golden, GOLDEN);
}

const GOLDEN: &[(&str, u64)] = &[
    ("census", 0x5977ae87b216b9be),
    ("clock", 0x2f6c9aadeb712649),
    ("counter", 0x5d8fa004239fe7eb),
    ("dice", 0x3639681edea58bd4),
    ("early", 0xc3a5b8e666dbaeb0),
    ("fizz", 0x2f1d9837b79537f0),
    ("fnbody", 0x9a512ebde990c606),
    ("funcs", 0xa9eb5b21508b3e3),
    ("kinds", 0xeb2f742ce684eb2e),
    ("lists", 0x645cb0d65c58934e),
    ("mathops", 0xd92100a884701153),
    ("mover", 0x3ddd3422a27581dd),
    ("numparse", 0x9a7031b335c8b8d0),
    ("numtext", 0xe82b52cf634b145d),
    ("players", 0x8b26a3eca84d9903),
    ("pulse", 0x2e64ddb77ab302c),
    ("queries", 0xbaebdfb421029c58),
    ("rules", 0x5179851f17731066),
    ("space", 0xc05d2310598e41c6),
    ("spawner", 0x5cb4209362339bf4),
    ("states", 0xe504455b9480600b),
    ("strops", 0x963b4fdb69a14064),
    ("strsearch", 0xf09d02174fdc7213),
    ("swarm", 0x321cfb5d8b3dfea1),
    ("timers", 0xb1c8e2b11434a7c1),
    ("transc", 0x1f8263c0abbec08c),
    ("vectext", 0xfa58358617167d07),
];

#[test]
fn replays_carry_the_players() {
    let mut rec = Recording::new("players").with_players(&[7, 3]);
    let mut live = MapHost { players: vec![7, 3], alive: [7, 3].into(), ..MapHost::default() };
    let mut g = interp("players");
    for i in [0, 1, 2] {
        rec.fire(&mut g, "who", &[Value::Int(i)], &mut live).unwrap();
    }
    let back = Recording::from_text(&rec.to_text()).unwrap();
    assert_eq!(back, rec);
    assert!(rec.to_text().contains("players"));
    assert!(!Recording::new("x").to_text().contains("players"), "an empty list is left out");
    let mut fresh = MapHost::default();
    back.play(&mut interp("players"), &mut fresh);
    assert_eq!(fresh.logs, live.logs);
    assert_eq!(fresh.logs, ["true7v0", "true3v0", "falsenone"]);
}
