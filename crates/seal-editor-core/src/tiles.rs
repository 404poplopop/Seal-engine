//! Tilemap painting: the grid a tile brush snaps to (square, isometric or hex),
//! rule files that pick each tile's picture from its neighbours, and the
//! `tiles.paint` / `tiles.erase` commands. Tiles stay ordinary sprite objects,
//! so everything else (layers, tint, collision by role, undo) applies to them.

use std::path::Path;

use crate::{CommandArgs, CoreError, EditorSession};

/// Extension of a tile rule file.
pub const RULE_EXT: &str = "tiles";

/// Create-panel brush value prefix for painting prefab copies on tile cells.
pub const PREFAB_BRUSH_PREFIX: &str = "prefab:";

/// The shape of the cells a tile brush paints into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grid {
    /// One-unit squares.
    Square,
    /// Diamonds one unit wide and half a unit tall.
    Iso,
    /// Pointy-top hexagons one unit wide.
    Hex,
}

const HEX_ROW: f32 = 0.866_025_4; // sqrt(3) / 2

impl Grid {
    pub fn parse(text: &str) -> Grid {
        match text {
            "iso" | "isometric" => Grid::Iso,
            "hex" | "hexagon" => Grid::Hex,
            _ => Grid::Square,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Grid::Square => "square",
            Grid::Iso => "iso",
            Grid::Hex => "hex",
        }
    }

    /// The cell holding a scene point.
    pub fn cell_at(self, x: f32, y: f32) -> (i32, i32) {
        match self {
            Grid::Square => (x.floor() as i32, y.floor() as i32),
            Grid::Iso => ((x + 2.0 * y).round() as i32, (2.0 * y - x).round() as i32),
            Grid::Hex => {
                let r = y / HEX_ROW;
                let q = x - r * 0.5;
                hex_round(q, r)
            }
        }
    }

    /// The centre of a cell in scene units.
    pub fn center(self, cell: (i32, i32)) -> (f32, f32) {
        let (i, j) = (cell.0 as f32, cell.1 as f32);
        match self {
            Grid::Square => (i + 0.5, j + 0.5),
            Grid::Iso => ((i - j) * 0.5, (i + j) * 0.25),
            Grid::Hex => (i + j * 0.5, j * HEX_ROW),
        }
    }

    /// Size of one cell's picture: width and height in scene units.
    pub fn cell_size(self) -> (f32, f32) {
        match self {
            Grid::Square => (1.0, 1.0),
            Grid::Iso => (1.0, 0.5),
            Grid::Hex => (1.0, 2.0 / 3f32.sqrt()),
        }
    }

    /// The corners of a cell's outline, for drawing the hover cell.
    pub fn outline(self, cell: (i32, i32)) -> Vec<(f32, f32)> {
        let (cx, cy) = self.center(cell);
        match self {
            Grid::Square => vec![
                (cx - 0.5, cy - 0.5),
                (cx + 0.5, cy - 0.5),
                (cx + 0.5, cy + 0.5),
                (cx - 0.5, cy + 0.5),
            ],
            Grid::Iso => vec![(cx, cy - 0.25), (cx + 0.5, cy), (cx, cy + 0.25), (cx - 0.5, cy)],
            Grid::Hex => {
                let r = 1.0 / 3f32.sqrt();
                (0..6)
                    .map(|k| {
                        let a =
                            std::f32::consts::FRAC_PI_6 + k as f32 * std::f32::consts::FRAC_PI_3;
                        (cx + r * a.cos(), cy + r * a.sin())
                    })
                    .collect()
            }
        }
    }

    /// Neighbouring cells in the order rule patterns use: square N E S W,
    /// iso up-right, down-right, down-left, up-left (the four edges, clockwise
    /// from the top), hex the six sides E NE NW W SW SE.
    pub fn neighbours(self, cell: (i32, i32)) -> Vec<(i32, i32)> {
        let (i, j) = cell;
        match self {
            Grid::Square => vec![(i, j + 1), (i + 1, j), (i, j - 1), (i - 1, j)],
            // Diamonds share edges up-right, down-right, down-left and up-left.
            Grid::Iso => vec![(i + 1, j), (i, j - 1), (i - 1, j), (i, j + 1)],
            Grid::Hex => {
                vec![(i + 1, j), (i, j + 1), (i - 1, j + 1), (i - 1, j), (i, j - 1), (i + 1, j - 1)]
            }
        }
    }

