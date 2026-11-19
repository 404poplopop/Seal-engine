//! Prefab tile brush strokes and Inspector tile data on instances.

use seal_editor_core::tiles::{
    decode_tile_data, decode_tile_set, is_painted_tile, pack_tile_overrides, Grid, PREFAB_BRUSH_PREFIX,
    RuleSet,
};
use seal_editor_core::{CommandArgs, EditorSession, Layout};

fn project(tag: &str) -> (EditorSession, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("seal-tile-prefab-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = seal_editor_core::create_project(&dir, "TilePrefab").unwrap();
    let mut session = EditorSession::new(Layout::default());
    session.project = Some(project);
    (session, dir)
}

#[test]
fn prefab_brush_places_on_cells_and_undoes_in_one_step() {
    let (mut s, _dir) = session_setup("brush");
    s.dispatch("scene.add", &CommandArgs::new(&[("kind", "empty")])).unwrap();
    s.dispatch("scene.make_prefab", &CommandArgs::new(&[("name", "Crate")])).unwrap();
    let brush = format!("{PREFAB_BRUSH_PREFIX}prefabs/Crate.prefab");
    let before = s.world.entities.len();
    s.dispatch(
        "tiles.paint",
        &CommandArgs::new(&[("brush", brush.as_str()), ("cells", "0,0;1,0;2,0")]),
    )
    .unwrap();
    assert_eq!(s.world.entities.len(), before + 3);
    for i in 0..3 {
        let (cx, cy) = Grid::Square.center((i, 0));
        assert!(
            s.world.entities.iter().any(|e| {
                e.source_kind == "prefab"
                    && (e.translation[0] - cx).abs() < 0.02
                    && (e.translation[1] - cy).abs() < 0.02
            }),
            "cell {i},0"
        );
    }
    s.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert_eq!(s.world.entities.len(), before);
}

fn session_setup(tag: &str) -> (EditorSession, std::path::PathBuf) {
    project(tag)
}

#[test]
fn tile_data_edits_round_trip_and_hides_for_non_tiles() {
    const TILES: &str = "seal tiles 1
default sprites/floor.png
tile sprites/floor.png
  data seed=1
  occluder yes
";
    let (mut s, dir) = session_setup("data");
    std::fs::create_dir_all(dir.join("tiles")).unwrap();
    std::fs::write(dir.join("tiles/floor.tiles"), TILES).unwrap();
    s.dispatch(
        "tiles.paint",
        &CommandArgs::new(&[
            ("brush", "tiles/floor.tiles"),
            ("cells", "0,0"),
            ("solid", "true"),
        ]),
    )
    .unwrap();
    let tile = s
        .world
        .entities
        .iter()
        .find(|e| e.sprite == "sprites/floor.png")
        .unwrap();
    assert!(is_painted_tile(tile));
    assert!(decode_tile_set(&tile.overrides).as_deref() == Some("tiles/floor.tiles"));
    let id = tile.id.clone();
    s.dispatch(
        "tiles.data_set",
        &CommandArgs::new(&[("id", id.as_str()), ("key", "type"), ("value", "grass")]),
    )
    .unwrap();
    let entity = s.world.entity(&id).unwrap();
    let data = decode_tile_data(&entity.overrides);
    assert!(data.iter().any(|(k, v)| k == "type" && v == "grass"));
    assert!(data.iter().any(|(k, _)| k == "seed"));
    s.dispatch(
        "tiles.data_remove",
        &CommandArgs::new(&[("id", id.as_str()), ("key", "seed")]),
    )
    .unwrap();
    let entity = s.world.entity(&id).unwrap();
    assert!(!decode_tile_data(&entity.overrides).iter().any(|(k, _)| k == "seed"));
    let packed = pack_tile_overrides(&[("a".into(), "b".into())], Some("tiles/x.tiles"));
    assert_eq!(decode_tile_data(&packed), vec![("a".to_string(), "b".to_string())]);
    assert_eq!(decode_tile_set(&packed).as_deref(), Some("tiles/x.tiles"));
    s.dispatch("scene.add", &CommandArgs::new(&[("kind", "empty")])).unwrap();
    let other = s.world.entities.last().unwrap();
    assert!(!is_painted_tile(other));
    let rules = RuleSet::parse(TILES).unwrap();
    assert!(rules.def_for("sprites/floor.png").unwrap().occluder);
}

#[test]
fn damaged_tiledata_still_parses_pairs() {
    let packed = "tiledata:badpair,type=ok";
    assert_eq!(
        decode_tile_data(packed),
        vec![("type".to_string(), "ok".to_string())]
    );
}