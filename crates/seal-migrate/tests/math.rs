//! Math library functions: trig, rounding, lerp, clamp and 2D vectors must
//! import completely and compute what the classic functions compute.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

const MATH: &str = include_str!("fixtures/math.txt");

fn run() -> (Interp, MapHost) {
    let imp = import(MATH, &Options { name: "math".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let program = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    (Interp::new(program, 1), MapHost::default())
}

fn fire(vm: &mut Interp, host: &mut MapHost, event: &str, args: &[Value]) -> Vec<String> {
    let before = host.logs.len();
    vm.fire(event, args, host).unwrap();
    host.logs[before..].to_vec()
}

#[test]
fn degrees_sine() {
    let (mut vm, mut h) = run();
    assert_eq!(fire(&mut vm, &mut h, "Trig", &[Value::Float(30.0)]), ["500"]);
    assert_eq!(fire(&mut vm, &mut h, "Trig", &[Value::Float(90.0)]), ["1000"]);
    assert_eq!(fire(&mut vm, &mut h, "Trig", &[Value::Float(-45.0)]), ["-707"]);
}

#[test]
fn floor_round_ceil() {
    let (mut vm, mut h) = run();
    assert_eq!(fire(&mut vm, &mut h, "Rounding", &[Value::Float(-2.5)]), ["-3", "-3", "-2"]);
    assert_eq!(fire(&mut vm, &mut h, "Rounding", &[Value::Float(2.5)]), ["2", "3", "3"]);
    assert_eq!(fire(&mut vm, &mut h, "Rounding", &[Value::Float(7.0)]), ["7", "7", "7"]);
}

#[test]
fn lerp_and_exact_clamp() {
    let (mut vm, mut h) = run();
    let args = |a: f64, b: f64, t: f64, v: i64, lo: i64, hi: i64| {
        vec![
            Value::Float(a),
            Value::Float(b),
            Value::Float(t),
            Value::Int(v),
            Value::Int(lo),
            Value::Int(hi),
        ]
    };
    assert_eq!(fire(&mut vm, &mut h, "LerpClamp", &args(2.0, 6.0, 0.25, 15, 0, 10)), ["3", "10"]);
    assert_eq!(fire(&mut vm, &mut h, "LerpClamp", &args(10.0, 0.0, 1.5, -5, 0, 10)), ["-5", "0"]);
    assert_eq!(fire(&mut vm, &mut h, "LerpClamp", &args(0.0, 0.0, 0.0, 5, 0, 10)), ["0", "5"]);
    // Min above Max: below Min gives Min, otherwise Max, like the classic clamp.
    assert_eq!(fire(&mut vm, &mut h, "LerpClamp", &args(0.0, 0.0, 0.0, 5, 8, 2)), ["0", "8"]);
    assert_eq!(fire(&mut vm, &mut h, "LerpClamp", &args(0.0, 0.0, 0.0, 9, 8, 2)), ["0", "2"]);
}

#[test]
fn vectors_make_break_normal_dot_scale() {
    let (mut vm, mut h) = run();
    // |(3,4)| = 5, (3,4).(1,0) = 3, normal x = 0.6, (3,4) + 2 * (3,4) = (9,12).
    assert_eq!(
        fire(&mut vm, &mut h, "Vec", &[Value::Float(3.0), Value::Float(4.0)]),
        ["5", "3", "60", "12"]
    );
    // The zero vector normalizes to zero instead of failing.
    assert_eq!(
        fire(&mut vm, &mut h, "Vec", &[Value::Float(0.0), Value::Float(0.0)]),
        ["0", "0", "0", "0"]
    );
}

#[test]
fn int_remainder_follows_the_dividend_and_zero_divisor_gives_zero() {
    let (mut vm, mut h) = run();
    for (a, b, want) in [(7, 3, "1"), (-7, 3, "-1"), (7, -3, "1"), (6, 3, "0"), (5, 0, "0")] {
        assert_eq!(fire(&mut vm, &mut h, "Remainder", &[Value::Int(a), Value::Int(b)]), [want]);
    }
}

#[test]
fn square_and_the_other_boolean_gates() {
    let (mut vm, mut h) = run();
    assert_eq!(fire(&mut vm, &mut h, "Squared", &[Value::Float(3.0)]), ["9"]);
    assert_eq!(fire(&mut vm, &mut h, "Squared", &[Value::Float(-4.0)]), ["16"]);
    for (a, b) in [(false, false), (false, true), (true, false), (true, true)] {
        let want =
            [(a != b), !(a && b), !(a || b)].map(|v| if v { "true" } else { "false" }.to_string());
        assert_eq!(
            fire(&mut vm, &mut h, "Logic", &[Value::Bool(a), Value::Bool(b)]),
            want,
            "{a} {b}"
        );
    }
}

#[test]
fn sign_tan_and_bool_int_conversions() {
    let (mut vm, mut h) = run();
    let go = |vm: &mut Interp, h: &mut MapHost, n: i64, x: f64, b: bool| {
        fire(vm, h, "Misc", &[Value::Int(n), Value::Float(x), Value::Bool(b)])
    };
    assert_eq!(go(&mut vm, &mut h, 7, 0.785398163397448, true), ["1", "1", "1", "1", "true"]);
    assert_eq!(go(&mut vm, &mut h, -3, -0.785398163397448, false), ["-1", "-1", "-1", "0", "true"]);
    assert_eq!(go(&mut vm, &mut h, 0, 0.0, false), ["0", "0", "0", "0", "false"]);
}

#[test]
fn nearly_equal_and_integer_width_conversions() {
    let (mut vm, mut h) = run();
    let go = |vm: &mut Interp, h: &mut MapHost, a: f64, b: f64| {
        fire(vm, h, "Near", &[Value::Float(a), Value::Float(b), Value::Int(5)])
    };
    assert_eq!(go(&mut vm, &mut h, 1.0, 1.00005), ["true", "5"]);
    assert_eq!(go(&mut vm, &mut h, 1.0, 1.001), ["false", "5"]);
}

#[test]
fn hypotenuse_and_numeric_width_conversions() {
    let (mut vm, mut h) = run();
    let out = fire(&mut vm, &mut h, "Hyp", &[Value::Float(3.0), Value::Float(4.0), Value::Int(9)]);
    assert_eq!(out, ["5", "3", "9", "9"]);
}

#[test]
fn powers_logs_and_inverse_trig() {
    let (mut vm, mut h) = run();
    let out = fire(&mut vm, &mut h, "Pow", &[Value::Float(8.0), Value::Float(0.5)]);
    let want = [
        8f64.powf(0.5),
        8f64.exp(),
        8f64.ln(),
        8f64.ln() / 0.5f64.ln(),
        0.5f64.atan2(8.0),
        0.5f64.atan2(8.0).to_degrees(),
        0.5f64.asin(),
        0.5f64.acos().to_degrees(),
        8f64.atan().to_degrees(),
    ];
    // The fixture rounds each result before printing it.
    let want: Vec<String> = want.iter().map(|w| (w.round() as i64).to_string()).collect();
    assert_eq!(out, want);
    assert_eq!(out, ["3", "2981", "2", "-3", "0", "4", "1", "60", "83"]);
}

#[test]
fn text_search_ignores_case_unless_asked() {
    let (mut vm, mut h) = run();
    let go = |vm: &mut Interp, h: &mut MapHost, s: &str, t: &str| {
        fire(vm, h, "Search", &[Value::Str(s.into()), Value::Str(t.into())])
    };
    // Contains and StartsWith ignore case; EndsWith is case sensitive here.
    assert_eq!(go(&mut vm, &mut h, "Seal Hop", "HOP"), ["true", "false", "false"]);
    assert_eq!(go(&mut vm, &mut h, "Seal Hop", "seal"), ["true", "true", "false"]);
    assert_eq!(go(&mut vm, &mut h, "Seal Hop", "Hop"), ["true", "false", "true"]);
}
