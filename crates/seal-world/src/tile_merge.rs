//! Solid tiles that touch become a few rectangles instead of one collider each.
//!
//! Rows of the same height merge first, then stacked runs with the same left
//! and right edges merge. A single row comes out the same as merging that row
//! on its own. The rectangles cover the tiles and do not overlap each other.

/// One rectangle covering a run of solid tiles. `size` is the full width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolidRect {
    pub center: [f32; 2],
    pub size: [f32; 2],
}

/// Merge `tiles` (`center`, full `size`) into as few rectangles as the rows allow.
pub fn merge_tiles(tiles: &[([f32; 2], [f32; 2])]) -> Vec<SolidRect> {
    let mut tiles = tiles.to_vec();
    tiles.sort_by(|a, b| a.0[1].total_cmp(&b.0[1]).then(a.0[0].total_cmp(&b.0[0])));
    let mut rows: Vec<(f32, f32, f32, f32)> = Vec::new();
    for (center, size) in &tiles {
        let (left, right) = (center[0] - size[0] * 0.5, center[0] + size[0] * 0.5);
        if let Some(last) = rows.last_mut() {
            if (last.2 - center[1]).abs() < 1e-3
                && (last.3 - size[1]).abs() < 1e-3
                && left <= last.1 + 1e-3
            {
                last.1 = last.1.max(right);
                continue;
            }
        }
        rows.push((left, right, center[1], size[1]));
    }
    let mut blocks: Vec<(f32, f32, f32, f32)> = rows
        .into_iter()
        .map(|(left, right, y, h)| (left, right, y - h * 0.5, y + h * 0.5))
        .collect();
    blocks.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.total_cmp(&b.2)));
    let mut merged: Vec<(f32, f32, f32, f32)> = Vec::new();
    for block in blocks {
        if let Some(last) = merged.last_mut() {
            if (last.0 - block.0).abs() < 1e-3
                && (last.1 - block.1).abs() < 1e-3
                && block.2 <= last.3 + 1e-3
            {
                last.3 = last.3.max(block.3);
                continue;
            }
        }
        merged.push(block);
    }
    merged
        .into_iter()
        .map(|(left, right, bottom, top)| SolidRect {
            center: [(left + right) * 0.5, (bottom + top) * 0.5],
            size: [right - left, top - bottom],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn cell(x: f32, y: f32) -> ([f32; 2], [f32; 2]) {
        ([x, y], [1.0, 1.0])
    }

    fn covered(rects: &[SolidRect]) -> BTreeSet<(i32, i32)> {
        let mut cells = BTreeSet::new();
        for rect in rects {
            let left = (rect.center[0] - rect.size[0] * 0.5).round() as i32;
            let right = (rect.center[0] + rect.size[0] * 0.5).round() as i32;
            let bottom = (rect.center[1] - rect.size[1] * 0.5).round() as i32;
            let top = (rect.center[1] + rect.size[1] * 0.5).round() as i32;
            for y in bottom..top {
                for x in left..right {
                    assert!(cells.insert((x, y)), "rectangles overlap at {x},{y}");
                }
            }
        }
        cells
    }

    fn expect(tiles: &[([f32; 2], [f32; 2])]) -> BTreeSet<(i32, i32)> {
        tiles
            .iter()
            .map(|(c, _)| ((c[0] - 0.5).round() as i32, (c[1] - 0.5).round() as i32))
            .collect()
    }

    #[test]
    fn a_block_becomes_one_rectangle_and_a_gap_stays_split() {
        let block = [cell(0.5, 0.5), cell(1.5, 0.5), cell(0.5, 1.5), cell(1.5, 1.5)];
        let rects = merge_tiles(&block);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].size, [2.0, 2.0]);
        assert_eq!(covered(&rects), expect(&block));
        let gap = [cell(0.5, 0.5), cell(0.5, 2.5)];
        let rects = merge_tiles(&gap);
        assert_eq!(rects.len(), 2);
        assert_eq!(covered(&rects), expect(&gap));
    }

    #[test]
    fn an_l_and_a_single_row_cover_exactly_those_cells() {
        let row = [cell(0.5, 0.5), cell(1.5, 0.5), cell(2.5, 0.5)];
        let rects = merge_tiles(&row);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].size, [3.0, 1.0]);
        assert_eq!(covered(&rects), expect(&row));
        let l_shape = [cell(0.5, 0.5), cell(1.5, 0.5), cell(0.5, 1.5)];
        let rects = merge_tiles(&l_shape);
        assert_eq!(rects.len(), 2);
        assert_eq!(covered(&rects), expect(&l_shape));
    }
}
