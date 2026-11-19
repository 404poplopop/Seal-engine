//! Navigation grid with A* pathfinding (16b core). Pure and deterministic:
//! ties are broken by cell index, so the same grid gives the same path.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// A grid of walkable and blocked cells. Cell (x, y) maps to world
/// `origin + (x, y) * cell`; cell centres are used for path points.
#[derive(Clone, Debug, PartialEq)]
pub struct NavGrid {
    pub width: usize,
    pub height: usize,
    pub cell: f32,
    pub origin: (f32, f32),
    blocked: Vec<bool>,
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct Node {
    f: u32,
    g: u32,
    idx: usize,
}
impl Ord for Node {
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.cmp(&self.f).then(o.idx.cmp(&self.idx))
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl NavGrid {
    pub fn new(width: usize, height: usize, cell: f32, origin: (f32, f32)) -> NavGrid {
        NavGrid {
            width,
            height,
            cell: cell.max(1e-3),
            origin,
            blocked: vec![false; width * height],
        }
    }

    pub fn set_blocked(&mut self, x: usize, y: usize, blocked: bool) {
        if x < self.width && y < self.height {
            self.blocked[y * self.width + x] = blocked;
        }
    }

    pub fn is_walkable(&self, x: i64, y: i64) -> bool {
        x >= 0
            && y >= 0
            && (x as usize) < self.width
            && (y as usize) < self.height
            && !self.blocked[y as usize * self.width + x as usize]
    }

    /// Cell containing a world point, or None outside the grid.
    pub fn cell_of(&self, p: (f32, f32)) -> Option<(usize, usize)> {
        let fx = ((p.0 - self.origin.0) / self.cell).floor();
        let fy = ((p.1 - self.origin.1) / self.cell).floor();
        if fx < 0.0 || fy < 0.0 || fx >= self.width as f32 || fy >= self.height as f32 {
            return None;
        }
        Some((fx as usize, fy as usize))
    }

    pub fn centre_of(&self, c: (usize, usize)) -> (f32, f32) {
        (
            self.origin.0 + (c.0 as f32 + 0.5) * self.cell,
            self.origin.1 + (c.1 as f32 + 0.5) * self.cell,
        )
    }

    /// Shortest path of cell centres from `from` to `to` (both included),
    /// 8-way movement without cutting blocked corners. None if either end is
    /// outside or blocked, or no route exists.
    pub fn find_path(&self, from: (f32, f32), to: (f32, f32)) -> Option<Vec<(f32, f32)>> {
        let s = self.cell_of(from)?;
        let t = self.cell_of(to)?;
        if !self.is_walkable(s.0 as i64, s.1 as i64) || !self.is_walkable(t.0 as i64, t.1 as i64) {
            return None;
        }
        let w = self.width;
        let (si, ti) = (s.1 * w + s.0, t.1 * w + t.0);
        let h = |i: usize| -> u32 {
            let (x, y) = ((i % w) as i64, (i / w) as i64);
            let (dx, dy) =
                ((x - t.0 as i64).unsigned_abs() as u32, (y - t.1 as i64).unsigned_abs() as u32);
            // octile distance, straight 10, diagonal 14
            10 * dx.max(dy) - 6 * dx.min(dy)
        };
        let mut best = vec![u32::MAX; self.blocked.len()];
        let mut prev = vec![usize::MAX; self.blocked.len()];
        let mut heap = BinaryHeap::new();
        best[si] = 0;
        heap.push(Node { f: h(si), g: 0, idx: si });
        while let Some(Node { g, idx, .. }) = heap.pop() {
            if idx == ti {
                let mut out = vec![];
                let mut i = ti;
                while i != usize::MAX {
                    out.push(self.centre_of((i % w, i / w)));
                    i = prev[i];
                }
                out.reverse();
                return Some(out);
            }
            if g > best[idx] {
                continue;
            }
            let (x, y) = ((idx % w) as i64, (idx / w) as i64);
            for (dx, dy) in
                [(0i64, -1i64), (-1, 0), (1, 0), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)]
            {
                let (nx, ny) = (x + dx, y + dy);
                if !self.is_walkable(nx, ny) {
                    continue;
                }
                let diag = dx != 0 && dy != 0;
                if diag && (!self.is_walkable(x + dx, y) || !self.is_walkable(x, y + dy)) {
                    continue;
                }
                let ni = ny as usize * w + nx as usize;
                let ng = g + if diag { 14 } else { 10 };
                if ng < best[ni] {
                    best[ni] = ng;
                    prev[ni] = idx;
                    heap.push(Node { f: ng + h(ni), g: ng, idx: ni });
                }
            }
        }
        None
    }
}
