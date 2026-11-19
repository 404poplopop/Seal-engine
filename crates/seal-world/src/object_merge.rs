//! Merge selected sprites into one batched group, or shapes and meshes into one mesh.
//!
//! The result is a single object. Sprite parts stay in image order so they draw as one
//! batch per picture. Mesh triangles are stored in the merged object's local space.
//! Undo is the caller's scene snapshot: merging only edits the entity list.

use crate::{Entity, Shape, World};

/// Sprites or triangles kept on one object after a merge. Empty `kind` means none.
#[derive(Clone, Debug, PartialEq)]
pub struct MergedDraw {
    /// `sprites` or `mesh`. Empty when this object is not a merge.
    pub kind: String,
    /// Packed parts. See [`encode_sprites`] and [`encode_tris`].
    pub body: String,
}

impl Default for MergedDraw {
    fn default() -> Self {
        Self { kind: String::new(), body: String::new() }
    }
}

seal_ecs::text_component!(MergedDraw as "merged_draw" { kind, body });

/// One sprite quad in world space, used to compare a merge with the objects it replaced.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteMark {
    pub image: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rgba: [u8; 4],
}

/// One triangle in world space, xy, with a flat colour.
#[derive(Clone, Debug, PartialEq)]
pub struct TriMark {
    pub points: [[f32; 2]; 3],
    pub rgb: [u8; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpritePart {
    pub image: String,
    pub local: [f32; 2],
    pub size: [f32; 2],
    pub tint: [f32; 4],
    pub layer: i32,
    pub order: i32,
}

#[derive(Clone, Debug)]
pub struct DrawnSprite {
    pub image: String,
    pub x: f32,
    pub y: f32,
    pub half: [f32; 2],
    pub tint: [f32; 4],
    pub layer: i32,
    pub order: i32,
}

#[derive(Clone, Debug)]
pub struct DrawnTri {
    pub points: [[f32; 2]; 3],
    pub color: [f32; 3],
}

/// The merged object draws its parts itself, so the plain body is not also drawn.
pub fn hides_body(entity: &Entity) -> bool {
    !entity.merged.kind.is_empty()
}

pub fn merge(world: &mut World, ids: &[String]) -> Result<String, String> {
    let mut picked: Vec<Entity> = Vec::new();
    for id in ids {
        if id == "root" {
            continue;
        }
        let Some(entity) = world.entity(id) else {
            return Err(format!("unknown object {id}"));
        };
        if entity.locked {
            return Err("a locked object cannot be merged".into());
        }
        if !mergeable(entity) {
            return Err("select sprites, shapes, or meshes".into());
        }
        picked.push(entity.clone());
    }
    if picked.len() < 2 {
        return Err("select two or more sprites, shapes, or meshes".into());
    }
    let sprites_only = picked.iter().all(is_sprite);
    let (kind, body, origin) = if sprites_only {
        let origin = average(picked.iter().map(|e| [e.translation[0], e.translation[1]]));
        let mut parts: Vec<SpritePart> = picked
            .iter()
            .map(|e| SpritePart {
                image: e.sprite.clone(),
                local: [e.translation[0] - origin[0], e.translation[1] - origin[1]],
                size: [e.scale[0].abs().max(0.05), e.scale[1].abs().max(0.05)],
                tint: e.tint,
                layer: e.layer,
                order: e.order,
            })
            .collect();
        parts.sort_by(|a, b| a.image.cmp(&b.image).then(a.order.cmp(&b.order)));
        ("sprites".to_string(), encode_sprites(&parts), origin)
    } else {
        let origin = average(picked.iter().map(|e| [e.translation[0], e.translation[1]]));
        let mut tris = Vec::new();
        for entity in &picked {
            let color = front_half(entity).map(|(_, color)| color).unwrap_or(entity.color);
            for points in local_tris(entity, origin) {
                tris.push((points, color));
            }
        }
        if tris.is_empty() {
            return Err("select sprites, shapes, or meshes".into());
        }
        ("mesh".to_string(), encode_tris(&tris), origin)
    };
    let parent = shared_parent(&picked);
    let name = world.free_name("Merged");
    let id = world.add("empty");
    if let Some(entity) = world.entity_mut(&id) {
        entity.name = name;
        entity.parent = parent;
        entity.translation = [origin[0], origin[1], 0.0];
        entity.shape = None;
        entity.sprite.clear();
        entity.tag.clear();
        entity.color = picked[0].color;
        entity.merged = MergedDraw { kind, body };
    }
    let doomed: Vec<String> = picked.iter().map(|e| e.id.clone()).collect();
    for entity in &mut world.entities {
        if entity.parent.as_ref().is_some_and(|p| doomed.contains(p)) && entity.id != id {
            entity.parent = Some(id.clone());
        }
    }
    world.entities.retain(|entity| !doomed.contains(&entity.id));
    Ok(id)
}

fn mergeable(entity: &Entity) -> bool {
    is_sprite(entity) || entity.shape.is_some() || shape_poly(entity) || entity.source_kind == "mesh"
}

fn is_sprite(entity: &Entity) -> bool {
    !entity.sprite.is_empty() && entity.shape.is_none() && !shape_poly(entity)
}

fn shape_poly(entity: &Entity) -> bool {
    !entity.shape2d.kind.is_empty() && entity.shape2d.points.len() >= 3
}

fn shared_parent(picked: &[Entity]) -> Option<String> {
    let first = picked[0].parent.clone();
    if picked.iter().all(|e| e.parent == first) {
        first
    } else {
        Some("root".into())
    }
}

fn average(points: impl Iterator<Item = [f32; 2]>) -> [f32; 2] {
    let mut n = 0.0;
    let mut sum = [0.0; 2];
    for p in points {
        sum[0] += p[0];
        sum[1] += p[1];
        n += 1.0;
    }
    if n == 0.0 {
        [0.0, 0.0]
    } else {
        [sum[0] / n, sum[1] / n]
    }
}

fn local_tris(entity: &Entity, origin: [f32; 2]) -> Vec<[[f32; 2]; 3]> {
    let mut out = Vec::new();
    if shape_poly(entity) {
        let pts: Vec<[f32; 2]> = entity
            .shape2d
            .points
            .iter()
            .map(|p| {
                [
                    entity.translation[0] + p[0] - origin[0],
                    entity.translation[1] + p[1] - origin[1],
                ]
            })
            .collect();
        if pts.len() >= 3 {
            for i in 1..pts.len() - 1 {
                out.push([pts[0], pts[i], pts[i + 1]]);
            }
        }
        return out;
    }
    if let Some((half, _)) = front_half(entity) {
        let c = [entity.translation[0] - origin[0], entity.translation[1] - origin[1]];
        let (hx, hy) = (half[0], half[1]);
        let a = [c[0] - hx, c[1] - hy];
        let b = [c[0] + hx, c[1] - hy];
        let d = [c[0] + hx, c[1] + hy];
        let e = [c[0] - hx, c[1] + hy];
        out.push([a, b, d]);
        out.push([a, d, e]);
    }
    out
}

fn front_half(entity: &Entity) -> Option<([f32; 2], [f32; 3])> {
    let color = entity.color;
    if let Some(shape) = entity.shape {
        let half = match shape {
            Shape::Sphere => {
                let r = 0.5 * entity.scale.iter().copied().map(f32::abs).fold(0.0, f32::max);
                [r, r]
            }
            Shape::Plane => [20.0 * entity.scale[0].abs(), 0.05 * entity.scale[1].abs()],
            _ => [0.5 * entity.scale[0].abs(), 0.5 * entity.scale[1].abs()],
        };
        return Some((half, color));
    }
    if entity.source_kind == "mesh" {
        return Some(([0.5 * entity.scale[0].abs().max(0.5), 0.5 * entity.scale[1].abs().max(0.5)], color));
    }
    if is_sprite(entity) {
        return Some(([entity.scale[0].abs().max(0.05) * 0.5, entity.scale[1].abs().max(0.05) * 0.5], [
            entity.tint[0],
            entity.tint[1],
            entity.tint[2],
        ]));
    }
    None
}

pub fn encode_sprites(parts: &[SpritePart]) -> String {
    parts
        .iter()
        .map(|p| {
            format!(
                "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                p.image.replace('|', "/"),
                num(p.local[0]),
                num(p.local[1]),
                num(p.size[0]),
                num(p.size[1]),
                num(p.tint[0]),
                num(p.tint[1]),
                num(p.tint[2]),
                num(p.tint[3]),
                p.layer,
                p.order
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

pub fn encode_tris(tris: &[([[f32; 2]; 3], [f32; 3])]) -> String {
    tris.iter()
        .map(|(t, color)| {
            format!(
                "{}|{}|{}|{}|{}|{}|{}|{}|{}",
                num(t[0][0]),
                num(t[0][1]),
                num(t[1][0]),
                num(t[1][1]),
                num(t[2][0]),
                num(t[2][1]),
                num(color[0]),
                num(color[1]),
                num(color[2])
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn num(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".into()
    } else {
        trimmed.into()
    }
}

fn parse_sprites(body: &str) -> Vec<SpritePart> {
    let mut out = Vec::new();
    for piece in body.split(';').filter(|s| !s.is_empty()) {
        let p: Vec<&str> = piece.split('|').collect();
        if p.len() < 11 {
            continue;
        }
        let n = |i: usize| p[i].parse::<f32>().unwrap_or(0.0);
        out.push(SpritePart {
            image: p[0].to_string(),
            local: [n(1), n(2)],
            size: [n(3), n(4)],
            tint: [n(5), n(6), n(7), n(8)],
            layer: p[9].parse().unwrap_or(0),
            order: p[10].parse().unwrap_or(0),
        });
    }
    out
}

fn parse_tris(body: &str) -> Vec<([[f32; 2]; 3], [f32; 3])> {
    let mut out = Vec::new();
    for piece in body.split(';').filter(|s| !s.is_empty()) {
        let p: Vec<f32> = piece.split('|').filter_map(|s| s.parse().ok()).collect();
        if p.len() >= 6 {
            let color = if p.len() >= 9 { [p[6], p[7], p[8]] } else { [1.0, 1.0, 1.0] };
            out.push(([[p[0], p[1]], [p[2], p[3]], [p[4], p[5]]], color));
        }
    }
    out
}

/// Sprite quads the object draws, in world space.
pub fn sprite_marks(entity: &Entity) -> Vec<SpriteMark> {
    if entity.merged.kind == "sprites" {
        return parse_sprites(&entity.merged.body)
            .into_iter()
            .map(|p| SpriteMark {
                image: p.image,
                x: entity.translation[0] + p.local[0],
                y: entity.translation[1] + p.local[1],
                w: p.size[0],
                h: p.size[1],
                rgba: tint_bytes(p.tint),
            })
            .collect();
    }
    if is_sprite(entity) {
        return vec![SpriteMark {
            image: entity.sprite.clone(),
            x: entity.translation[0],
            y: entity.translation[1],
            w: entity.scale[0].abs().max(0.05),
            h: entity.scale[1].abs().max(0.05),
            rgba: tint_bytes(entity.tint),
        }];
    }
    Vec::new()
}

/// Triangles the object draws, in world space.
pub fn tri_marks(entity: &Entity) -> Vec<TriMark> {
    if entity.merged.kind == "mesh" {
        return parse_tris(&entity.merged.body)
            .into_iter()
            .map(|(t, color)| TriMark {
                points: t.map(|p| [entity.translation[0] + p[0], entity.translation[1] + p[1]]),
                rgb: rgb_bytes(color),
            })
            .collect();
    }
    if is_sprite(entity) {
        return Vec::new();
    }
    let Some((_, color)) = front_half(entity) else {
        return Vec::new();
    };
    let rgb = rgb_bytes(color);
    local_tris(entity, [0.0, 0.0])
        .into_iter()
        .map(|t| TriMark { points: t, rgb })
        .collect()
}

fn tint_bytes(tint: [f32; 4]) -> [u8; 4] {
    tint.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn rgb_bytes(color: [f32; 3]) -> [u8; 3] {
    color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// How many sprite batches the merged group submits. Parts are stored in image order,
/// so one picture is one batch.
pub fn sprite_batches(entity: &Entity) -> usize {
    let marks = sprite_marks(entity);
    let mut n = 0;
    let mut prev = String::new();
    for mark in &marks {
        if mark.image != prev {
            n += 1;
            prev.clone_from(&mark.image);
        }
    }
    n
}

/// Paint marks into a fixed grid. The same marks always produce the same pixels.
pub fn raster(sprites: &[SpriteMark], tris: &[TriMark]) -> Vec<u8> {
    const N: i32 = 64;
    let mut pix = vec![0u8; (N * N * 4) as usize];
    let paint = |pix: &mut [u8], x: i32, y: i32, rgba: [u8; 4]| {
        if rgba[3] == 0 {
            return;
        }
        let i = ((y * N + x) * 4) as usize;
        pix[i] = rgba[0];
        pix[i + 1] = rgba[1];
        pix[i + 2] = rgba[2];
        pix[i + 3] = rgba[3];
    };
    for mark in sprites {
        let x0 = mark.x - mark.w * 0.5;
        let y0 = mark.y - mark.h * 0.5;
        let x1 = mark.x + mark.w * 0.5;
        let y1 = mark.y + mark.h * 0.5;
        for y in 0..N {
            for x in 0..N {
                let wx = (x as f32 / 4.0) - 8.0;
                let wy = (y as f32 / 4.0) - 8.0;
                if wx >= x0 && wx < x1 && wy >= y0 && wy < y1 {
                    paint(&mut pix, x, y, mark.rgba);
                }
            }
        }
    }
    for tri in tris {
        for y in 0..N {
            for x in 0..N {
                let p = [(x as f32 / 4.0) - 8.0, (y as f32 / 4.0) - 8.0];
                if point_in_tri(p, tri.points) {
                    paint(&mut pix, x, y, [tri.rgb[0], tri.rgb[1], tri.rgb[2], 255]);
                }
            }
        }
    }
    pix
}

fn point_in_tri(p: [f32; 2], t: [[f32; 2]; 3]) -> bool {
    let sign = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
        (a[0] - c[0]) * (b[1] - c[1]) - (b[0] - c[0]) * (a[1] - c[1])
    };
    let d1 = sign(p, t[0], t[1]);
    let d2 = sign(p, t[1], t[2]);
    let d3 = sign(p, t[2], t[0]);
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

pub fn drawn_sprites(entity: &Entity) -> Vec<DrawnSprite> {
    if entity.merged.kind != "sprites" {
        return Vec::new();
    }
    parse_sprites(&entity.merged.body)
        .into_iter()
        .map(|p| DrawnSprite {
            image: p.image,
            x: entity.translation[0] + p.local[0],
            y: entity.translation[1] + p.local[1],
            half: [p.size[0] * 0.5, p.size[1] * 0.5],
            tint: p.tint,
            layer: p.layer,
            order: p.order,
        })
        .collect()
}

pub fn drawn_tris(entity: &Entity) -> Vec<DrawnTri> {
    if entity.merged.kind != "mesh" {
        return Vec::new();
    }
    parse_tris(&entity.merged.body)
        .into_iter()
        .map(|(t, color)| DrawnTri {
            points: t.map(|p| [entity.translation[0] + p[0], entity.translation[1] + p[1]]),
            color,
        })
        .collect()
}

/// Marks of every listed object, sprites then triangles, for an equality check.
pub fn collect_marks(entities: &[Entity], ids: &[String]) -> (Vec<SpriteMark>, Vec<TriMark>) {
    let mut sprites = Vec::new();
    let mut tris = Vec::new();
    for id in ids {
        if let Some(entity) = entities.iter().find(|e| e.id == *id) {
            sprites.extend(sprite_marks(entity));
            tris.extend(tri_marks(entity));
        }
    }
    sprites.sort_by(|a, b| a.image.cmp(&b.image).then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal)));
    tris.sort_by(|a, b| a.points[0][0].partial_cmp(&b.points[0][0]).unwrap_or(std::cmp::Ordering::Equal));
    (sprites, tris)
}
