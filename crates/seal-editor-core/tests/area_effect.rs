//! Create-menu areas, their inspector fields, and scene save and load.

use seal_editor_core::{CommandArgs, EditorSession, Layout};

fn set(session: &mut EditorSession, id: &str, field: &str, value: &str) {
    session.models.set_field(id, field, value);
    session.mirror_node(id);
}

#[test]
fn create_menu_areas_edit_save_and_refuse_bad_text() {
    let mut session = EditorSession::new(Layout::default());
    session.dispatch("scene.add", &CommandArgs::new(&[("kind", "wind_area")])).unwrap();
    let id = session.models.selection.clone().unwrap();
    let area = session.world.entity(&id).unwrap();
    assert_eq!(area.name, "Wind Area");
    assert_eq!(area.area.mode, "directional");
    assert_eq!(area.area.strength, 12.0);
    assert_eq!(area.phys2d.body, "fixed");
    assert!(area.phys2d.size[0] > 1.0);
    let shown = seal_editor_core::area_effect::fields(&area.area, &area.surface);
    assert!(shown.iter().any(|(k, v)| k == "area_mode" && v == "directional"));
    assert!(shown.iter().any(|(k, v)| k == "area_direction" && v == "1, 0"));
    assert!(shown.iter().any(|(k, _)| k == "surface_speed"));
    assert!(!seal_editor_core::area_effect::gizmos(area).is_empty());

    set(&mut session, &id, "area_strength", "7");
    set(&mut session, &id, "area_variation", "0.25");
    set(&mut session, &id, "surface_speed", "3");
    set(&mut session, &id, "area_mode", "sideways");
    set(&mut session, &id, "area_strength", "nope");
    let area = session.world.entity(&id).unwrap();
    assert_eq!(area.area.mode, "directional");
    assert_eq!(area.area.strength, 7.0);
    assert_eq!(area.area.variation, 0.25);
    assert_eq!(area.surface.speed, 3.0);

    let loaded = seal_world::World::from_text(&session.world.to_text()).unwrap();
    let back = loaded.entity(&id).unwrap();
    assert_eq!(back.area.strength, 7.0);
    assert_eq!(back.area.variation, 0.25);
    assert_eq!(back.surface.speed, 3.0);
    assert_eq!(back.area.mode, "directional");

    set(&mut session, &id, "area_mode", "none");
    let loaded = seal_world::World::from_text(&session.world.to_text()).unwrap();
    assert!(loaded.entity(&id).unwrap().area.mode.is_empty());
    assert!(!loaded.to_text().contains("area_effect"));

    for kind in ["water_area", "gravity_area", "attractor"] {
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", kind)])).unwrap();
        let id = session.models.selection.clone().unwrap();
        let e = session.world.entity(&id).unwrap();
        assert!(!e.area.mode.is_empty(), "{kind}");
        assert!(e.shape2d.points.len() >= 3, "{kind}");
    }
}