    /// Cells along a straight drag from one cell to another, both included.
    pub fn line(self, from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
        let (a, b) = (self.center(from), self.center(to));
        let (step_w, _) = self.cell_size();
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let steps = ((len / (step_w * 0.25)).ceil() as i32).max(1);
        let mut out = Vec::new();
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let cell = self.cell_at(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            if out.last() != Some(&cell) && !out.contains(&cell) {
                out.push(cell);
            }
        }
        out
    }
}

fn hex_round(q: f32, r: f32) -> (i32, i32) {
    let s = -q - r;
    let (mut rq, mut rr, rs) = (q.round(), r.round(), s.round());
    let (dq, dr, ds) = ((rq - q).abs(), (rr - r).abs(), (rs - s).abs());
    if dq > dr && dq > ds {
        rq = -rr - rs;
    } else if dr > ds {
        rr = -rq - rs;
    }
    (rq as i32, rr as i32)
}

/// How a tile type affects the 2D navigation grid when painted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TileNavigation {
    #[default]
    Walkable,
    Blocked,
}

/// Per-picture settings in a tile rule file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TileDef {
    pub custom_data: Vec<(String, String)>,
    pub animate_frames: Vec<String>,
    pub animate_fps: f32,
    pub occluder: bool,
    pub navigation: TileNavigation,
    pub prefab: String,
}

/// A tile rule file: each rule is a neighbour pattern and the picture to use
/// when it matches. Pattern letters follow `Grid::neighbours`: `1` the
/// neighbour must be a tile of this set, `0` it must not, `*` either.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleSet {
    pub rules: Vec<(String, String)>,
    pub defs: std::collections::HashMap<String, TileDef>,
}

impl RuleSet {
    pub fn parse(text: &str) -> Result<RuleSet, String> {
        let mut rules = Vec::new();
        let mut defs = std::collections::HashMap::new();
        let mut current: Option<String> = None;
        for (n, raw) in text.lines().enumerate() {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("seal tiles") {
                continue;
            }
            if raw.starts_with(' ') || raw.starts_with('\t') {
                let Some(pic) = current.as_ref() else {
                    return Err(format!("line {}: indented row without a tile header", n + 1));
                };
                let def = defs.entry(pic.clone()).or_default();
                parse_def_line(def, trimmed).map_err(|e| format!("line {}: {e}", n + 1))?;
                continue;
            }
            let mut parts = trimmed.split_whitespace();
            match parts.next() {
                Some("tile") | Some("brush") => {
                    let picture = parts
                        .next()
                        .ok_or_else(|| format!("line {}: tile needs a picture path", n + 1))?
                        .to_string();
                    current = Some(picture.clone());
                    defs.entry(picture).or_default();
                }
                Some("rule") => {
                    current = None;
                    let pattern = parts
                        .next()
                        .ok_or_else(|| format!("line {}: rule needs a pattern", n + 1))?;
                    let picture = parts
                        .next()
                        .ok_or_else(|| format!("line {}: rule needs a picture", n + 1))?;
                    if !pattern.chars().all(|c| matches!(c, '0' | '1' | '*')) {
                        return Err(format!("line {}: bad rule pattern", n + 1));
                    }
                    rules.push((pattern.to_string(), picture.to_string()));
                }
                Some("default") => {
                    current = None;
                    let picture = parts
                        .next()
                        .ok_or_else(|| format!("line {}: default needs a picture", n + 1))?;
                    rules.push((String::new(), picture.to_string()));
                }
                _ => {
                    return Err(format!(
                        "line {}: expected `rule`, `default`, `tile` or `brush`",
                        n + 1
                    ));
                }
            }
        }
        if rules.is_empty() {
            return Err("a tile rule file needs at least one rule".into());
        }
        Ok(RuleSet { rules, defs })
    }

    pub fn def_for(&self, picture: &str) -> Option<&TileDef> {
        self.defs.get(picture)
    }

    pub fn load(path: &Path) -> Result<RuleSet, String> {
        let text =
            std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
        RuleSet::parse(&text)
    }

    /// Every picture this set can paint.
    pub fn pictures(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self.rules.iter().map(|(_, p)| p.as_str()).collect();
        out.dedup();
        out
    }

