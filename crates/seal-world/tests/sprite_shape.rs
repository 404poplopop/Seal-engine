//! Spline terrain: the curve, the fill, the edge bands, and the collider.

use seal_world::{sprite_shape, World};

fn shaped(points: &str, closed: bool) -> sprite_shape::Shape {
    let mut world = World::starter();
    let id = world.add("sprite_shape");
    let entity = world.entity_mut(&id).unwrap();
    assert!(sprite_shape::apply(entity, "ss_points", points));
    assert!(sprite_shape::apply(entity, "ss_closed", if closed { "yes" } else { "no" }));
    entity.sprite_shape.clone()
}

#[test]
fn curve_hits_the_ends_and_the_known_cubic_midpoint() {
    let shape = shaped("0 0 smooth 0 1; 1 0 corner", false);
    let start = sprite_shape::curve_at(&shape, 0, 0.0).unwrap();
    let end = sprite_shape::curve_at(&shape, 0, 1.0).unwrap();
    let mid = sprite_shape::curve_at(&shape, 0, 0.5).unwrap();
    assert!((start[0]).abs() < 1e-4 && start[1].abs() < 1e-4, "{start:?}");
    assert!((end[0] - 1.0).abs() < 1e-4 && end[1].abs() < 1e-4, "{end:?}");
    assert!((mid[0] - 0.5).abs() < 1e-4 && (mid[1] - 0.375).abs() < 1e-4, "{mid:?}");
}

#[test]
fn tessellation_stays_within_tolerance_and_a_closed_loop_joins() {
    let open = shaped("0 0 corner; 1 0 corner; 1 1 corner", false);
    let open_line = sprite_shape::tessellate(&open);
    assert_eq!(open_line.points.len(), 3, "a straight open spline keeps its corners");

    let closed = shaped("0 0 corner; 2 0 corner; 2 1 corner; 0 1 corner", true);
    let looped = sprite_shape::tessellate(&closed);
    assert_eq!(looped.points.len(), 5, "a closed loop repeats the first point");
    let first = looped.points[0];
    let last = *looped.points.last().unwrap();
    assert!((first[0] - last[0]).abs() < 1e-4 && (first[1] - last[1]).abs() < 1e-4);

    let curved = shaped("0 0 smooth 0 2; 3 0 corner", false);
    let samples = sprite_shape::tessellate(&curved);
    assert!(samples.points.len() > 2, "a bend adds samples");
    assert!(samples.points.len() < 300, "depth stays bounded: {}", samples.points.len());
}

#[test]
fn angle_bands_are_top_side_and_bottom() {
    let top = [-40.0, 40.0];
    let side = [40.0, 140.0];
    let bottom = [140.0, -140.0];
    assert_eq!(sprite_shape::classify(0.0, top, side, bottom), sprite_shape::Band::Top);
    assert_eq!(sprite_shape::classify(90.0, top, side, bottom), sprite_shape::Band::Side);
    assert_eq!(sprite_shape::classify(180.0, top, side, bottom), sprite_shape::Band::Bottom);
    assert_eq!(sprite_shape::classify(-90.0, top, side, bottom), sprite_shape::Band::Side);
}

#[test]
fn fill_area_matches_the_polygon_for_convex_and_concave() {
    let square = [[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [0.0, 1.0]];
    let ell = [[0.0, 0.0], [3.0, 0.0], [3.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]];
    for pts in [square.as_slice(), ell.as_slice()] {
        let poly = sprite_shape::polygon_area(pts);
        let tris = sprite_shape::triangulated_area(pts);
        assert!((poly - tris).abs() < 1e-3, "split area {tris} vs polygon {poly}");
        let filled = sprite_shape::fill_triangles(pts, [0.5, 0.5]);
        let area = sprite_shape::fill_area(&filled);
        assert!((area - poly).abs() < 1e-2, "tiled fill {area} vs polygon {poly}");
    }
}

#[test]
fn collider_follows_the_points_and_round_trips() {
    let mut world = World::starter();
    let id = world.add("sprite_shape");
    {
        let entity = world.entity_mut(&id).unwrap();
        sprite_shape::apply(entity, "ss_points", "0 0 corner; 2 0 corner; 2 1 corner; 0 1 corner");
        sprite_shape::apply(entity, "ss_closed", "yes");
        sprite_shape::apply(entity, "ss_collider", "yes");
        let (polygon, pts) = sprite_shape::collider_outline(&entity.sprite_shape);
        assert!(polygon);
        assert_eq!(pts.len(), 4);
        assert_eq!(entity.phys2d.shape, "polygon");
        assert_eq!(entity.phys2d.points.len(), 4);
    }
    let text = world.to_text();
    let loaded = World::from_text(&text).unwrap();
    let back = loaded.entity(&id).unwrap();
    assert!(back.sprite_shape.on);
    assert_eq!(back.sprite_shape.points.0.len(), 4);
    assert_eq!(back.phys2d.points.len(), 4);

    let open = shaped("0 0 corner; 3 0 corner; 3 1 corner", false);
    let (polygon, pts) = sprite_shape::collider_outline(&open);
    assert!(!polygon);
    assert_eq!(pts.len(), 3);
}

#[test]
fn an_old_scene_loads_without_a_sprite_shape() {
    let demo = World::demo();
    let text = demo.to_text();
    assert!(!text.contains("sprite_shape"), "a default component is not written");
    let loaded = World::from_text(&text).unwrap();
    assert!(loaded.entities.iter().all(|e| !e.sprite_shape.on));
    assert_eq!(loaded.entities.len(), demo.entities.len());
}

#[test]
fn a_damaged_point_list_refuses_to_load() {
    let mut world = World::starter();
    let id = world.add("sprite_shape");
    let text = world.to_text();
    assert!(text.contains("sprite_shape"), "{id} should be saved");
    let broken = text.replacen("corner", "nope", 1);
    match World::from_text(&broken) {
        Err(err) => assert!(err.contains("damaged") || err.contains("sprite shape"), "{err}"),
        Ok(_) => panic!("a damaged point list loaded"),
    }
}

#[test]
fn a_platformer_stands_on_the_collider() {
    let mut world = World::starter();
    let ground = world.add("sprite_shape");
    {
        let entity = world.entity_mut(&ground).unwrap();
        entity.translation = [80.0, 0.0, 0.0];
        sprite_shape::apply(
            entity,
            "ss_points",
            "-3 0 corner; 3 0 corner; 3 -1 corner; -3 -1 corner",
        );
        sprite_shape::apply(entity, "ss_closed", "yes");
        sprite_shape::apply(entity, "ss_collider", "yes");
    }
    {
        let entity = world.entity_mut("hero").unwrap();
        entity.sprite = "hero.png".into();
        entity.translation = [80.0, 3.0, 0.0];
        entity.hidden = false;
    }
    world.play();
    for _ in 0..240 {
        world.step(1.0 / 60.0);
    }
    let y = world.entity("hero").unwrap().translation[1];
    assert!(y.is_finite() && (0.3..1.2).contains(&y), "rests on the slab, y={y}");
}
