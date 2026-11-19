//! Polygon and edge collider points: drag, insert, delete, and a sprite outline.
//!
//! Points are in the object's own units. A picture that fills the sprite
//! (from -0.5 to 0.5 on each axis) traces into that same square.

use crate::Entity;

fn scale_of(v: f32) -> f32 {
    if v.abs() < 1e-6 {
        1.0
    } else {
        v
    }
}

/// World position of a local outline point.
pub fn to_world(entity: &Entity, local: [f32; 2]) -> [f32; 2] {
    let (s, c) = entity.rotation[2].to_radians().sin_cos();
    let x = local[0] * entity.scale[0];
    let y = local[1] * entity.scale[1];
    [entity.translation[0] + c * x - s * y, entity.translation[1] + s * x + c * y]
}

/// Local outline point under a world position.
pub fn to_local(entity: &Entity, world: [f32; 2]) -> [f32; 2] {
    let (s, c) = entity.rotation[2].to_radians().sin_cos();
    let dx = world[0] - entity.translation[0];
    let dy = world[1] - entity.translation[1];
    let lx = c * dx + s * dy;
    let ly = -s * dx + c * dy;
    [lx / scale_of(entity.scale[0]), ly / scale_of(entity.scale[1])]
}

fn editable(entity: &Entity) -> bool {
    matches!(entity.phys2d.shape.as_str(), "polygon" | "edge")
        && (!entity.phys2d.body.is_empty() || entity.phys2d.part)
}

fn min_points(shape: &str) -> usize {
    if shape == "edge" {
        2
    } else {
        3
    }
}

/// Move point `index` to the world position. False when it cannot move.
pub fn move_point(entity: &mut Entity, index: usize, world: [f32; 2]) -> bool {
    if !editable(entity) || index >= entity.phys2d.points.len() {
        return false;
    }
    let local = to_local(entity, world);
    if !local[0].is_finite() || !local[1].is_finite() {
        return false;
    }
    if (entity.phys2d.points[index][0] - local[0]).abs() < 1e-5
        && (entity.phys2d.points[index][1] - local[1]).abs() < 1e-5
    {
        return false;
    }
    entity.phys2d.points[index] = local;
    true
}

/// Insert a point after `index` (the start of an edge) at the world position.
pub fn insert_point(entity: &mut Entity, index: usize, world: [f32; 2]) -> bool {
    if !editable(entity) || entity.phys2d.points.len() < 2 || index >= entity.phys2d.points.len() {
        return false;
    }
    let local = to_local(entity, world);
    if !local[0].is_finite() || !local[1].is_finite() {
        return false;
    }
    entity.phys2d.points.insert(index + 1, local);
    true
}

/// Remove point `index`. A polygon keeps at least 3 points, an edge at least 2.
pub fn delete_point(entity: &mut Entity, index: usize) -> bool {
    if !editable(entity) || index >= entity.phys2d.points.len() {
        return false;
    }
    if entity.phys2d.points.len() <= min_points(&entity.phys2d.shape) {
        return false;
    }
    entity.phys2d.points.remove(index);
    true
}

/// Trace opaque pixels into a polygon in the sprite's own square, from -0.5 to 0.5.
/// `pixels` is packed red, green, blue, alpha. Y in the buffer grows downward.
pub fn trace_alpha(pixels: &[u8], width: u32, height: u32) -> Result<Vec<[f32; 2]>, String> {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 || pixels.len() != w * h * 4 {
        return Err("the picture is empty or damaged".into());
    }
    let solid = |x: i32, y: i32| -> bool {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            return false;
        }
        pixels[((y as usize) * w + x as usize) * 4 + 3] >= 128
    };
    let mut start = None;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if solid(x, y) {
                start = Some((x, y));
                break;
            }
        }
        if start.is_some() {
            break;
        }
    }
    let Some((sx, sy)) = start else {
        return Err("the picture has no opaque pixels".into());
    };
    // 8-neighbour walk, starting from the empty cell to the left of the first pixel.
    let dirs = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];
    let mut dir = 4; // came from the left, so the next search starts after that
    let mut x = sx;
    let mut y = sy;
    let mut contour = Vec::new();
    for _ in 0..(w * h * 2).max(8) {
        contour.push([x as f32, y as f32]);
        let mut found = None;
        for step in 0..8 {
            let i = (dir + 1 + step) % 8;
            let (nx, ny) = (x + dirs[i].0, y + dirs[i].1);
            if solid(nx, ny) {
                found = Some((nx, ny, i));
                break;
            }
        }
        let Some((nx, ny, i)) = found else {
            break;
        };
        x = nx;
        y = ny;
        dir = (i + 4) % 8;
        if x == sx && y == sy {
            break;
        }
    }
    if contour.len() < 3 {
        return Err("the outline needs at least 3 points".into());
    }
    let simplified = simplify(&contour, 1.2);
    if simplified.len() < 3 {
        return Err("the outline needs at least 3 points".into());
    }
    let points = simplified
        .into_iter()
        .map(|p| [(p[0] + 0.5) / w as f32 - 0.5, 0.5 - (p[1] + 0.5) / h as f32])
        .collect();
    Ok(points)
}

