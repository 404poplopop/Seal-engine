//! Shared sprite draw order, pivot placement, and nine-slice cuts.
//!
//! The editor viewport and the runtime sprite batch both call these so a scene
//! draws in the same order, from the same pivot, with the same tiled slices.

use std::collections::HashMap;

/// Per-object override; empty uses the sorting layer rule, then scene depth.
pub const SORT_MODES: &[&str] = &["default", "y_sort", "axis", "sort_group"];

/// How one sorting layer draws its sprites when no object overrides it.
pub const LAYER_SORTS: &[&str] = &["default", "y_sort", "axis"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ObjectSortMode {
    #[default]
    Default,
    YSort,
    Axis,
    SortGroup,
}

impl ObjectSortMode {
    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "y_sort" => Self::YSort,
            "axis" => Self::Axis,
            "sort_group" => Self::SortGroup,
            _ => Self::Default,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::YSort => "y_sort",
            Self::Axis => "axis",
            Self::SortGroup => "sort_group",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LayerSortMode {
    #[default]
    Default,
    YSort,
    Axis,
}

impl LayerSortMode {
    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "y_sort" => Self::YSort,
            "axis" => Self::Axis,
            _ => Self::Default,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::YSort => "y_sort",
            Self::Axis => "axis",
        }
    }
}

/// Project setting `sprites.sort_axis`: x and y weights for axis sorting.
pub fn parse_sort_axis(text: &str) -> [f32; 2] {
    let parts: Vec<f32> = text
        .split([',', ' '])
        .filter_map(|p| p.trim().parse::<f32>().ok())
        .filter(|v: &f32| v.is_finite())
        .collect();
    match parts.as_slice() {
        [] => [0.0, 1.0],
        [x] => [*x, 1.0],
        [x, y, ..] => [*x, *y],
    }
}

fn format_num(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".into()
    } else {
        trimmed.into()
    }
}

pub fn sort_axis_text(axis: [f32; 2]) -> String {
    format!("{}, {}", format_num(axis[0]), format_num(axis[1]))
}

/// Project setting `sprites.sort_layers`: `layer:mode` pairs separated by commas or newlines.
pub fn parse_sort_layers(text: &str) -> HashMap<i32, LayerSortMode> {
    let mut out = HashMap::new();
    for chunk in text.split([',', '\n', ';']) {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let Some((layer, mode)) = chunk.split_once(':') else { continue };
        let mode = mode.trim();
        if !LAYER_SORTS.contains(&mode) {
            continue;
        }
        let Ok(layer) = layer.trim().parse::<f32>() else { continue };
        if !layer.is_finite() || layer < 0.0 {
            continue;
        }
        out.insert(layer.round() as i32, LayerSortMode::parse(mode));
    }
    out
}