    /// The picture for a cell whose neighbours are (`true`) or are not tiles of this set.
    pub fn pick(&self, around: &[bool]) -> &str {
        let fits = |pattern: &str| {
            pattern.chars().zip(around).all(|(c, here)| match c {
                '1' => *here,
                '0' => !*here,
                _ => true,
            })
        };
        self.rules
            .iter()
            .find(|(pattern, _)| fits(pattern))
            .map(|(_, picture)| picture.as_str())
            .unwrap_or_else(|| self.rules[0].1.as_str())
    }
}

fn parse_def_line(def: &mut TileDef, line: &str) -> Result<(), String> {
    let mut parts = line.split_whitespace();
    match parts.next() {
        Some("data") => {
            let kv = parts.next().ok_or_else(|| "data needs key=value".to_string())?;
            let (k, v) = kv.split_once('=').ok_or_else(|| "data needs key=value".to_string())?;
            def.custom_data.push((k.to_string(), v.to_string()));
        }
        Some("animate") => {
            let tokens: Vec<&str> = parts.collect();
            let ix = tokens
                .iter()
                .position(|t| *t == "fps")
                .ok_or_else(|| "animate needs frames and fps".to_string())?;
            let frames: Vec<String> = tokens[..ix].iter().map(|s| (*s).to_string()).collect();
            let fps: f32 = tokens
                .get(ix + 1)
                .unwrap_or(&"8")
                .parse()
                .map_err(|_| "fps needs a number".to_string())?;
            if frames.is_empty() {
                return Err("animate needs at least one picture".into());
            }
            def.animate_frames = frames;
            def.animate_fps = fps;
        }
        Some("occluder") => {
            let v = parts.next().unwrap_or("yes");
            def.occluder = matches!(v, "yes" | "true" | "1");
        }
        Some("navigation") => {
            let v = parts.next().unwrap_or("walkable");
            def.navigation = match v {
                "blocked" | "block" => TileNavigation::Blocked,
                _ => TileNavigation::Walkable,
            };
        }
        Some("prefab") => {
            def.prefab = parts.next().unwrap_or_default().to_string();
        }
        _ => return Err("unknown tile property".into()),
    }
    Ok(())
}

/// Encode custom data for round-trip on the entity overrides field.
pub fn encode_tile_data(pairs: &[(String, String)]) -> String {
    if pairs.is_empty() {
        return String::new();
    }
    let body: String = pairs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(",");
    format!("tiledata:{body}")
}

/// Instance overrides for a painted tile: `tiledata:` custom fields and optional `tileset:` source file.
pub fn pack_tile_overrides(custom: &[(String, String)], tile_set: Option<&str>) -> String {
    let mut parts = Vec::new();
    let data = encode_tile_data(custom);
    if !data.is_empty() {
        parts.push(data);
    }
    if let Some(path) = tile_set.filter(|p| !p.is_empty()) {
        parts.push(format!("tileset:{path}"));
    }
    parts.join(";")
}

pub fn decode_tile_data(overrides: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for piece in overrides.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let Some(rest) = piece.strip_prefix("tiledata:") else {
            continue;
        };
        for kv in rest.split(',') {
            if let Some((k, v)) = kv.split_once('=') {
                out.push((k.to_string(), v.to_string()));
            }
        }
    }
    out
}

pub fn decode_tile_set(overrides: &str) -> Option<String> {
    overrides
        .split(';')
        .map(str::trim)
        .find_map(|piece| piece.strip_prefix("tileset:").map(str::to_string))
}

/// A grid-snapped sprite instance (not a prefab copy) counts as a painted tile.
pub fn is_painted_tile(entity: &crate::Entity) -> bool {
    if entity.sprite.is_empty() || entity.source_kind == "prefab" {
        return false;
    }
    for grid in [Grid::Square, Grid::Iso, Grid::Hex] {
        let cell = grid.cell_at(entity.translation[0], entity.translation[1]);
        let (cx, cy) = grid.center(cell);
        let (w, h) = grid.cell_size();
        if (entity.translation[0] - cx).abs() > 0.02 || (entity.translation[1] - cy).abs() > 0.02 {
            continue;
        }
        if (entity.scale[0] - w).abs() < 0.05 && (entity.scale[1] - h).abs() < 0.05 {
            return true;
        }
    }
    false
}

