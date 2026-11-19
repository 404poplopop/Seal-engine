//! 3D vectors: math library nodes and actor location.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value, V3};
use seal_migrate::{import, Options};

const VEC3: &str = include_str!("fixtures/vec3.txt");
const BAD: &str = include_str!("fixtures/vec3_bad.txt");
const ME: u64 = 1;

fn run() -> (Interp, MapHost) {
    let imp = import(VEC3, &Options { name: "vec3".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let program = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    let start = Value::Vec3(V3 { x: 0.0, y: 0.0, z: 0.0 });
    (Interp::new(program, ME), MapHost::default().with(ME, "transform", "translation", start))
}

fn fire(vm: &mut Interp, host: &mut MapHost, event: &str, args: &[Value]) -> Vec<String> {
    let before = host.logs.len();
    vm.fire(event, args, host).unwrap();
    host.logs[before..].to_vec()
}

#[test]
fn vector_math_matches_the_classic_results() {
    let (mut vm, mut h) = run();
    let args = [Value::Float(1.0), Value::Float(2.0), Value::Float(2.0)];
    // Length, cross with up (Y), dot with (1,2,3), v + 2v (Z), -v (Y), normal X * 100.
    assert_eq!(fire(&mut vm, &mut h, "Vec3", &args), ["3", "-1", "11", "6", "-2", "33"]);
}

#[test]
fn set_offset_and_read_the_actor_location() {
    let (mut vm, mut h) = run();
    assert_eq!(fire(&mut vm, &mut h, "Move", &[]), ["12", "20", "26"]);
    assert_eq!(
        h.fields.get(&(ME, "transform".to_string(), "translation".to_string())),
        Some(&Value::Vec3(V3 { x: 11.5, y: 20.0, z: 26.0 }))
    );
}

#[test]
fn sweeping_moves_are_reported() {
    let imp =
        import(BAD, &Options { name: "bad".into(), ..Options::default() }).expect("fixture parses");
    let text = imp.report.to_text();
    assert_eq!(imp.report.unsupported(), 1, "{text}");
    assert!(text.contains("sweeping moves are not mapped"), "{text}");
}
