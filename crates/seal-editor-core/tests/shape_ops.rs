//! Merge, subtract, intersect and offset of filled shapes, and a collider from a sprite.

use seal_editor_core::asset_editor::{ASSET_CLIP_DRAFT, ASSET_DRAFT};
use seal_editor_core::sprite_frames::SpriteFrames;
use seal_editor_core::{shape2d, CommandArgs, EditorSession, Layout};
use std::fs;
use std::path::PathBuf;

fn set(session: &mut EditorSession, id: &str, field: &str, value: &str) {
    session.models.set_field(id, field, value);
    session.mirror_node(id);
}

fn blank() -> (EditorSession, Vec<String>) {
    let mut session = EditorSession::new(Layout::default());
    for e in &mut session.world.entities {
        e.hidden = true;
    }
    let es = &session.world.entities;
    let ids = es
        .iter()
        .filter(|e| !es.iter().any(|c| c.parent.as_deref() == Some(e.id.as_str())))
        .map(|e| e.id.clone())
        .collect();
    (session, ids)
}

fn polygon(session: &mut EditorSession, id: &str, at: [f32; 2], points: &str) {
    set(session, id, "shape2d", "polygon");
    set(session, id, "shape_points", points);
    // Transform lives on the outliner row; writing the entity alone is mirrored back.
    set(session, id, "x", &at[0].to_string());
    set(session, id, "y", &at[1].to_string());
    set(session, id, "z", "0");
    set(session, id, "sx", "1");
    set(session, id, "sy", "1");
    set(session, id, "sz", "1");
    set(session, id, "rz", "0");
    set(session, id, "hidden", "no");
}

#[test]
fn merge_subtract_and_offset_keep_one_filled_shape() {
    let (mut session, ids) = blank();
    let (a, b) = (ids[2].clone(), ids[3].clone());
    polygon(&mut session, &a, [0.0, 0.0], "0 0; 1 0; 1 1; 0 1");
    polygon(&mut session, &b, [0.0, 0.0], "0.5 0; 1.5 0; 1.5 1; 0.5 1");
    session.foundation.selection = vec![a.clone(), b.clone()];
    session.dispatch("shape.merge", &CommandArgs::default()).unwrap();
    let merged = session.world.entity(&a).unwrap().shape2d.points.clone();
    let area = ring_area(&merged);
    assert!((area - 1.5).abs() < 1.0e-2, "{area} points {merged:?}");
    assert!(session.world.entity(&b).unwrap().shape2d.kind.is_empty());

    polygon(&mut session, &b, [0.0, 0.0], "0.5 0; 1.5 0; 1.5 1; 0.5 1");
    session.foundation.selection = vec![a.clone(), b.clone()];
    // Put the first square back before subtract, because merge replaced it.
    set(&mut session, &a, "shape_points", "0 0; 1 0; 1 1; 0 1");
    session.dispatch("shape.subtract", &CommandArgs::default()).unwrap();
    let cut = session.world.entity(&a).unwrap().shape2d.points.clone();
    let area = ring_area(&cut);
    assert!((area - 0.5).abs() < 1.0e-2, "{area} points {cut:?}");

    set(&mut session, &a, "shape2d", "polygon");
    set(&mut session, &a, "shape_points", "0 0; 1 0; 1 1; 0 1");
    session.foundation.selection = vec![a.clone()];
    session.dispatch("shape.offset", &CommandArgs::new(&[("distance", "0.5")])).unwrap();
    let grown = session.world.entity(&a).unwrap().shape2d.points.clone();
    let area = ring_area(&grown);
    assert!((area - 4.0).abs() < 1.0e-2, "{area} points {grown:?}");

    let before = session.world.to_text();
    set(&mut session, &a, "shape_texture", "sprites/hero.png");
    set(&mut session, &a, "shape_uv_offset", "0.25, -0.25");
    set(&mut session, &a, "shape_uv_scale", "2, 2");
    set(&mut session, &a, "shape_uv_rotation", "90");
    let loaded = seal_world::World::from_text(&session.world.to_text()).unwrap();
    assert_eq!(loaded.entity(&a).unwrap().shape2d, session.world.entity(&a).unwrap().shape2d);
    polygon(&mut session, &b, [10.0, 10.0], "0 0; 1 0; 1 1; 0 1");
    session.foundation.selection = vec![a.clone(), b.clone()];
    let err = session.dispatch("shape.merge", &CommandArgs::default()).unwrap_err();
    assert!(err.to_string().contains("do not join"), "{err}");
    assert_eq!(session.world.entity(&a).unwrap().shape2d.points.len(), 4);
    assert_eq!(session.world.entity(&b).unwrap().shape2d.kind, "polygon");

    let plain = seal_world::World::from_text(&before).unwrap();
    assert!(plain.entity(&a).unwrap().shape2d.texture.is_empty());
    assert_eq!(plain.entity(&a).unwrap().shape2d.uv_scale, [1.0, 1.0]);
    let _ = shape2d::default_text("shape_texture");
}

