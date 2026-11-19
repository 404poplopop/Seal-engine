//! Viewport marquee, lasso, and pick-through helpers (unit-tested).

/// View modes the viewport shade menu enables.
pub const SHADE_MODES: &[&str] = &[
    "lit",
    "unlit",
    "wireframe",
    "collision",
    "lighting_only",
    "overdraw",
    "base_colour",
    "normals",
    "depth",
];

pub fn shade_mode_allowed(mode: &str) -> bool {
    SHADE_MODES.contains(&mode)
}

use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkRect {
    pub x: f32,
    pub y: f32,
    pub half_w: f32,
    pub half_h: f32,
    pub depth: f32,
    pub pickable: bool,
}

impl MarkRect {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        (px - self.x).abs() <= self.half_w && (py - self.y).abs() <= self.half_h
    }

    pub fn intersects_box(&self, min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> bool {
        let (l, r) = (self.x - self.half_w, self.x + self.half_w);
        let (b, t) = (self.y - self.half_h, self.y + self.half_h);
        l <= max_x && r >= min_x && b <= max_y && t >= min_y
    }
}

/// Indices of pickable marks whose bounds intersect the axis-aligned box.
pub fn marquee_indices(
    marks: &[MarkRect],
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
) -> Vec<usize> {
    marks
        .iter()
        .enumerate()
        .filter(|(_, m)| m.pickable && m.intersects_box(min_x, min_y, max_x, max_y))
        .map(|(i, _)| i)
        .collect()
}

fn point_in_polygon(px: f32, py: f32, poly: &[[f32; 2]]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = (poly[i][0], poly[i][1]);
        let (xj, yj) = (poly[j][0], poly[j][1]);
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi).max(1e-9) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Indices of pickable marks whose center lies inside the closed lasso polygon.
pub fn lasso_indices(marks: &[MarkRect], polygon: &[[f32; 2]]) -> Vec<usize> {
    if polygon.len() < 3 {
        return Vec::new();
    }
    marks
        .iter()
        .enumerate()
        .filter(|(_, m)| m.pickable && point_in_polygon(m.x, m.y, polygon))
        .map(|(i, _)| i)
        .collect()
}

/// Front-to-back stack at one scene point (smaller depth draws in front).
pub fn pick_stack_at(marks: &[MarkRect], px: f32, py: f32) -> Vec<usize> {
    let mut hits: Vec<(usize, f32, f32)> = marks
        .iter()
        .enumerate()
        .filter(|(_, m)| m.pickable && m.contains(px, py))
        .map(|(i, m)| (i, m.depth, m.half_w * m.half_h))
        .collect();
    hits.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal))
    });
    hits.into_iter().map(|(i, _, _)| i).collect()
}

/// Next index when the user clicks the same spot again (pick through).
pub fn pick_through_next(stack: &[usize], current: Option<usize>) -> Option<usize> {
    if stack.is_empty() {
        return None;
    }
    if stack.len() == 1 {
        return Some(stack[0]);
    }
    let Some(cur) = current else {
        return Some(stack[0]);
    };
    let pos = stack.iter().position(|&i| i == cur).unwrap_or(0);
    Some(stack[(pos + 1) % stack.len()])
}

/// Drop indices the user marked unselectable (entity ids matching `blocked`).
pub fn filter_selectable_indices(indices: &[usize], ids: &[&str], blocked: &[&str]) -> Vec<usize> {
    indices
        .iter()
        .copied()
        .filter(|&i| ids.get(i).is_some_and(|id| !blocked.contains(id)))
        .collect()
}

/// Heat colour for overdraw view: blue (1 layer) toward red (8+).
pub fn overdraw_tint(count: usize) -> [u8; 3] {
    let t = ((count.saturating_sub(1)) as f32 / 7.0).clamp(0.0, 1.0);
    let r = (t * 255.0) as u8;
    let b = ((1.0 - t) * 255.0) as u8;
    [r, 48, b]
}

/// Per-mark overlap count for overdraw shading (how many other rects cover its center).
pub fn overdraw_counts(marks: &[MarkRect]) -> Vec<usize> {
    marks
        .iter()
        .enumerate()
        .map(|(i, m)| {
            marks
                .iter()
                .enumerate()
                .filter(|(j, other)| *j != i && other.pickable && other.contains(m.x, m.y))
                .count()
                + 1
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_marks() -> Vec<MarkRect> {
        vec![
            MarkRect { x: 0.0, y: 0.0, half_w: 1.0, half_h: 1.0, depth: 0.0, pickable: true },
            MarkRect { x: 0.5, y: 0.0, half_w: 1.0, half_h: 1.0, depth: 1.0, pickable: true },
            MarkRect { x: 5.0, y: 5.0, half_w: 0.5, half_h: 0.5, depth: 0.0, pickable: false },
        ]
    }

    #[test]
    fn marquee_hits_overlapping_boxes() {
        let marks = sample_marks();
        let empty = marquee_indices(&marks, 10.0, 10.0, 12.0, 12.0);
        assert_eq!(empty, Vec::<usize>::new());
        let both = marquee_indices(&marks, -2.0, -1.0, 1.5, 1.0);
        assert_eq!(both, vec![0, 1]);
    }

    #[test]
    fn lasso_includes_center_inside_polygon() {
        let marks = sample_marks();
        let poly = [[-2.0, -2.0], [2.0, -2.0], [2.0, 2.0], [-2.0, 2.0]];
        assert_eq!(lasso_indices(&marks, &poly), vec![0, 1]);
        let small = [[4.0, 4.0], [6.0, 4.0], [6.0, 6.0]];
        assert!(lasso_indices(&marks, &small).is_empty());
    }

    #[test]
    fn lasso_rejects_open_polyline() {
        let marks = sample_marks();
        assert!(lasso_indices(&marks, &[[0.0, 0.0], [1.0, 1.0]]).is_empty());
    }

    #[test]
    fn pick_through_cycles_depth_order() {
        let marks = sample_marks();
        let stack = pick_stack_at(&marks, 0.25, 0.0);
        assert_eq!(stack, vec![0, 1]);
        assert_eq!(pick_through_next(&stack, None), Some(0));
        assert_eq!(pick_through_next(&stack, Some(0)), Some(1));
        assert_eq!(pick_through_next(&stack, Some(1)), Some(0));
    }

    #[test]
    fn unselectable_skipped_from_selection() {
        let ids = ["a", "b", "c"];
        let blocked = ["b"];
        assert_eq!(filter_selectable_indices(&[0, 1, 2], &ids, &blocked), vec![0, 2]);
    }

    #[test]
    fn overdraw_counts_overlap_at_center() {
        let marks = sample_marks();
        let counts = overdraw_counts(&marks);
        assert!(counts[0] >= 2);
        assert!(counts[1] >= 2);
    }

    #[test]
    fn overdraw_tint_warms_with_depth() {
        let cool = overdraw_tint(1);
        let hot = overdraw_tint(8);
        assert!(hot[0] > cool[0]);
        assert!(hot[2] < cool[2]);
    }
}
