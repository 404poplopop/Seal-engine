//! Set Actor Hidden In Game writes the sprite's hidden flag.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

const HIDE: &str = include_str!("fixtures/hide.txt");

#[test]
fn hiding_sets_the_sprite_flag_on_self() {
    let imp = import(HIDE, &Options { name: "hide".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    let p = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    let mut vm = Interp::new(p, 5);
    let mut h = MapHost::default().with(5, "sprite", "hidden", Value::Bool(false));
    let flag =
        |h: &MapHost| h.fields.get(&(5, "sprite".to_string(), "hidden".to_string())).cloned();
    vm.fire("Hide", &[], &mut h).unwrap();
    assert_eq!(flag(&h), Some(Value::Bool(true)));
    vm.fire("Show", &[], &mut h).unwrap();
    assert_eq!(flag(&h), Some(Value::Bool(false)));
}
