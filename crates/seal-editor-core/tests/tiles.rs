use seal_editor_core::tiles::Grid;
use seal_editor_core::{CommandArgs, EditorSession, Layout};

fn session(tag: &str) -> (EditorSession, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("seal-tiles-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = seal_editor_core::create_project(&dir, "TileTest").unwrap();
    std::fs::create_dir_all(dir.join("tiles")).unwrap();
    std::fs::write(
        dir.join("tiles/ground.tiles"),
        "seal tiles 1\n# dirt wherever another ground tile sits on top\nrule 1*** sprites/dirt.png\ndefault sprites/grass.png\n",
    )
    .unwrap();
    let mut session = EditorSession::new(Layout::default());
    session.project = Some(project);
    (session, dir)
}

fn tiles_on(session: &EditorSession, layer: i32) -> Vec<(f32, f32, String, String)> {
    let mut out: Vec<_> = session
        .world
        .entities
        .iter()
        .filter(|e| !e.sprite.is_empty() && e.layer == layer)
        .map(|e| (e.translation[0], e.translation[1], e.sprite.clone(), e.tag.clone()))
        .collect();
    out.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.total_cmp(&b.0)));
    out
}

fn paint(session: &mut EditorSession, pairs: &[(&str, &str)]) {
    session.dispatch("tiles.paint", &CommandArgs::new(pairs)).unwrap();
}

#[test]
fn rule_tiles_pick_grass_on_top_and_dirt_below_and_update_neighbours() {
    let (mut s, _dir) = session("rules");
    let before = s.world.entities.len();
    paint(&mut s, &[("brush", "tiles/ground.tiles"), ("cells", "0,0;1,0"), ("solid", "true")]);
    let row = tiles_on(&s, 0);
    assert_eq!(row.len(), 2);
    assert!(row.iter().all(|t| t.2 == "sprites/grass.png" && t.3 == "ground"), "{row:?}");
    // Painting above turns the tile underneath into dirt.
    paint(&mut s, &[("brush", "tiles/ground.tiles"), ("cells", "0,1"), ("solid", "true")]);
    let all = tiles_on(&s, 0);
    let under = all.iter().find(|t| (t.0, t.1) == (0.5, 0.5)).unwrap();
    let top = all.iter().find(|t| (t.0, t.1) == (0.5, 1.5)).unwrap();
    assert_eq!((under.2.as_str(), top.2.as_str()), ("sprites/dirt.png", "sprites/grass.png"));
    // Erasing the top tile turns the one below back into grass.
    s.dispatch(
        "tiles.erase",
        &CommandArgs::new(&[("cells", "0,1"), ("brush", "tiles/ground.tiles")]),
    )
    .unwrap();
    let under = tiles_on(&s, 0).into_iter().find(|t| (t.0, t.1) == (0.5, 0.5)).unwrap();
    assert_eq!(under.2, "sprites/grass.png");
    // Each stroke is one undo step.
    for _ in 0..3 {
        s.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    }
    assert_eq!(s.world.entities.len(), before);
}

#[test]
fn layers_hold_separate_tiles_and_repainting_a_cell_is_a_no_op() {
    let (mut s, _dir) = session("layers");
    paint(&mut s, &[("brush", "sprites/grass.png"), ("cells", "2,0")]);
    paint(&mut s, &[("brush", "sprites/flower.png"), ("cells", "2,0"), ("layer", "1")]);
    paint(&mut s, &[("brush", "sprites/flower.png"), ("cells", "2,0"), ("layer", "1")]);
    assert_eq!(tiles_on(&s, 0).len(), 1);
    assert_eq!(tiles_on(&s, 1).len(), 1);
    assert_eq!(tiles_on(&s, 1)[0].3, "", "tiles are not solid unless asked");
    // A different picture on the same layer replaces the tile there.
    paint(&mut s, &[("brush", "sprites/rock.png"), ("cells", "2,0")]);
    let base = tiles_on(&s, 0);
    assert_eq!(base.len(), 1);
    assert_eq!(base[0].2, "sprites/rock.png");
}

#[test]
fn iso_and_hex_tiles_land_on_their_grid_and_take_its_shape() {
    let (mut s, _dir) = session("grids");
    paint(
        &mut s,
        &[("brush", "sprites/iso.png"), ("cells", "1,0;0,1"), ("grid", "iso"), ("layer", "2")],
    );
    paint(
        &mut s,
        &[("brush", "sprites/hex.png"), ("cells", "0,0;1,0;0,1"), ("grid", "hex"), ("layer", "3")],
    );
    for (layer, grid) in [(2, Grid::Iso), (3, Grid::Hex)] {
        let (w, h) = grid.cell_size();
        for e in s.world.entities.iter().filter(|e| e.layer == layer) {
            let cell = grid.cell_at(e.translation[0], e.translation[1]);
            let c = grid.center(cell);
            assert!((c.0 - e.translation[0]).abs() < 1e-4 && (c.1 - e.translation[1]).abs() < 1e-4);
            assert_eq!((e.scale[0], e.scale[1]), (w, h));
        }
    }
    assert_eq!(tiles_on(&s, 2).len(), 2);
    assert_eq!(tiles_on(&s, 3).len(), 3);
}

#[test]
fn a_bad_rule_file_is_refused_without_changing_the_scene() {
    let (mut s, dir) = session("bad");
    std::fs::write(dir.join("tiles/broken.tiles"), "rule 1x sprites/a.png\n").unwrap();
    let before = s.world.to_text();
    let err = s.dispatch(
        "tiles.paint",
        &CommandArgs::new(&[("brush", "tiles/broken.tiles"), ("cells", "0,0")]),
    );
    assert!(err.is_err());
    assert_eq!(s.world.to_text(), before);
}