fn apply_tile_def(entity: &mut crate::Entity, def: &TileDef, solid: bool, tile_set: Option<&str>) {
    entity.tag.clear();
    if def.navigation == TileNavigation::Blocked {
        entity.tag = "nav_block".into();
    } else if def.occluder {
        entity.tag = "occluder".into();
    } else if solid {
        entity.tag = "ground".into();
    }
    entity.overrides = pack_tile_overrides(&def.custom_data, tile_set);
    if def.animate_frames.len() >= 2 {
        entity.sprite = def.animate_frames[0].clone();
        entity.frames = def.animate_frames.len() as u32;
        entity.fps = def.animate_fps.max(0.1);
    }
}

fn grid_of(session: &EditorSession, args: &CommandArgs) -> Grid {
    match args.get("grid") {
        Some(grid) => Grid::parse(grid),
        None => Grid::parse(&session.draft("tile-grid").unwrap_or_default()),
    }
}

fn parse_cells(text: &str) -> Result<Vec<(i32, i32)>, CoreError> {
    text.split(';')
        .filter(|s| !s.trim().is_empty())
        .map(|pair| {
            let (a, b) =
                pair.split_once(',').ok_or_else(|| CoreError::new("cells are `i,j;i,j`"))?;
            Ok((
                a.trim().parse().map_err(|_| CoreError::new("cells are whole numbers"))?,
                b.trim().parse().map_err(|_| CoreError::new("cells are whole numbers"))?,
            ))
        })
        .collect()
}

/// The tile object sitting at a cell on a layer, if any.
fn tile_at(
    session: &EditorSession,
    grid: Grid,
    cell: (i32, i32),
    layer: i32,
    set: Option<&[&str]>,
) -> Option<String> {
    let (x, y) = grid.center(cell);
    session
        .world
        .entities
        .iter()
        .find(|e| {
            !e.sprite.is_empty()
                && e.layer == layer
                && (e.translation[0] - x).abs() < 0.01
                && (e.translation[1] - y).abs() < 0.01
                && set.is_none_or(|set| set.contains(&e.sprite.as_str()))
        })
        .map(|e| e.id.clone())
}

fn place_prefab_cell(
    session: &mut EditorSession,
    root: &Path,
    prefab_rel: &str,
    grid: Grid,
    cell: (i32, i32),
    layer: i32,
) -> bool {
    if let Some(old) = tile_at(session, grid, cell, layer, None) {
        session.world.entities.retain(|e| e.id != old);
    }
    let (x, y) = grid.center(cell);
    let file = if root.join(prefab_rel).is_file() {
        root.join(prefab_rel)
    } else {
        root.join("prefabs").join(prefab_rel)
    };
    let Ok(text) = std::fs::read_to_string(&file) else {
        return false;
    };
    let Ok(id) = session.world.add_prefab_text(&text, None) else {
        return false;
    };
    let asset = Path::new(prefab_rel)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if let Some(e) = session.world.entity_mut(&id) {
        e.source_kind = "prefab".into();
        e.source_asset = asset;
        e.translation[0] = x;
        e.translation[1] = y;
        e.layer = layer;
    }
    true
}

