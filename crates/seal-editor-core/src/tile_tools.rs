//! Tile brush geometry, flood fill, weighted random picks, and named patterns.

use std::collections::VecDeque;

use crate::tiles::Grid;

/// Cap for bucket fill so a mis-click cannot freeze the editor.
pub const FLOOD_FILL_LIMIT: usize = 4096;

/// How the Tile Brush paints in the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PaintTool {
    #[default]
    Brush,
    Rectangle,
    Line,
    Fill,
    Picker,
    Eraser,
    Random,
    PatternPaint,
    PatternSave,
}

impl PaintTool {
    pub fn parse(text: &str) -> PaintTool {
        match text.trim() {
            "rectangle" | "rect" => PaintTool::Rectangle,
            "line" => PaintTool::Line,
            "fill" | "bucket" => PaintTool::Fill,
            "picker" | "pick" => PaintTool::Picker,
            "eraser" | "erase" => PaintTool::Eraser,
            "random" => PaintTool::Random,
            "pattern" | "pattern_paint" | "stamp" => PaintTool::PatternPaint,
            "pattern_save" | "save_pattern" => PaintTool::PatternSave,
            _ => PaintTool::Brush,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            PaintTool::Brush => "brush",
            PaintTool::Rectangle => "rectangle",
            PaintTool::Line => "line",
            PaintTool::Fill => "fill",
            PaintTool::Picker => "picker",
            PaintTool::Eraser => "eraser",
            PaintTool::Random => "random",
            PaintTool::PatternPaint => "pattern",
            PaintTool::PatternSave => "pattern_save",
        }
    }
}

/// Every cell in the axis-aligned box between two corners, inclusive.
pub fn rectangle_cells(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    let (i0, i1) = (from.0.min(to.0), from.0.max(to.0));
    let (j0, j1) = (from.1.min(to.1), from.1.max(to.1));
    let mut out = Vec::new();
    for i in i0..=i1 {
        for j in j0..=j1 {
            out.push((i, j));
        }
    }
    out
}

/// Flood fill on one layer: same picture as the start cell (empty counts as one
/// kind). Returns an empty list when the start cell has no tile.
pub fn flood_fill(
    grid: Grid,
    _layer: i32,
    sprite_at: &dyn Fn((i32, i32)) -> Option<String>,
    start: (i32, i32),
    limit: usize,
) -> Vec<(i32, i32)> {
    let target = match sprite_at(start) {
        Some(s) => s,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut queue = VecDeque::from([start]);
    while let Some(cell) = queue.pop_front() {
        if !seen.insert(cell) {
            continue;
        }
        if sprite_at(cell).as_deref() != Some(target.as_str()) {
            continue;
        }
        out.push(cell);
        if out.len() >= limit {
            break;
        }
        for n in grid.neighbours(cell) {
            if !seen.contains(&n) {
                queue.push_back(n);
            }
        }
    }
    out
}

/// Weighted pick from `picture:weight;...` using a deterministic seed.
pub fn weighted_pick(seed: u64, weights: &str) -> Option<String> {
    let mut entries: Vec<(String, u32)> = Vec::new();
    for piece in weights.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (pic, w) = piece.rsplit_once(':').unwrap_or((piece, "1"));
        let w: u32 = w.trim().parse().unwrap_or(1).max(1);
        if !pic.is_empty() {
            entries.push((pic.to_string(), w));
        }
    }
    if entries.is_empty() {
        return None;
    }
    let total: u64 = entries.iter().map(|(_, w)| *w as u64).sum();
    let pick = seed % total;
    let mut acc = 0u64;
    for (pic, w) in &entries {
        acc += *w as u64;
        if pick < acc {
            return Some(pic.clone());
        }
    }
    entries.last().map(|(p, _)| p.clone())
}

/// One cell in a saved stamp pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternCell {
    pub di: i32,
    pub dj: i32,
    pub sprite: String,
    pub solid: bool,
}

/// A named tile stamp stored in the project.
#[derive(Clone, Debug, PartialEq)]
pub struct TilePattern {
    pub grid: Grid,
    pub cells: Vec<PatternCell>,
}

