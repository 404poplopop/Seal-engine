//! Tile brush tools: shapes, fill, random, patterns, metadata, undo per stroke.

use seal_editor_core::tile_tools::{flood_fill, rectangle_cells, weighted_pick, TilePattern};
use seal_editor_core::tiles::{encode_tile_data, Grid, RuleSet, TileNavigation};
use seal_editor_core::{CommandArgs, EditorSession, Layout};

fn session(tag: &str) -> (EditorSession, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("seal-tiletools-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = seal_editor_core::create_project(&dir, "TileTools").unwrap();
    std::fs::create_dir_all(dir.join("tilepatterns")).unwrap();
    let mut session = EditorSession::new(Layout::default());
    session.project = Some(project);
    (session, dir)
}

#[test]
fn rectangle_and_line_cells_cover_iso_and_hex_grids() {
    let rect = rectangle_cells((0, 0), (2, 1));
    assert_eq!(rect.len(), 6);
    for grid in [Grid::Iso, Grid::Hex] {
        let line = grid.line((0, 0), (4, 2));
        assert!(line.len() >= 3);
        assert_eq!(line[0], (0, 0));
    }
}

#[test]
fn random_brush_uses_the_seed_and_pattern_save_round_trips() {
    let w = "sprites/a.png:1;sprites/b.png:1";
    assert_ne!(weighted_pick(0, w), weighted_pick(1, w));
    let (mut s, dir) = session("pattern");
    s.dispatch(
        "tiles.paint",
        &CommandArgs::new(&[("brush", "sprites/a.png"), ("cells", "0,0;1,0")]),
    )
    .unwrap();
    s.dispatch(
        "tiles.pattern_save",
        &CommandArgs::new(&[
            ("name", "test"),
            ("cells", "0,0;1,0"),
            ("layer", "0"),
        ]),
    )
    .unwrap();
    assert!(dir.join("tilepatterns/test.pattern").is_file());
    let text = std::fs::read_to_string(dir.join("tilepatterns/test.pattern")).unwrap();
    TilePattern::parse(&text).unwrap();
    s.dispatch(
        "tiles.pattern_paint",
        &CommandArgs::new(&[("name", "test"), ("origin", "3,0"), ("layer", "0")]),
    )
    .unwrap();
    assert_eq!(
        s.world.entities.iter().filter(|e| !e.sprite.is_empty()).count(),
        4
    );
}

#[test]
fn tile_metadata_reaches_entities_lights_and_navigation() {
    const TILES: &str = "seal tiles 1
default sprites/wall.png
tile sprites/wall.png
  data type=wall
  occluder yes
  navigation blocked
";
    let rules = RuleSet::parse(TILES).unwrap();
    assert_eq!(rules.def_for("sprites/wall.png").unwrap().navigation, TileNavigation::Blocked);
    let data = encode_tile_data(&[("type".into(), "wall".into())]);
    assert!(data.starts_with("tiledata:"));
    let (mut s, dir) = session("meta");
    std::fs::create_dir_all(dir.join("tiles")).unwrap();
    std::fs::write(dir.join("tiles/wall.tiles"), TILES).unwrap();
    s.dispatch(
        "tiles.paint",
        &CommandArgs::new(&[
            ("brush", "tiles/wall.tiles"),
            ("cells", "0,0"),
            ("solid", "true"),
        ]),
    )
    .unwrap();
    let e = s
        .world
        .entities
        .iter()
        .find(|e| e.sprite == "sprites/wall.png")
        .unwrap();
    assert_eq!(e.tag, "nav_block");
    assert!(e.overrides.starts_with("tiledata:"));
    let before = s.world.entities.len();
    s.dispatch(
        "tiles.paint",
        &CommandArgs::new(&[("brush", "tiles/wall.tiles"), ("cells", "1,0")]),
    )
    .unwrap();
    s.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert_eq!(s.world.entities.len(), before);
}

#[test]
fn flood_fill_matches_same_sprite_only() {
    let grid = Grid::Square;
    let mut m = std::collections::HashMap::new();
    m.insert((0, 0), "g".into());
    m.insert((1, 0), "g".into());
    m.insert((0, 1), "d".into());
    let at = |c: (i32, i32)| m.get(&c).cloned();
    let filled = flood_fill(grid, 0, &at, (0, 0), 64);
    assert_eq!(filled.len(), 2);
    assert!(RuleSet::parse("rule 1x pic.png\n").is_err());
}