//! Forward, right and up vectors follow the classic rotation matrix, both
//! from a rotator value and from the object's own rotation.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value, V3};
use seal_migrate::{import, Options};

#[test]
fn rotation_axes_match_the_classic_matrix() {
    let imp = import(
        include_str!("fixtures/axes.txt"),
        &Options { name: "axes".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost { alive: [1].into(), ..MapHost::default() };
    h.fields.insert(
        (1, "transform".into(), "rotation".into()),
        Value::Vec3(V3 { x: 30.0, y: 0.0, z: 0.0 }),
    );
    vm.fire("Axes", &[Value::Vec3(V3 { x: 0.0, y: 90.0, z: 0.0 })], &mut h).unwrap();
    vm.fire("Mine", &[], &mut h).unwrap();
    let clean: Vec<String> = h.logs.iter().map(|s| s.replace("-0.000", "0.000")).collect();
    assert_eq!(
        clean,
        [
            "X=0.000 Y=1.000 Z=0.000",
            "X=-1.000 Y=0.000 Z=0.000",
            "X=0.000 Y=0.000 Z=1.000",
            "X=0.866 Y=0.000 Z=0.500",
            "X=0.000 Y=1.000 Z=0.000",
            "X=-0.500 Y=0.000 Z=0.866",
        ]
    );
}