fn ring_area(pts: &[[f32; 2]]) -> f32 {
    let mut sum = 0.0;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        sum += a[0] * b[1] - b[0] * a[1];
    }
    (sum * 0.5).abs()
}

fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) {
    let file = std::io::BufWriter::new(fs::File::create(path).unwrap());
    let mut enc = png::Encoder::new(file, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn project() -> (PathBuf, EditorSession) {
    let dir = std::env::temp_dir().join(format!("seal-sheet-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(project.join("sprites")).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 12);
    fs::write(
        project.join("project.seal"),
        format!(
            "schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
        ),
    )
    .unwrap();
    let (w, h) = (16u32, 8u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 1..4 {
        for x in 1..4 {
            let i = ((y * w + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
        }
    }
    for y in 1..3 {
        for x in 10..13 {
            let i = ((y * w + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&[0, 255, 0, 255]);
        }
    }
    px[((6 * w + 7) * 4) as usize + 3] = 255;
    write_png(&project.join("sprites/sheet.png"), w, h, &px);
    let path = project.to_string_lossy().to_string();
    let mut s = EditorSession::new(Layout::default());
    s.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    (project, s)
}

#[test]
fn island_slice_pivot_border_and_outlines_round_trip() {
    let (project, mut s) = project();
    s.dispatch(
        "asset.frames_new",
        &CommandArgs::new(&[
            ("path", "anims/sheet"),
            ("image", "sprites/sheet.png"),
            ("cols", "1"),
            ("rows", "1"),
        ]),
    )
    .unwrap();
    assert_eq!(s.draft(ASSET_DRAFT).as_deref(), Some("anims/sheet.frames"));
    s.dispatch(
        "asset.frames_slice",
        &CommandArgs::new(&[
            ("clip", "idle"),
            ("image", "sprites/sheet.png"),
            ("mode", "islands"),
            ("min", "2"),
            ("pad", "0"),
        ]),
    )
    .unwrap();
    let lib = SpriteFrames::parse(&fs::read_to_string(project.join("anims/sheet.frames")).unwrap())
        .unwrap();
    assert_eq!(lib.clips[0].frames.len(), 2, "the single speck is dropped");
    s.dispatch(
        "asset.frames_frame",
        &CommandArgs::new(&[
            ("clip", "idle"),
            ("index", "0"),
            ("action", "set"),
            ("pivot", "0.2,0.8"),
            ("border", "1,2,3,4"),
        ]),
    )
    .unwrap();
    s.dispatch(
        "asset.frames_frame",
        &CommandArgs::new(&[("clip", "idle"), ("index", "0"), ("action", "outline")]),
    )
    .unwrap();
    s.dispatch(
        "asset.frames_frame",
        &CommandArgs::new(&[("clip", "idle"), ("index", "0"), ("action", "physics")]),
    )
    .unwrap();
    let lib = SpriteFrames::parse(&fs::read_to_string(project.join("anims/sheet.frames")).unwrap())
        .unwrap();
    let frame = &lib.clips[0].frames[0];
    assert_eq!(frame.pivot, [0.2, 0.8]);
    assert_eq!(frame.border, [1.0, 2.0, 3.0, 4.0]);
    assert!(frame.outline.len() >= 3, "{:?}", frame.outline);
    assert!(frame.physics.len() >= 3, "{:?}", frame.physics);
    let again = SpriteFrames::parse(&lib.to_text()).unwrap();
    assert_eq!(again, lib);
    assert_eq!(s.draft(ASSET_CLIP_DRAFT).as_deref(), Some("idle"));

    let ids = s.world.entities.iter().map(|e| e.id.clone()).collect::<Vec<_>>();
    let id = ids[2].clone();
    s.models.selection = Some(id.clone());
    s.world.entity_mut(&id).unwrap().sprite = "anims/sheet.frames".into();
    s.dispatch("sprite.collider_from_sprite", &CommandArgs::default()).unwrap();
    let body = &s.world.entity(&id).unwrap().phys2d;
    assert_eq!(body.shape, "polygon");
    assert!(body.points.len() >= 3, "{:?}", body.points);
}
