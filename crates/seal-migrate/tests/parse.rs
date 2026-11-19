//! String To Int and String To Float read the number a text starts with.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

const PARSE: &str = include_str!("fixtures/parse.txt");

fn program() -> Program {
    let imp = import(PARSE, &Options { name: "parse".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"))
}

#[test]
fn leading_numbers_are_read_and_the_rest_ignored() {
    let mut vm = Interp::new(program(), 1);
    let mut h = MapHost::default();
    for (text, whole, real) in
        [("42", 42, 42.0), ("  -3.5kg", -3, -3.5), ("1e2", 1, 100.0), ("none", 0, 0.0)]
    {
        vm.fire("Read", &[Value::Str(text.into())], &mut h).unwrap();
        assert_eq!(vm.var("Whole"), Some(&Value::Int(whole)), "{text}");
        assert_eq!(vm.var("Real"), Some(&Value::Float(real)), "{text}");
    }
}