pub fn sort_layers_text(map: &HashMap<i32, LayerSortMode>) -> String {
    let mut keys: Vec<_> = map.keys().copied().collect();
    keys.sort_unstable();
    keys.into_iter()
        .map(|layer| format!("{layer}:{}", map[&layer].name()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One flat sprite about to be sorted for drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SortEntry {
    pub index: usize,
    pub entity_index: usize,
    pub layer: i32,
    pub order: i32,
    pub depth: f32,
    /// Pivot position in scene units (used for Y-sort and axis sort).
    pub pivot_x: f32,
    pub pivot_y: f32,
    pub object_mode: ObjectSortMode,
    /// When set, this sprite draws inside that group's batch.
    pub group_parent: Option<usize>,
}

fn effective_mode(entry: &SortEntry, layer_rules: &HashMap<i32, LayerSortMode>) -> ObjectSortMode {
    if entry.object_mode != ObjectSortMode::Default {
        return entry.object_mode;
    }
    match layer_rules.get(&entry.layer).copied().unwrap_or_default() {
        LayerSortMode::YSort => ObjectSortMode::YSort,
        LayerSortMode::Axis => ObjectSortMode::Axis,
        LayerSortMode::Default => ObjectSortMode::Default,
    }
}

fn axis_key(entry: &SortEntry, axis: [f32; 2]) -> f32 {
    entry.pivot_x * axis[0] + entry.pivot_y * axis[1]
}

/// Sort sprite indices back to front (painter order: low layer first, then order, then depth).
pub fn sort_indices(
    entries: &[SortEntry],
    layer_rules: &HashMap<i32, LayerSortMode>,
    axis: [f32; 2],
) -> Vec<usize> {
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by(|&a, &b| {
        let ea = &entries[a];
        let eb = &entries[b];
        let ga = ea.group_parent.unwrap_or(a);
        let gb = eb.group_parent.unwrap_or(b);
        let group_cmp = ga.cmp(&gb);
        if group_cmp != std::cmp::Ordering::Equal {
            let pa = &entries[ga];
            let pb = &entries[gb];
            let mode_a = effective_mode(pa, layer_rules);
            let mode_b = effective_mode(pb, layer_rules);
            pa.layer
                .cmp(&pb.layer)
                .then(pa.order.cmp(&pb.order))
                .then_with(|| sort_by_mode(pa, pb, mode_a, mode_b, axis))
                .then(pa.depth.partial_cmp(&pb.depth).unwrap_or(std::cmp::Ordering::Equal))
        } else if ea.group_parent.is_some() {
            ea.order.cmp(&eb.order).then(a.cmp(&b))
        } else {
            let mode_a = effective_mode(ea, layer_rules);
            let mode_b = effective_mode(eb, layer_rules);
            ea.layer
                .cmp(&eb.layer)
                .then(ea.order.cmp(&eb.order))
                .then_with(|| sort_by_mode(ea, eb, mode_a, mode_b, axis))
                .then(eb.depth.partial_cmp(&ea.depth).unwrap_or(std::cmp::Ordering::Equal))
                .then(a.cmp(&b))
        }
    });
    order
}

fn sort_by_mode(
    a: &SortEntry,
    b: &SortEntry,
    mode_a: ObjectSortMode,
    mode_b: ObjectSortMode,
    axis: [f32; 2],
) -> std::cmp::Ordering {
    let key = |e: &SortEntry, mode: ObjectSortMode| -> f32 {
        match mode {
            ObjectSortMode::YSort => e.pivot_y,
            ObjectSortMode::Axis => axis_key(e, axis),
            ObjectSortMode::SortGroup | ObjectSortMode::Default => e.depth,
        }
    };
    key(a, mode_a).partial_cmp(&key(b, mode_b)).unwrap_or(std::cmp::Ordering::Equal)
}

/// First ancestor of `index` for which `is_group` is set. `parent_of` walks one step.
pub fn group_root(
    index: usize,
    parent_of: impl Fn(usize) -> Option<usize>,
    is_group: impl Fn(usize) -> bool,
) -> Option<usize> {
    let mut walk = parent_of(index);
    let mut guard = 0;
    while let Some(parent) = walk {
        if is_group(parent) {
            return Some(parent);
        }
        walk = parent_of(parent);
        guard += 1;
        if guard > 64 {
            break;
        }
    }
    None
}

/// Where Y-sort and axis sort measure this sprite: the editor's pivot point.
pub fn sort_pivot(origin: [f32; 2], pivot: [f32; 2], size: [f32; 2]) -> [f32; 2] {
    [origin[0] + (pivot[0] - 0.5) * size[0], origin[1] + (pivot[1] - 0.5) * size[1]]
}

/// Shift from the entity position to the centre of a sprite card.
pub fn card_shift(pivot: [f32; 2], size: [f32; 2]) -> [f32; 2] {
    [(0.5 - pivot[0]) * size[0], (0.5 - pivot[1]) * size[1]]
}

/// Local rectangle with the pivot at the origin. Returns x0, y0, x1, y1, y up.
pub fn pivot_rect(size: [f32; 2], pivot: [f32; 2]) -> [f32; 4] {
    let x0 = -pivot[0] * size[0];
    let y0 = -pivot[1] * size[1];
    [x0, y0, x0 + size[0], y0 + size[1]]
}

/// Cut positions along one side, as fractions 0..1 of the drawn quad and of the
/// frame's texture span. `near`/`far` are the border pixels at the start and
/// end of the span, `frame_px` the frame's pixel length, `scale` the sprite scale.
pub fn nine_cuts(near: f32, far: f32, frame_px: f32, scale: f32) -> ([f32; 4], [f32; 4]) {
    if frame_px <= 0.0 {
        return ([0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 1.0, 1.0]);
    }
    let near = near.clamp(0.0, frame_px);
    let far = far.clamp(0.0, frame_px - near);
    let tex = [0.0, near / frame_px, 1.0 - far / frame_px, 1.0];
    let scale = scale.max(1e-4);
    let (mut a, mut b) = (near / frame_px / scale, far / frame_px / scale);
    if a + b > 1.0 {
        let k = 1.0 / (a + b);
        a *= k;
        b *= k;
    }
    ([0.0, a, 1.0 - b, 1.0], tex)
}

/// One tiled piece of a nine-slice. `span` is fx0, fy0, fx1, fy1 with y growing
/// from the top of the quad toward the bottom. `uv` is what the editor passes
/// to a textured quad: u0, v0, u1, v1 as fractions of the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlicePiece {
    pub span: [f32; 4],
    pub uv: [f32; 4],
}

/// Tile the edges and centre of a nine-slice. `border` is left, right, top,
/// bottom in picture pixels. `frame` is u0, v0, u1, v1. `picture` is the
/// texture size in pixels. `scale` is the sprite's world width and height.
pub fn tile_pieces(
    border: [f32; 4],
    frame: [f32; 4],
    picture: [f32; 2],
    scale: [f32; 2],
) -> Vec<SlicePiece> {
    let [l, rt, t, b] = border;
    let (first_x, last_x) = if frame[2] < frame[0] { (rt, l) } else { (l, rt) };
    let (first_y, last_y) = if frame[3] < frame[1] { (b, t) } else { (t, b) };
    let frame_w = (frame[2] - frame[0]).abs() * picture[0];
    let frame_h = (frame[3] - frame[1]).abs() * picture[1];
    let (px, tx) = nine_cuts(first_x, last_x, frame_w, scale[0]);
    let (py, ty) = nine_cuts(first_y, last_y, frame_h, scale[1]);
    let mut out = Vec::new();
    for j in 0..3 {
        for i in 0..3 {
            let (i0, i1) = (i, i + 1);
            let (j0, j1) = (j, j + 1);
            let w = (px[i1] - px[i0]).abs().max(1e-4);
            let h = (py[j1] - py[j0]).abs().max(1e-4);
            let repeats_x = if i0 == 1 && i1 == 2 {
                ((w * scale[0]) / first_x.max(1.0)).ceil().max(1.0) as usize
            } else {
                1
            };
            let repeats_y = if j0 == 1 && j1 == 2 {
                ((h * scale[1]) / first_y.max(1.0)).ceil().max(1.0) as usize
            } else {
                1
            };
            for ty_i in 0..repeats_y {
                for tx_i in 0..repeats_x {
                    let fx0 = px[i0] + (px[i1] - px[i0]) * tx_i as f32 / repeats_x as f32;
                    let fx1 = px[i0] + (px[i1] - px[i0]) * (tx_i + 1) as f32 / repeats_x as f32;
                    let fy0 = py[j0] + (py[j1] - py[j0]) * ty_i as f32 / repeats_y as f32;
                    let fy1 = py[j0] + (py[j1] - py[j0]) * (ty_i + 1) as f32 / repeats_y as f32;
                    let u0 = tx[i0] + (tx[i1] - tx[i0]) * tx_i as f32 / repeats_x as f32;
                    let u1 = tx[i0] + (tx[i1] - tx[i0]) * (tx_i + 1) as f32 / repeats_x as f32;
                    let v0 = ty[j0] + (ty[j1] - ty[j0]) * ty_i as f32 / repeats_y as f32;
                    let v1 = ty[j0] + (ty[j1] - ty[j0]) * (ty_i + 1) as f32 / repeats_y as f32;
                    out.push(SlicePiece { span: [fx0, fy0, fx1, fy1], uv: [u0, v0, u1, v1] });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damaged_sort_layers_drop_bad_pairs() {
        let rules = parse_sort_layers("bad, 2:what, 1:y_sort, -3:axis");
        assert_eq!(rules.get(&1), Some(&LayerSortMode::YSort));
        assert!(!rules.contains_key(&2));
        assert_eq!(parse_sort_axis("nope"), [0.0, 1.0]);
        let broken =
            tile_pieces([-8.0, -8.0, -8.0, -8.0], [0.0, 0.0, 0.0, 0.0], [0.0, 0.0], [0.0, 0.0]);
        assert_eq!(broken.len(), 9, "a zero picture still yields the nine pieces");
    }

    #[test]
    fn tile_mode_at_native_scale_is_nine_quads() {
        let pieces =
            tile_pieces([8.0, 8.0, 8.0, 8.0], [0.0, 0.0, 1.0, 1.0], [32.0, 32.0], [1.0, 1.0]);
        assert_eq!(pieces.len(), 9);
        assert_eq!(pieces[0].uv, [0.0, 0.0, 0.25, 0.25]);
        let centre = pieces.iter().find(|p| p.uv[0] == 0.25 && p.uv[1] == 0.25).unwrap();
        assert_eq!(centre.uv, [0.25, 0.25, 0.75, 0.75]);
    }

    #[test]
    fn pivot_rect_puts_the_pivot_on_the_origin() {
        let [x0, y0, x1, y1] = pivot_rect([4.0, 2.0], [0.0, 0.0]);
        assert_eq!([x0, y0, x1, y1], [0.0, 0.0, 4.0, 2.0]);
        let [x0, y0, _, _] = pivot_rect([4.0, 2.0], [0.5, 1.0]);
        assert_eq!([x0, y0], [-2.0, -2.0]);
    }
}
