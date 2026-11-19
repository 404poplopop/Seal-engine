//! Sprite-sheet cuts: equal cells, separate pictures found by their opaque
//! pixels, and the outlines of one cell.
//!
//! Coordinates on an outline run from 0 to 1 across the cell, with y pointing up.

use crate::frames::Frame;

/// Every cell of a sheet split into `columns` by `rows`, left to right, then top to bottom.
pub fn cell_count_frames(image: &str, columns: u32, rows: u32) -> Vec<Frame> {
    let (columns, rows) = (columns.max(1), rows.max(1));
    crate::frames::grid_frames(image, columns, rows, 0, columns.saturating_mul(rows))
}

/// One picture found by walking opaque pixels, then padded and clipped to the sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Island {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

fn opaque(rgba: &[u8], width: usize, x: usize, y: usize) -> bool {
    rgba[(y * width + x) * 4 + 3] > 8
}

/// Frames around each separate opaque picture.
///
/// `min_size` drops a picture whose width or height is smaller than that many
/// pixels. `padding` grows each picture by that many pixels on every side,
/// stopping at the edge of the sheet. Pictures are ordered top to bottom,
/// then left to right.
pub fn island_frames(
    image: &str,
    rgba: &[u8],
    width: u32,
    height: u32,
    min_size: u32,
    padding: u32,
) -> Vec<Frame> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return Vec::new();
    }
    let mut seen = vec![false; w * h];
    let mut islands = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if seen[i] || !opaque(rgba, w, x, y) {
                continue;
            }
            let mut stack = vec![(x, y)];
            seen[i] = true;
            let (mut x0, mut y0, mut x1, mut y1) = (x, y, x, y);
            while let Some((cx, cy)) = stack.pop() {
                x0 = x0.min(cx);
                y0 = y0.min(cy);
                x1 = x1.max(cx);
                y1 = y1.max(cy);
                for (nx, ny) in
                    [(cx + 1, cy), (cx.wrapping_sub(1), cy), (cx, cy + 1), (cx, cy.wrapping_sub(1))]
                {
                    if nx >= w || ny >= h {
                        continue;
                    }
                    let ni = ny * w + nx;
                    if seen[ni] || !opaque(rgba, w, nx, ny) {
                        continue;
                    }
                    seen[ni] = true;
                    stack.push((nx, ny));
                }
            }
            let (iw, ih) = (x1 - x0 + 1, y1 - y0 + 1);
            if iw < min_size.max(1) as usize || ih < min_size.max(1) as usize {
                continue;
            }
            let pad = padding as i32;
            islands.push(Island {
                x0: x0 as i32 - pad,
                y0: y0 as i32 - pad,
                x1: x1 as i32 + 1 + pad,
                y1: y1 as i32 + 1 + pad,
            });
        }
    }
    islands.sort_by(|a, b| a.y0.cmp(&b.y0).then(a.x0.cmp(&b.x0)));
    islands
        .into_iter()
        .map(|island| {
            let x0 = island.x0.clamp(0, w as i32) as f32;
            let y0 = island.y0.clamp(0, h as i32) as f32;
            let x1 = island.x1.clamp(0, w as i32) as f32;
            let y1 = island.y1.clamp(0, h as i32) as f32;
            Frame::cell(image, [x0 / w as f32, y0 / h as f32, x1 / w as f32, y1 / h as f32])
        })
        .filter(|frame| frame.uv[2] > frame.uv[0] && frame.uv[3] > frame.uv[1])
        .collect()
}

fn cell_rect(uv: [f32; 4], width: u32, height: u32) -> (i32, i32, i32, i32) {
    let x0 = (uv[0] * width as f32).floor() as i32;
    let y0 = (uv[1] * height as f32).floor() as i32;
    let x1 = (uv[2] * width as f32).ceil() as i32;
    let y1 = (uv[3] * height as f32).ceil() as i32;
    (x0, y0, (x1 - x0).max(1), (y1 - y0).max(1))
}