/// `tiles.paint`: fill cells with a picture or a rule set, on one tile layer.
/// Args: `brush` (picture or `.tiles` path), `cells` (`i,j;i,j`), optional
/// `grid` (square, iso, hex), `layer` (number) and `solid` (true makes the
/// tiles ground the player stands on).
pub fn paint_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let brush = args
        .get("brush")
        .filter(|b| !b.is_empty())
        .ok_or_else(|| CoreError::new("tiles.paint needs a brush"))?
        .to_string();
    let cells = parse_cells(args.get("cells").unwrap_or_default())?;
    let grid = grid_of(session, args);
    let layer: i32 = args.get("layer").and_then(|l| l.trim().parse().ok()).unwrap_or(0);
    let solid = args.get("solid") == Some("true");
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project to paint tiles"))?;
    if let Some(prefab_rel) = brush.strip_prefix(PREFAB_BRUSH_PREFIX) {
        let previous = session.world.to_text();
        let mut touched = false;
        for &cell in &cells {
            if place_prefab_cell(session, &root, prefab_rel, grid, cell, layer) {
                touched = true;
            }
        }
        if touched {
            finish(session, previous);
        }
        return Ok(());
    }
    let rules = if brush.ends_with(&format!(".{RULE_EXT}")) {
        Some(RuleSet::load(&root.join(&brush)).map_err(CoreError::new)?)
    } else {
        None
    };
    let random_weights = args.get("weights").unwrap_or_default();
    let random_seed = args.get("seed").and_then(|s| s.parse::<u64>().ok()).unwrap_or(1);
    let previous = session.world.to_text();
    let (w, h) = grid.cell_size();
    let first_picture =
        rules.as_ref().map(|r| r.rules[0].1.clone()).unwrap_or_else(|| brush.clone());
    let set: Vec<String> = rules
        .as_ref()
        .map(|r| r.pictures().iter().map(|p| p.to_string()).collect())
        .unwrap_or_else(|| vec![brush.clone()]);
    let set_refs: Vec<&str> = set.iter().map(String::as_str).collect();
    let mut touched = false;
    for &cell in &cells {
        let picture = if !random_weights.is_empty() {
            crate::tile_tools::weighted_pick(random_seed ^ cell_wang_hash(cell), random_weights)
                .unwrap_or_else(|| first_picture.clone())
        } else {
            first_picture.clone()
        };
        let def = rules.as_ref().and_then(|r| r.def_for(&picture));
        if let Some(def) = def {
            if !def.prefab.is_empty() {
                if place_prefab_cell(session, &root, &def.prefab, grid, cell, layer) {
                    touched = true;
                }
                continue;
            }
        }
        if tile_at(session, grid, cell, layer, Some(&set_refs)).is_some() {
            continue;
        }
        // Painting over another picture on this layer replaces it.
        if let Some(old) = tile_at(session, grid, cell, layer, None) {
            session.world.entities.retain(|e| e.id != old);
        }
        let id = session.world.add("empty");
        let stem = Path::new(&picture).file_stem().map(|s| s.to_string_lossy().to_string());
        let name = session.world.free_name(&stem.unwrap_or_else(|| "Tile".into()));
        let (x, y) = grid.center(cell);
        if let Some(e) = session.world.entity_mut(&id) {
            e.name = name;
            e.sprite = picture.clone();
            e.translation[0] = x;
            e.translation[1] = y;
            e.layer = layer;
            e.scale[0] = w;
            e.scale[1] = h;
            let tile_set = rules.as_ref().map(|_| brush.as_str());
            if let Some(def) = def {
                apply_tile_def(e, def, solid, tile_set);
            } else if solid {
                e.tag = "ground".into();
            }
        }
        touched = true;
    }
    if let Some(rules) = &rules {
        retile(session, grid, layer, rules, &cells);
    }
    if !touched && rules.is_none() {
        return Ok(());
    }
    finish(session, previous);
    Ok(())
}

/// Re-pick pictures for painted cells and their neighbours from a rule set.
fn retile(
    session: &mut EditorSession,
    grid: Grid,
    layer: i32,
    rules: &RuleSet,
    cells: &[(i32, i32)],
) {
    let set = rules.pictures().iter().map(|p| p.to_string()).collect::<Vec<_>>();
    let refs: Vec<&str> = set.iter().map(String::as_str).collect();
    let mut todo: Vec<(i32, i32)> = Vec::new();
    for &cell in cells {
        for c in std::iter::once(cell).chain(grid.neighbours(cell)) {
            if !todo.contains(&c) {
                todo.push(c);
            }
        }
    }
    let mut changes = Vec::new();
    for cell in todo {
        let Some(id) = tile_at(session, grid, cell, layer, Some(&refs)) else { continue };
        let around: Vec<bool> = grid
            .neighbours(cell)
            .into_iter()
            .map(|n| tile_at(session, grid, n, layer, Some(&refs)).is_some())
            .collect();
        changes.push((id, rules.pick(&around).to_string()));
    }
    for (id, picture) in changes {
        if let Some(e) = session.world.entity_mut(&id) {
            e.sprite = picture;
        }
    }
}

