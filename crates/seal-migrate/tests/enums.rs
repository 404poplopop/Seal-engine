//! Enum values travel as enumerator names.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

#[test]
fn switch_on_enum_and_enum_equality_use_enumerator_names() {
    let imp = import(
        include_str!("fixtures/enums.txt"),
        &Options { name: "enums".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost::default();
    for e in ["NewEnumerator0", "NewEnumerator1", "NewEnumerator2", "NewEnumerator9"] {
        vm.fire("Feel", &[Value::Str(e.into())], &mut h).unwrap();
    }
    assert_eq!(h.logs, ["calm", "glad", "true"]);
}