/// Tight outline of the opaque pixels in `uv`, in cell space (y up).
///
/// A cell with no opaque pixels returns the cell's four corners.
pub fn tight_outline(rgba: &[u8], width: u32, height: u32, uv: [f32; 4]) -> Vec<[f32; 2]> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return unit_quad();
    }
    let (x0, y0, cw, ch) = cell_rect(uv, width, height);
    let mut pixels = Vec::new();
    for y in 0..ch {
        for x in 0..cw {
            let (px, py) = (x0 + x, y0 + y);
            if px < 0 || py < 0 || px as usize >= w || py as usize >= h {
                continue;
            }
            if opaque(rgba, w, px as usize, py as usize) {
                pixels.push((px, py));
            }
        }
    }
    if pixels.is_empty() {
        return unit_quad();
    }
    let contour = trace(&pixels).unwrap_or_else(|| pixels.clone());
    let simplified = simplify(&contour, 1.25);
    let mut out: Vec<[f32; 2]> = simplified
        .into_iter()
        .map(|(px, py)| {
            let u = (px as f32 - x0 as f32) / cw as f32;
            let v = 1.0 - (py as f32 - y0 as f32) / ch as f32;
            [u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)]
        })
        .collect();
    if out.len() >= 2 && dist2(out[0], *out.last().unwrap()) < 1.0e-8 {
        out.pop();
    }
    if out.len() < 3 {
        unit_quad()
    } else {
        out
    }
}

/// Convex outline used as a collider. Built from `tight` when that is present.
pub fn physics_outline(tight: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let hull = convex_hull(tight);
    if hull.len() >= 3 {
        hull
    } else {
        unit_quad()
    }
}

fn unit_quad() -> Vec<[f32; 2]> {
    vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    dx * dx + dy * dy
}

/// Walk the boundary of a set of pixels. Points are pixel corners in image space (y down).
fn trace(pixels: &[(i32, i32)]) -> Option<Vec<(i32, i32)>> {
    let set: std::collections::BTreeSet<(i32, i32)> = pixels.iter().copied().collect();
    let start = *pixels.iter().min()?;
    // Edge walk: from each boundary corner, step to the next corner that keeps
    // the opaque pixel on the left of the directed edge (image y down).
    let mut edges = Vec::new();
    for &(x, y) in pixels {
        let filled = |dx: i32, dy: i32| set.contains(&(x + dx, y + dy));
        // Four edges of this pixel, as (from, to), when the outside is on the right.
        let candidates = [
            ((x, y), (x + 1, y), !filled(0, -1)),
            ((x + 1, y), (x + 1, y + 1), !filled(1, 0)),
            ((x + 1, y + 1), (x, y + 1), !filled(0, 1)),
            ((x, y + 1), (x, y), !filled(-1, 0)),
        ];
        for (a, b, boundary) in candidates {
            if boundary {
                edges.push((a, b));
            }
        }
    }
    if edges.is_empty() {
        return None;
    }
    let mut next: std::collections::BTreeMap<(i32, i32), (i32, i32)> =
        std::collections::BTreeMap::new();
    for (a, b) in edges {
        next.entry(a).or_insert(b);
    }
    let mut ring = vec![start];
    let mut at = *next.get(&start)?;
    let mut guard = 0;
    while at != start && guard < next.len() + 2 {
        ring.push(at);
        at = *next.get(&at)?;
        guard += 1;
    }
    if ring.len() < 3 {
        None
    } else {
        Some(ring)
    }
}

fn simplify(points: &[(i32, i32)], epsilon: f32) -> Vec<(i32, i32)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (ax, ay) = (points[a].0 as f32, points[a].1 as f32);
        let (bx, by) = (points[b].0 as f32, points[b].1 as f32);
        let (mut far, mut far_i) = (0.0f32, a);
        for i in a + 1..b {
            let (px, py) = (points[i].0 as f32, points[i].1 as f32);
            let d = point_line(px, py, ax, ay, bx, by);
            if d > far {
                far = d;
                far_i = i;
            }
        }
        if far > epsilon && far_i != a {
            keep[far_i] = true;
            stack.push((a, far_i));
            stack.push((far_i, b));
        }
    }
    points.iter().enumerate().filter(|(i, _)| keep[*i]).map(|(_, p)| *p).collect()
}