/// `tiles.erase`: remove the tiles at cells on one layer (any picture), then
/// re-pick neighbours when a rule set is given as `brush`.
pub fn erase_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let cells = parse_cells(args.get("cells").unwrap_or_default())?;
    let grid = grid_of(session, args);
    let layer: i32 = args.get("layer").and_then(|l| l.trim().parse().ok()).unwrap_or(0);
    let previous = session.world.to_text();
    let mut gone = false;
    for &cell in &cells {
        while let Some(id) = tile_at(session, grid, cell, layer, None) {
            session.world.entities.retain(|e| e.id != id);
            gone = true;
        }
    }
    if let Some(brush) = args.get("brush").filter(|b| b.ends_with(&format!(".{RULE_EXT}"))) {
        if let Some(root) = session.project.as_ref().map(|p| p.root.clone()) {
            if let Ok(rules) = RuleSet::load(&root.join(brush)) {
                retile(session, grid, layer, &rules, &cells);
            }
        }
    }
    if gone {
        finish(session, previous);
    }
    Ok(())
}

fn cell_wang_hash(cell: (i32, i32)) -> u64 {
    let (i, j) = cell;
    ((i as u64).wrapping_mul(3_748_241) ^ (j as u64).wrapping_mul(2_654_435))
        + 0x9E37_79B9_7F4A_7C15
}

/// Picture on a layer at a grid cell, if any.
pub fn sprite_at_cell(
    session: &EditorSession,
    grid: Grid,
    cell: (i32, i32),
    layer: i32,
) -> Option<String> {
    let (x, y) = grid.center(cell);
    session
        .world
        .entities
        .iter()
        .find(|e| {
            !e.sprite.is_empty()
                && e.layer == layer
                && (e.translation[0] - x).abs() < 0.01
                && (e.translation[1] - y).abs() < 0.01
        })
        .map(|e| e.sprite.clone())
}

/// `tiles.pattern_save`: write a stamp under `tilepatterns/`.
pub fn pattern_save_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = args
        .get("name")
        .filter(|n| !n.is_empty())
        .ok_or_else(|| CoreError::new("pattern save needs a name"))?;
    if name.contains('/') || name.contains("..") {
        return Err(CoreError::new("pattern name must be a simple file name"));
    }
    let cells = parse_cells(args.get("cells").unwrap_or_default())?;
    if cells.is_empty() {
        return Err(CoreError::new("pattern save needs a rectangle of cells"));
    }
    let grid = grid_of(session, args);
    let layer: i32 = args.get("layer").and_then(|l| l.trim().parse().ok()).unwrap_or(0);
    let (i0, j0) = cells[0];
    let mut pattern_cells = Vec::new();
    for cell in cells {
        let Some(sprite) = sprite_at_cell(session, grid, cell, layer) else {
            continue;
        };
        let solid = session.world.entities.iter().any(|e| {
            !e.sprite.is_empty() && e.sprite == sprite && e.layer == layer && e.tag == "ground" && {
                let (x, y) = grid.center(cell);
                (e.translation[0] - x).abs() < 0.01 && (e.translation[1] - y).abs() < 0.01
            }
        });
        pattern_cells.push(crate::tile_tools::PatternCell {
            di: cell.0 - i0,
            dj: cell.1 - j0,
            sprite,
            solid,
        });
    }
    if pattern_cells.is_empty() {
        return Err(CoreError::new("the selection has no tiles to save"));
    }
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project to save a pattern"))?;
    let dir = root.join("tilepatterns");
    std::fs::create_dir_all(&dir).map_err(|e| CoreError::new(e.to_string()))?;
    let pat = crate::tile_tools::TilePattern { grid, cells: pattern_cells };
    std::fs::write(dir.join(format!("{name}.pattern")), pat.to_text())
        .map_err(|e| CoreError::new(e.to_string()))?;
    session.notify(format!("Saved tile pattern {name}"), false);
    Ok(())
}

