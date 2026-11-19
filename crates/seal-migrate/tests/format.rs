//! Format Text becomes literal pieces and arguments joined in order.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options, Status};

fn program(src: &str) -> Program {
    let imp = import(src, &Options { name: "format".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"))
}

#[test]
fn arguments_fill_the_template_in_order() {
    let mut vm = Interp::new(program(include_str!("fixtures/format.txt")), 1);
    let mut h = MapHost::default();
    let args =
        [Value::Str("Ann".into()), Value::Int(12345), Value::Float(0.6666), Value::Bool(true)];
    vm.fire("Show", &args, &mut h).unwrap();
    vm.fire("Plain", &[], &mut h).unwrap();
    assert_eq!(
        h.logs,
        ["Ann has 12,345 coins (0.667), rich: true {x} {Missing} - Ann!", "just text, ."]
    );
}

#[test]
fn argument_modifiers_are_reported() {
    let imp = import(include_str!("fixtures/format_plural.txt"), &Options::default())
        .expect("fixture parses");
    let r =
        imp.report.nodes.iter().find(|n| n.class.ends_with("K2Node_FormatText")).expect("reported");
    assert!(
        matches!(&r.status, Status::Unsupported(why) if why.contains("modifier")),
        "{:?}",
        r.status
    );
}

#[test]
fn numbers_and_bools_become_display_text() {
    let mut vm = Interp::new(program(include_str!("fixtures/numtext.txt")), 1);
    let mut h = MapHost::default();
    vm.fire("Show", &[Value::Int(-9876543), Value::Float(1234.56789), Value::Bool(false)], &mut h)
        .unwrap();
    assert_eq!(h.logs, ["-9,876,543/1,234.568/false"]);
}

#[test]
fn a_changed_number_look_is_reported() {
    let imp = import(include_str!("fixtures/numtext_padded.txt"), &Options::default())
        .expect("fixture parses");
    let r = &imp.report.nodes[0];
    assert!(
        matches!(&r.status, Status::Unsupported(why) if why.contains("MinimumIntegralDigits")),
        "{:?}",
        r.status
    );
}