fn point_line(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 1.0e-6 {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    ((px - ax) * dy - (py - ay) * dx).abs() / len
}

/// Convex hull, counter-clockwise, y up in the same space as `points`.
pub fn convex_hull(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut pts: Vec<[f32; 2]> =
        points.iter().copied().filter(|p| p[0].is_finite() && p[1].is_finite()).collect();
    pts.sort_by(|a, b| {
        a[0].partial_cmp(&b[0])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a[1].partial_cmp(&b[1]).unwrap_or(std::cmp::Ordering::Equal))
    });
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1.0e-5 && (a[1] - b[1]).abs() < 1.0e-5);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut lower = Vec::new();
    for p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= 0.0 {
            lower.pop();
        }
        lower.push(*p);
    }
    let mut upper = Vec::new();
    for p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= 0.0 {
            upper.pop();
        }
        upper.push(*p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Collider points in the object's local units, from a frame's physics outline
/// (or its tight outline, or the whole cell). The pivot sits on the object.
pub fn collider_points(
    pivot: [f32; 2],
    physics: &[[f32; 2]],
    outline: &[[f32; 2]],
) -> Vec<[f32; 2]> {
    let src: Vec<[f32; 2]> = if physics.len() >= 3 {
        physics.to_vec()
    } else if outline.len() >= 3 {
        physics_outline(outline)
    } else {
        unit_quad()
    };
    src.into_iter().map(|p| [p[0] - pivot[0], p[1] - pivot[1]]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint(w: usize, h: usize, pixels: &[(usize, usize)]) -> Vec<u8> {
        let mut rgba = vec![0u8; w * h * 4];
        for (x, y) in pixels {
            let i = (y * w + x) * 4;
            rgba[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
        }
        rgba
    }

    #[test]
    fn cell_count_splits_the_sheet_into_equal_cells() {
        let frames = cell_count_frames("sheet.png", 4, 2);
        assert_eq!(frames.len(), 8);
        assert_eq!(frames[0].uv, [0.0, 0.0, 0.25, 0.5]);
        assert_eq!(frames[5].uv, [0.25, 0.5, 0.5, 1.0]);
        assert!(frames.iter().all(|f| f.pivot == [0.5, 0.5] && f.border == [0.0; 4]));
    }

    #[test]
    fn islands_follow_opaque_pixels_and_honour_min_size_and_padding() {
        // Two blobs, plus a speck that min size drops. They are not in separate
        // clear rows, so a row-gap cut would not find them.
        let (w, h) = (12usize, 8usize);
        let mut px = Vec::new();
        for y in 1..4 {
            for x in 1..4 {
                px.push((x, y));
            }
        }
        for y in 1..3 {
            for x in 8..11 {
                px.push((x, y));
            }
        }
        px.push((6, 6));
        let rgba = paint(w, h, &px);
        let frames = island_frames("s.png", &rgba, w as u32, h as u32, 2, 1);
        assert_eq!(frames.len(), 2, "the speck is below the minimum size");
        // First blob is x 1..4, y 1..4, padded by 1 -> x 0..5, y 0..5.
        assert_eq!(frames[0].uv, [0.0, 0.0, 5.0 / 12.0, 5.0 / 8.0]);
        assert!(island_frames("s.png", &[], 0, 0, 1, 0).is_empty());
    }

    #[test]
    fn outlines_follow_the_opaque_shape_and_the_physics_outline_is_convex() {
        let (w, h) = (8u32, 8u32);
        let mut px = Vec::new();
        // An L: full left two columns and the bottom two rows.
        for y in 0..8 {
            for x in 0..2 {
                px.push((x, y as usize));
            }
        }
        for y in 6..8 {
            for x in 2..8 {
                px.push((x, y));
            }
        }
        let rgba = paint(w as usize, h as usize, &px);
        let tight = tight_outline(&rgba, w, h, [0.0, 0.0, 1.0, 1.0]);
        assert!(tight.len() >= 4, "{tight:?}");
        let area = seal_graph_area(&tight);
        // The L covers 16 + 12 = 28 of 64 pixels, about 0.44 of the cell.
        // The bounding box would be 1. The outline stays nearer the L.
        assert!(area > 0.3 && area < 0.7, "tight area {area} points {tight:?}");
        let hull = physics_outline(&tight);
        assert!(hull.len() >= 3);
        let hull_area = seal_graph_area(&hull);
        assert!(hull_area + 1.0e-3 >= area, "physics outline covers the tight outline");
    }

    fn seal_graph_area(pts: &[[f32; 2]]) -> f32 {
        let mut sum = 0.0;
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            sum += a[0] * b[1] - b[0] * a[1];
        }
        (sum * 0.5).abs()
    }

    #[test]
    fn collider_points_sit_on_the_pivot() {
        let physics = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let pts = collider_points([0.25, 0.75], &physics, &[]);
        assert_eq!(pts[0], [-0.25, -0.75]);
        assert_eq!(pts[2], [0.75, 0.25]);
    }
}
