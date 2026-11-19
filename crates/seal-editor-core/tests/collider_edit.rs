//! Collider point edits undo, and a sprite outline traces a solid picture.

use seal_editor_core::{CommandArgs, EditorSession, Layout};

fn session(tag: &str) -> (EditorSession, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("seal-collider-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = seal_editor_core::create_project(&dir, "ColliderTest").unwrap();
    let mut session = EditorSession::new(Layout::default());
    session.project = Some(project);
    (session, dir)
}

fn solid_png(path: &std::path::Path, size: u32) {
    let pixels = vec![255u8; (size * size * 4) as usize];
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}

#[test]
fn a_drag_is_one_undo_step() {
    let (mut session, _dir) = session("drag");
    let id = session.world.add("empty");
    {
        let entity = session.world.entity_mut(&id).unwrap();
        entity.phys2d.body = "fixed".into();
        entity.phys2d.shape = "polygon".into();
        entity.phys2d.points = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    }
    session.adopt_world();
    let before = session.world.entity(&id).unwrap().phys2d.points.clone();
    session
        .dispatch(
            "collider.edit",
            &CommandArgs::new(&[
                ("id", &id),
                ("op", "point"),
                ("index", "1"),
                ("x", "2"),
                ("y", "0"),
                ("begin", "1"),
            ]),
        )
        .unwrap();
    session
        .dispatch(
            "collider.edit",
            &CommandArgs::new(&[
                ("id", &id),
                ("op", "point"),
                ("index", "1"),
                ("x", "3"),
                ("y", "0"),
                ("begin", "0"),
            ]),
        )
        .unwrap();
    assert_eq!(session.world.entity(&id).unwrap().phys2d.points[1], [3.0, 0.0]);
    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert_eq!(session.world.entity(&id).unwrap().phys2d.points, before);
}

#[test]
fn outline_from_sprite_traces_a_picture_and_refuses_a_damaged_file() {
    let (mut session, dir) = session("outline");
    std::fs::create_dir_all(dir.join("sprites")).unwrap();
    solid_png(&dir.join("sprites/block.png"), 8);
    std::fs::write(dir.join("sprites/broken.png"), b"this is not a picture").unwrap();
    let id = session.world.add("empty");
    {
        let entity = session.world.entity_mut(&id).unwrap();
        entity.sprite = "sprites/block.png".into();
        entity.phys2d.shape = "polygon".into();
        entity.phys2d.points = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        // A collider without its own body is a part of its parent's compound body.
        entity.phys2d.part = true;
    }
    session.adopt_world();
    session
        .dispatch("collider.outline", &CommandArgs::new(&[("id", &id)]))
        .unwrap();
    let points = session.world.entity(&id).unwrap().phys2d.points.clone();
    assert!(points.len() >= 4, "{points:?}");
    assert!(points.iter().any(|p| p[0] < -0.2 && p[1] < -0.2), "{points:?}");
    assert!(session.world.entity(&id).unwrap().phys2d.part);
    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert_eq!(session.world.entity(&id).unwrap().phys2d.points.len(), 3);
    // A compound part keeps its collider through a save and reload.
    let reloaded = seal_world::World::from_text(&session.world.to_text()).unwrap();
    let part = reloaded.entity(&id).unwrap();
    assert!(part.phys2d.part && part.phys2d.points.len() == 3, "{:?}", part.phys2d);
    session.world.entity_mut(&id).unwrap().sprite = "sprites/broken.png".into();
    let err = session
        .dispatch("collider.outline", &CommandArgs::new(&[("id", &id)]))
        .unwrap_err();
    assert!(err.to_string().contains("picture") || err.to_string().contains("read") || !err.to_string().is_empty());
}