impl TilePattern {
    pub fn parse(text: &str) -> Result<TilePattern, String> {
        let mut grid = Grid::Square;
        let mut cells = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with("seal tile_pattern") {
                continue;
            }
            let mut parts = line.split_whitespace();
            match parts.next() {
                Some("grid") => {
                    grid = Grid::parse(parts.next().unwrap_or("square"));
                }
                Some("cell") => {
                    let rel =
                        parts.next().ok_or_else(|| format!("line {}: cell needs coords", n + 1))?;
                    let (a, b) = rel
                        .split_once(',')
                        .ok_or_else(|| format!("line {}: coords are i,j", n + 1))?;
                    let di: i32 = a.parse().map_err(|_| format!("line {}: bad coord", n + 1))?;
                    let dj: i32 = b.parse().map_err(|_| format!("line {}: bad coord", n + 1))?;
                    let mut solid = false;
                    let sprite = match parts.next() {
                        Some("solid") => {
                            solid = true;
                            parts
                                .next()
                                .ok_or_else(|| {
                                    format!("line {}: cell needs a picture path", n + 1)
                                })?
                                .to_string()
                        }
                        Some(pic) => pic.to_string(),
                        None => return Err(format!("line {}: cell needs a picture path", n + 1)),
                    };
                    cells.push(PatternCell { di, dj, sprite, solid });
                }
                _ => return Err(format!("line {}: unknown row", n + 1)),
            }
        }
        if cells.is_empty() {
            return Err("a pattern needs at least one cell".into());
        }
        Ok(TilePattern { grid, cells })
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from("seal tile_pattern 1\n");
        out.push_str(&format!("grid {}\n", self.grid.id()));
        for c in &self.cells {
            if c.solid {
                out.push_str(&format!("cell {},{} solid {}\n", c.di, c.dj, c.sprite));
            } else {
                out.push_str(&format!("cell {},{} {}\n", c.di, c.dj, c.sprite));
            }
        }
        out
    }

    /// Scene cells when stamping at `origin`.
    pub fn stamp_at(&self, origin: (i32, i32)) -> Vec<((i32, i32), String, bool)> {
        self.cells
            .iter()
            .map(|c| ((origin.0 + c.di, origin.1 + c.dj), c.sprite.clone(), c.solid))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_cells_cover_the_box_on_every_grid() {
        for grid in [Grid::Square, Grid::Iso, Grid::Hex] {
            let _ = grid;
            let cells = rectangle_cells((1, 2), (3, 4));
            assert_eq!(cells.len(), 9);
            assert!(cells.contains(&(1, 2)));
            assert!(cells.contains(&(3, 4)));
        }
    }

    #[test]
    fn flood_fill_stays_inside_matching_region() {
        let grid = Grid::Square;
        let mut map = std::collections::HashMap::new();
        map.insert((0, 0), "a".into());
        map.insert((1, 0), "a".into());
        map.insert((2, 0), "b".into());
        map.insert((0, 1), "a".into());
        let at = |c: (i32, i32)| map.get(&c).cloned();
        let filled = flood_fill(grid, 0, &at, (0, 0), 100);
        assert_eq!(filled.len(), 3);
        assert!(!filled.contains(&(2, 0)));
    }

    #[test]
    fn weighted_pick_follows_the_seed() {
        let w = "sprites/a.png:1;sprites/b.png:3";
        assert_eq!(weighted_pick(0, w).as_deref(), Some("sprites/a.png"));
        assert_eq!(weighted_pick(1, w).as_deref(), Some("sprites/b.png"));
        assert_eq!(weighted_pick(1, w), weighted_pick(5, w));
    }

    #[test]
    fn pattern_round_trips_and_stamps() {
        let pat = TilePattern {
            grid: Grid::Hex,
            cells: vec![
                PatternCell { di: 0, dj: 0, sprite: "sprites/x.png".into(), solid: true },
                PatternCell { di: 1, dj: 0, sprite: "sprites/y.png".into(), solid: false },
            ],
        };
        let back = TilePattern::parse(&pat.to_text()).unwrap();
        assert_eq!(back, pat);
        let stamped = back.stamp_at((5, 5));
        assert_eq!(stamped.len(), 2);
        assert_eq!(stamped[0].0, (5, 5));
    }

    #[test]
    fn broken_pattern_file_is_refused() {
        assert!(TilePattern::parse("cell bad\n").is_err());
    }
}
