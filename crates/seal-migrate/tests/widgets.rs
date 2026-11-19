//! On-screen widgets: CreateWidget names the widget, AddToViewport and
//! RemoveFromParent show and hide it, SetText sets a text element.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, widget_name, Options};

#[test]
fn widget_nodes_show_hide_and_set_text() {
    let imp = import(
        include_str!("fixtures/widgets.txt"),
        &Options { name: "hud".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost { alive: [1].into(), ..MapHost::default() };
    vm.fire("start", &[], &mut h).unwrap();
    assert_eq!(h.ui_shown, ["WBP_Hud".to_string()].into());
    vm.fire("Score", &[Value::Str("Score: 3".into())], &mut h).unwrap();
    assert_eq!(h.ui_texts["ScoreText"], "Score: 3");
    h.ui_shown.insert("hud".into());
    vm.fire("Hide", &[], &mut h).unwrap();
    assert_eq!(h.ui_shown, ["WBP_Hud".to_string()].into());
}

#[test]
fn widget_class_references_name_the_widget() {
    assert_eq!(widget_name("WidgetBlueprintGeneratedClass'/Game/UI/WBP_Hud.WBP_Hud_C'"), "WBP_Hud");
    assert_eq!(widget_name("/Game/Menus/Main.Main_C"), "Main");
}
