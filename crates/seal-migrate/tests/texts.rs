//! Text helpers: empty check, compares that keep or ignore case, case
//! changes and trimming, for texts and strings.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

#[test]
fn text_helpers_follow_the_classic_rules() {
    let imp = import(
        include_str!("fixtures/texts.txt"),
        &Options { name: "texts".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost::default();
    vm.fire("T", &[Value::Str("  Hi \t".into())], &mut h).unwrap();
    vm.fire("T", &[Value::Str("Hi".into())], &mut h).unwrap();
    vm.fire("T", &[Value::Str(String::new())], &mut h).unwrap();
    assert_eq!(
        h.logs,
        [
            "false", "false", "true", "false", "  HI \t", "  hi \t", "Hi \t", "  Hi", "Hi",
            "Hi \t", "  Hi", "false", "true", "false", "true", "HI", "hi", "Hi", "Hi", "Hi", "Hi",
            "Hi", "true", "false", "true", "false", "", "", "", "", "", "", "",
        ]
    );
}