/// `tiles.pattern_paint`: stamp a saved pattern at one origin cell.
pub fn pattern_paint_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = args
        .get("name")
        .filter(|n| !n.is_empty())
        .ok_or_else(|| CoreError::new("pattern paint needs a name"))?;
    let origin = parse_cells(args.get("origin").unwrap_or_default())?
        .into_iter()
        .next()
        .ok_or_else(|| CoreError::new("pattern paint needs an origin cell"))?;
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project to paint a pattern"))?;
    let path = root.join("tilepatterns").join(format!("{name}.pattern"));
    let text =
        std::fs::read_to_string(&path).map_err(|_| CoreError::new("pattern was not found"))?;
    let pat = crate::tile_tools::TilePattern::parse(&text).map_err(CoreError::new)?;
    let layer: i32 = args.get("layer").and_then(|l| l.trim().parse().ok()).unwrap_or(0);
    let previous = session.world.to_text();
    let grid = pat.grid;
    let (w, h) = grid.cell_size();
    let mut touched = false;
    for ((i, j), sprite, solid) in pat.stamp_at(origin) {
        if let Some(old) = tile_at(session, grid, (i, j), layer, None) {
            session.world.entities.retain(|e| e.id != old);
        }
        let id = session.world.add("empty");
        let tile_name = session.world.free_name("Tile");
        let (x, y) = grid.center((i, j));
        if let Some(e) = session.world.entity_mut(&id) {
            e.name = tile_name;
            e.sprite = sprite;
            e.translation[0] = x;
            e.translation[1] = y;
            e.layer = layer;
            e.scale[0] = w;
            e.scale[1] = h;
            if solid {
                e.tag = "ground".into();
            }
        }
        touched = true;
    }
    if touched {
        finish(session, previous);
    }
    Ok(())
}

fn finish(session: &mut EditorSession, previous: String) {
    session.adopt_world();
    crate::models::push_scene_past(session, previous);
    session.mark_scene();
}

pub const TILE_GRID_DRAFT: &str = "tile-grid";
pub const TILE_LAYER_DRAFT: &str = "tile-layer";
pub const TILE_SOLID_DRAFT: &str = "tile-solid";
pub const TILE_TOOL_DRAFT: &str = "tile-tool";
pub const TILE_ERASE_DRAFT: &str = "tile-erase";
pub const TILE_RANDOM_WEIGHTS_DRAFT: &str = "tile-random-weights";
pub const TILE_PATTERN_NAME_DRAFT: &str = "tile-pattern-name";
pub const TILE_RANDOM_SEED_DRAFT: &str = "tile-random-seed";

/// Remember the active tile brush tool (autosaved with other drafts).
pub fn set_paint_tool(session: &mut EditorSession, tool: &str) -> Result<(), CoreError> {
    session.set_draft(TILE_TOOL_DRAFT, tool)?;
    let erase = if tool == "eraser" { "true" } else { "false" };
    session.set_draft(TILE_ERASE_DRAFT, erase)?;
    Ok(())
}

/// Metadata row for the Inspector tile section.
#[derive(Clone, Debug, Default)]
pub struct TileInspectorInfo {
    pub custom_data: Vec<(String, String)>,
    pub tile_set: Option<String>,
    pub occluder: bool,
    pub navigation_blocked: bool,
}

pub fn tile_inspector_info(session: &EditorSession, entity: &crate::Entity) -> TileInspectorInfo {
    let mut info = TileInspectorInfo {
        custom_data: decode_tile_data(&entity.overrides),
        tile_set: decode_tile_set(&entity.overrides),
        ..TileInspectorInfo::default()
    };
    if info.tile_set.is_none() {
        info.tile_set = find_tile_set_for_picture(session, &entity.sprite);
    }
    if let Some(rel) = info.tile_set.as_ref() {
        if let Some(root) = session.project.as_ref() {
            let path = root.root.join(rel);
            if let Ok(rules) = RuleSet::load(&path) {
                if let Some(def) = rules.def_for(&entity.sprite) {
                    info.occluder = def.occluder;
                    info.navigation_blocked = def.navigation == TileNavigation::Blocked;
                }
            }
        }
    }
    info
}

pub fn find_tile_set_for_picture(session: &EditorSession, picture: &str) -> Option<String> {
    let root = session.project.as_ref()?.root.clone();
    let mut stacks = vec![root.clone()];
    while let Some(dir) = stacks.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stacks.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some(RULE_EXT) {
                continue;
            }
            let rel = path.strip_prefix(&root).ok()?.to_string_lossy().replace('\\', "/");
            let Ok(rules) = RuleSet::load(&path) else { continue };
            if rules.defs.contains_key(picture) || rules.pictures().contains(&picture) {
                return Some(rel);
            }
        }
    }
    None
}

