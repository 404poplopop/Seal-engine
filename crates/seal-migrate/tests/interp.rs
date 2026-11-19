//! FInterpTo, VInterpTo, MapRangeClamped and GetDistanceTo keep the classic
//! edge cases: zero speed, tiny gaps, flat ranges and a missing actor.

use seal_graph::rt::NO_ENTITY;
use seal_graph::{GraphInstance, Interp, MapHost, Program, Value, V3};
use seal_migrate::{import, Options};

#[test]
fn interpolation_and_range_mapping_match_the_classic_rules() {
    let imp = import(
        include_str!("fixtures/interp.txt"),
        &Options { name: "interp".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost { alive: [1, 2].into(), ..MapHost::default() };
    h.fields.insert(
        (1, "transform".into(), "translation".into()),
        Value::Vec3(V3 { x: 0.0, y: 0.0, z: 0.0 }),
    );
    h.fields.insert(
        (2, "transform".into(), "translation".into()),
        Value::Vec3(V3 { x: 3.0, y: 4.0, z: 0.0 }),
    );
    let f = Value::Float;
    for args in [
        [0.0, 10.0, 0.1, 5.0],
        [0.0, 10.0, 0.1, 0.0],
        [3.0, 3.00001, 1.0, 1.0],
        [0.0, 10.0, 1.0, 5.0],
        [0.0, 10.0, 0.1, -1.0],
    ] {
        vm.fire("Interp", &args.map(f), &mut h).unwrap();
    }
    for args in
        [[5.0, 0.0, 10.0], [-5.0, 0.0, 10.0], [20.0, 0.0, 10.0], [3.0, 4.0, 4.0], [4.0, 4.0, 4.0]]
    {
        vm.fire("Map", &args.map(f), &mut h).unwrap();
    }
    let v = |x| Value::Vec3(V3 { x, y: 0.0, z: 0.0 });
    vm.fire("VI", &[v(0.0), v(10.0)], &mut h).unwrap();
    vm.fire("Dist", &[Value::Entity(2)], &mut h).unwrap();
    vm.fire("Dist", &[Value::Entity(NO_ENTITY)], &mut h).unwrap();
    assert_eq!(
        h.logs,
        [
            "5.0",
            "10.0",
            "3.00001",
            "10.0",
            "10.0",
            "15.0",
            "10.0",
            "20.0",
            "10.0",
            "20.0",
            "X=5.000 Y=0.000 Z=0.000",
            "5.0",
            "0.0"
        ]
    );
}