fn simplify(points: &[[f32; 2]], epsilon: f32) -> Vec<[f32; 2]> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let mut far = 0.0;
        let mut at = 0;
        for i in a + 1..b {
            let d = point_line(points[i], points[a], points[b]);
            if d > far {
                far = d;
                at = i;
            }
        }
        if far > epsilon && at > a {
            keep[at] = true;
            stack.push((a, at));
            stack.push((at, b));
        }
    }
    points.iter().enumerate().filter(|(i, _)| keep[*i]).map(|(_, p)| *p).collect()
}

fn point_line(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len = (ab[0] * ab[0] + ab[1] * ab[1]).sqrt();
    if len < 1e-6 {
        let d = [p[0] - a[0], p[1] - a[1]];
        return (d[0] * d[0] + d[1] * d[1]).sqrt();
    }
    ((p[0] - a[0]) * ab[1] - (p[1] - a[1]) * ab[0]).abs() / len
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: usize, h: usize) -> Vec<u8> {
        vec![255; w * h * 4]
    }

    #[test]
    fn a_solid_rectangle_traces_to_four_corners() {
        let points = trace_alpha(&solid(8, 8), 8, 8).unwrap();
        assert!(points.len() >= 4 && points.len() <= 8, "{points:?}");
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (1.0f32, -1.0f32, 1.0f32, -1.0f32);
        for p in &points {
            min_x = min_x.min(p[0]);
            max_x = max_x.max(p[0]);
            min_y = min_y.min(p[1]);
            max_y = max_y.max(p[1]);
        }
        assert!(min_x < -0.3 && max_x > 0.3, "{points:?}");
        assert!(min_y < -0.3 && max_y > 0.3, "{points:?}");
    }

    #[test]
    fn a_damaged_buffer_and_a_blank_picture_are_refused() {
        assert!(trace_alpha(&[0, 0, 0, 255], 2, 2).is_err());
        assert!(trace_alpha(&vec![0; 16], 2, 2).is_err());
    }

    #[test]
    fn dragging_a_point_round_trips_through_a_turned_object() {
        let mut world =
            crate::World::from_text(&crate::World::empty_scene_text("Outline")).unwrap();
        let id = world.add("empty");
        {
            let entity = world.entity_mut(&id).unwrap();
            entity.phys2d.body = "fixed".into();
            entity.phys2d.shape = "polygon".into();
            entity.phys2d.points = vec![[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
            entity.translation = [2.0, 3.0, 0.0];
            entity.rotation[2] = 90.0;
            entity.scale = [2.0, 2.0, 1.0];
        }
        let entity = world.entity(&id).unwrap();
        let at = to_world(entity, entity.phys2d.points[0]);
        let back = to_local(entity, at);
        assert!((back[0] + 0.5).abs() < 1e-4 && (back[1] + 0.5).abs() < 1e-4, "{back:?}");
        let moved = to_world(entity, [0.25, 0.0]);
        let entity = world.entity_mut(&id).unwrap();
        assert!(move_point(entity, 1, moved));
        assert!((entity.phys2d.points[1][0] - 0.25).abs() < 1e-4);
        assert!(delete_point(entity, 2));
        assert!(!delete_point(entity, 0), "a polygon keeps three points");
    }
}