fn mutate_tile_data(
    session: &mut EditorSession,
    id: &str,
    edit: impl FnOnce(Vec<(String, String)>) -> Vec<(String, String)>,
) -> Result<(), CoreError> {
    let previous = session.world.to_text();
    let entity = session.world.entity(id).ok_or_else(|| CoreError::new("unknown object"))?;
    if !is_painted_tile(entity) {
        return Err(CoreError::new("not a painted tile"));
    }
    let tile_set = decode_tile_set(&entity.overrides);
    let next = edit(decode_tile_data(&entity.overrides));
    let packed = pack_tile_overrides(&next, tile_set.as_deref());
    let Some(e) = session.world.entity_mut(id) else {
        return Err(CoreError::new("unknown object"));
    };
    e.overrides = packed;
    finish(session, previous);
    Ok(())
}

/// Set or add one custom data pair on a painted tile (`id`, `key`, `value`).
pub fn tile_data_set_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let id = args.get("id").ok_or_else(|| CoreError::new("tile data needs an object"))?;
    let key = args
        .get("key")
        .filter(|k| !k.is_empty())
        .ok_or_else(|| CoreError::new("tile data needs a key"))?;
    let value = args.get("value").unwrap_or_default().to_string();
    mutate_tile_data(session, id, |mut pairs| {
        if let Some(row) = pairs.iter_mut().find(|(k, _)| k == key) {
            row.1 = value.clone();
        } else {
            pairs.push((key.to_string(), value.clone()));
        }
        pairs
    })
}

/// Remove one custom data key from a painted tile.
pub fn tile_data_remove_cmd(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let id = args.get("id").ok_or_else(|| CoreError::new("tile data needs an object"))?;
    let key = args
        .get("key")
        .filter(|k| !k.is_empty())
        .ok_or_else(|| CoreError::new("tile data needs a key"))?;
    mutate_tile_data(session, id, |pairs| pairs.into_iter().filter(|(k, _)| k != key).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_grid_finds_the_cell_it_centres() {
        for grid in [Grid::Square, Grid::Iso, Grid::Hex] {
            for i in -4..5 {
                for j in -4..5 {
                    let (x, y) = grid.center((i, j));
                    assert_eq!(grid.cell_at(x, y), (i, j), "{grid:?} {i},{j}");
                    // A point a little off centre still lands in the same cell.
                    assert_eq!(grid.cell_at(x + 0.1, y + 0.05), (i, j), "{grid:?} nudged {i},{j}");
                }
            }
        }
    }

    #[test]
    fn iso_and_hex_cells_tile_without_gaps() {
        // Points spread over an area all land in a cell whose outline holds them.
        for grid in [Grid::Iso, Grid::Hex] {
            for k in 0..400 {
                let (x, y) = ((k % 20) as f32 * 0.137 - 1.3, (k / 20) as f32 * 0.091 - 0.9);
                let cell = grid.cell_at(x, y);
                let (cx, cy) = grid.center(cell);
                let (w, h) = grid.cell_size();
                assert!(
                    (x - cx).abs() <= w * 0.5 + 1e-3 && (y - cy).abs() <= h * 0.5 + 1e-3,
                    "{grid:?} {x},{y}"
                );
            }
        }
    }

    #[test]
    fn rules_pick_by_neighbours_first_match_wins() {
        let rules = RuleSet::parse("seal tiles 1\n# dirt under anything\nrule 1*** sprites/dirt.png\ndefault sprites/grass.png\n").unwrap();
        assert_eq!(rules.pick(&[true, false, false, false]), "sprites/dirt.png");
        assert_eq!(rules.pick(&[false, true, true, true]), "sprites/grass.png");
        assert!(RuleSet::parse("rule 12 x.png").is_err());
        assert!(RuleSet::parse("").is_err());
    }

    #[test]
    fn a_drag_line_has_no_holes() {
        for grid in [Grid::Square, Grid::Iso, Grid::Hex] {
            let line = grid.line((0, 0), (6, 3));
            assert_eq!(line.first(), Some(&(0, 0)));
            assert_eq!(line.last(), Some(&(6, 3)));
            for pair in line.windows(2) {
                assert!(
                    grid.neighbours(pair[0]).contains(&pair[1]) || {
                        // Square lines may step diagonally.
                        (pair[0].0 - pair[1].0).abs() <= 1 && (pair[0].1 - pair[1].1).abs() <= 1
                    },
                    "{grid:?} jump {:?}",
                    pair
                );
            }
        }
    }
}
