use seal_world::World;

#[test]
fn prefab_round_trip_keeps_children_and_fields() {
    let mut w = World::starter();
    let a = w.add("cube");
    let b = w.add("sphere");
    w.entity_mut(&b).unwrap().parent = Some(a.clone());
    w.entity_mut(&b).unwrap().translation = [1.0, 2.0, 3.0];
    w.entity_mut(&a).unwrap().sprite = "sprites/c.png".into();
    w.entity_mut(&a).unwrap().layer = 3;
    w.entity_mut(&a).unwrap().frames = 4;
    w.entity_mut(&a).unwrap().color = [0.25, 0.5, 0.75];
    let text = w.prefab_text(&a).unwrap();
    assert!(w.prefab_text("root").is_none());
    let before = w.entities.len();
    let new_root = w.add_prefab_text(&text, None).unwrap();
    assert_eq!(w.entities.len(), before + 2);
    assert_ne!(new_root, a);
    let r = w.entity(&new_root).unwrap();
    assert_eq!(r.parent.as_deref(), Some("root"));
    assert_eq!((r.sprite.as_str(), r.layer, r.frames), ("sprites/c.png", 3, 4));
    assert_eq!(r.color, [0.25, 0.5, 0.75]);
    assert_ne!(r.name, w.entity(&a).unwrap().name);
    let kid = w.entities.iter().find(|e| e.parent.as_deref() == Some(new_root.as_str())).unwrap();
    assert_eq!(kid.translation, [1.0, 2.0, 3.0]);
    assert_ne!(kid.id, b);
    assert!(w.add_prefab_text("garbage", None).is_err());
}
