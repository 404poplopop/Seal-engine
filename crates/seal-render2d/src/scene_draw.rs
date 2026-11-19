//! Runtime sprite quads: the same order, pivot and tiled slices the editor uses.

use std::collections::HashMap;

use seal_ecs::core::Transform;
use seal_ecs::sprite_draw::{self, ObjectSortMode, SortEntry};
use seal_ecs::{text_component, ChildOf, Entity, World};
use seal_render::{BlendMode, Instance};

use super::{world_transforms, Affine2, Camera2D, Draw, FrameStats, Renderer2D, SortMode, Sprite};

/// Per-sprite sort override. Absent means the layer rule, then depth.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteOrder {
    pub mode: String,
}

impl Default for SpriteOrder {
    fn default() -> Self {
        Self { mode: "default".into() }
    }
}
text_component!(SpriteOrder as "sprite_order" { mode });

/// Nine-slice borders, tile or stretch, and an optional world size.
/// `border` is left, right, top, bottom in picture pixels. `size` replaces the
/// sprite size when both numbers are above zero. `picture` is the texture size
/// in pixels; zero uses the loaded image.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteSlice {
    pub border: [f32; 4],
    pub mode: String,
    pub size: [f32; 2],
    pub picture: [f32; 2],
}

impl Default for SpriteSlice {
    fn default() -> Self {
        Self { border: [0.0; 4], mode: "stretch".into(), size: [0.0; 2], picture: [0.0; 2] }
    }
}
text_component!(SpriteSlice as "sprite_slice" { border, mode, size, picture });

/// Project sorting: `layers` is `layer:mode` pairs, `axis` is the custom axis weights.
#[derive(Clone, Debug, PartialEq)]
pub struct SortSettings {
    pub layers: String,
    pub axis: [f32; 2],
}

impl Default for SortSettings {
    fn default() -> Self {
        Self { layers: String::new(), axis: [0.0, 1.0] }
    }
}
text_component!(SortSettings as "sort_settings" { layers, axis });

/// Soft glow for the runtime frame. Off unless `enabled` is set.
#[derive(Clone, Debug, PartialEq)]
pub struct Glow {
    pub enabled: bool,
    pub threshold: f32,
    pub intensity: f32,
    pub radius: f32,
}

impl Default for Glow {
    fn default() -> Self {
        Self { enabled: false, threshold: 0.8, intensity: 1.0, radius: 4.0 }
    }
}
text_component!(Glow as "glow" { enabled, threshold, intensity, radius });

struct Rec {
    entity: Entity,
    sprite: Sprite,
    mode: ObjectSortMode,
    transform: Transform,
    slice: Option<SpriteSlice>,
    matrix: Affine2,
    local_size: [f32; 2],
    sort_size: [f32; 2],
}

impl Renderer2D {
    pub(crate) fn collect_draws(
        &self,
        world: &mut World,
        width: u32,
        height: u32,
    ) -> (Vec<Draw>, FrameStats) {
        let camera = world.resource::<Camera2D>().cloned().unwrap_or_default();
        let transforms = world_transforms(world);
        let (w, h) = (width as f32, height as f32);
        // Render scale: a frame drawn at a different size shows the same view.
        let zoom = camera.zoom * if self.pixel_scale > 0.0 { self.pixel_scale } else { 1.0 };
        let view = |p: [f32; 2]| {
            [
                (p[0] - camera.center[0]) * zoom + w * 0.5,
                h * 0.5 - (p[1] - camera.center[1]) * zoom,
            ]
        };
        let rows = world.query::<(Entity, &Sprite)>().collect(|(e, s)| (e, s.clone()));
        let mut stats = FrameStats::default();
        let mut recs = Vec::new();
        for (entity, sprite) in rows {
            if sprite.hidden {
                continue;
            }
            stats.sprites += 1;
            let transform = world.get::<Transform>(entity).cloned().unwrap_or_default();
            let slice = world.get::<SpriteSlice>(entity).cloned();
            let mode = world
                .get::<SpriteOrder>(entity)
                .map(|o| ObjectSortMode::parse(&o.mode))
                .unwrap_or_default();
            let sized = slice.as_ref().is_some_and(|s| s.size[0] > 0.0 && s.size[1] > 0.0);
            let local_size =
                slice.as_ref().filter(|_| sized).map(|s| s.size).unwrap_or(sprite.size);
            let sort_size = if sized {
                local_size
            } else {
                [
                    sprite.size[0] * transform.scale[0].abs(),
                    sprite.size[1] * transform.scale[1].abs(),
                ]
            };
            let matrix = if sized {
                let local = Affine2::from_transform(&Transform {
                    translation: transform.translation,
                    rotation: transform.rotation,
                    scale: [1.0, 1.0, 1.0],
                });
                match world.get::<ChildOf>(entity).map(|c| c.parent()) {
                    Some(parent) => {
                        transforms.get(&parent).copied().unwrap_or(Affine2::IDENTITY).then(local)
                    }
                    None => local,
                }
            } else {
                transforms.get(&entity).copied().unwrap_or(Affine2::IDENTITY)
            };
            if !sprite.image.is_empty() && !self.images.contains_key(&sprite.image) {
                stats.missing_images += 1;
            }
            recs.push(Rec {
                entity,
                sprite,
                mode,
                transform,
                slice,
                matrix,
                local_size,
                sort_size,
            });
        }
        recs.sort_by_key(|rec| rec.entity.index());
        let groups = self.groups(world, &recs);
        let editor = self.editor_order(world, &recs);
        let order: Vec<usize> = match &editor {
            Some((rules, axis)) => {
                let entries: Vec<SortEntry> = recs
                    .iter()
                    .enumerate()
                    .map(|(index, rec)| {
                        let pivot = sprite_draw::sort_pivot(
                            [rec.transform.translation[0], rec.transform.translation[1]],
                            rec.sprite.anchor,
                            rec.sort_size,
                        );
                        SortEntry {
                            index,
                            entity_index: rec.entity.index() as usize,
                            layer: rec.sprite.layer,
                            order: rec.sprite.order,
                            depth: rec.transform.translation[2],
                            pivot_x: pivot[0],
                            pivot_y: pivot[1],
                            object_mode: rec.mode,
                            group_parent: groups[index],
                        }
                    })
                    .collect();
                sprite_draw::sort_indices(&entries, rules, *axis)
            }
            None => (0..recs.len()).collect(),
        };
        let mut draws = Vec::new();
        for index in order {
            let rec = &recs[index];
            let [x0, y0, x1, y1] = sprite_draw::pivot_rect(rec.local_size, rec.sprite.anchor);
            let local = [[x0, y1], [x1, y1], [x1, y0], [x0, y0]];
            let corners = local.map(|p| view(rec.matrix.apply(p)));
            if offscreen(corners, w, h) {
                stats.offscreen += 1;
                continue;
            }
            let texture = if rec.sprite.image.is_empty() {
                None
            } else {
                self.images.get(&rec.sprite.image).copied()
            };
            let mut uv = rec.sprite.uv;
            if rec.sprite.flip[0] {
                uv.swap(0, 2);
            }
            if rec.sprite.flip[1] {
                uv.swap(1, 3);
            }
            let blend = if rec.sprite.additive { BlendMode::Additive } else { BlendMode::Alpha };
            let picture = picture_size(self, rec);
            let instances = if rec.sprite.mask.is_empty() && rec.slice.is_none() {
                if let Some(mesh) = self.meshes.get(&rec.sprite.image) {
                    mesh_instances([x0, y0, x1, y1], &rec.matrix, &view, uv, rec.sprite.color, mesh)
                } else {
                    tile_or_one(
                        corners,
                        uv,
                        rec.sprite.color,
                        rec.slice.as_ref(),
                        picture,
                        rec.sort_size,
                    )
                }
            } else {
                tile_or_one(
                    corners,
                    uv,
                    rec.sprite.color,
                    rec.slice.as_ref(),
                    picture,
                    rec.sort_size,
                )
            };
            for instance in instances {
                draws.push(Draw {
                    layer: rec.sprite.layer,
                    sprite_order: rec.sprite.order,
                    order: rec.entity.index(),
                    texture,
                    blend,
                    instance,
                    mask: rec.sprite.mask.clone(),
                    image: rec.sprite.image.clone(),
                });
            }
        }
        if editor.is_none() {
            match self.sort {
                SortMode::Stable => draws.sort_by_key(|d| (d.layer, d.sprite_order, d.order)),
                SortMode::ByImage => draws.sort_by_key(|d| {
                    (d.layer, d.sprite_order, d.blend as u8, d.texture.map(|t| t.0), d.order)
                }),
            }
        }
        (draws, stats)
    }

    fn editor_order(
        &self,
        world: &World,
        recs: &[Rec],
    ) -> Option<(HashMap<i32, sprite_draw::LayerSortMode>, [f32; 2])> {
        let settings = world.resource::<SortSettings>().cloned();
        let custom = recs.iter().any(|r| r.mode != ObjectSortMode::Default)
            || settings.as_ref().is_some_and(|s| !s.layers.trim().is_empty());
        if !custom {
            return None;
        }
        let settings = settings.unwrap_or_default();
        Some((sprite_draw::parse_sort_layers(&settings.layers), settings.axis))
    }

    fn groups(&self, world: &World, recs: &[Rec]) -> Vec<Option<usize>> {
        let ids: Vec<Entity> = recs.iter().map(|r| r.entity).collect();
        let index_of: HashMap<Entity, usize> =
            ids.iter().enumerate().map(|(i, e)| (*e, i)).collect();
        (0..recs.len())
            .map(|index| {
                sprite_draw::group_root(
                    index,
                    |ix| {
                        let mut walk = world.get::<ChildOf>(ids[ix]).map(|c| c.parent());
                        while let Some(parent) = walk {
                            if let Some(&pix) = index_of.get(&parent) {
                                return Some(pix);
                            }
                            walk = world.get::<ChildOf>(parent).map(|c| c.parent());
                        }
                        None
                    },
                    |ix| recs[ix].mode == ObjectSortMode::SortGroup,
                )
            })
            .collect()
    }

    pub(crate) fn compose_glow<D: seal_render::Device>(
        &self,
        device: &mut D,
        world: &World,
        target: seal_render::TextureId,
    ) {
        let Some(glow) = world.resource::<Glow>() else { return };
        if !glow.enabled || glow.intensity <= 0.0 || glow.radius <= 0.0 {
            return;
        }
        let desc = device.desc(target);
        let src = device.read_back(target);
        let out = super::glow::apply(
            &src,
            desc.width,
            desc.height,
            glow.threshold,
            glow.intensity,
            glow.radius,
        );
        device.upload(target, &out);
    }
}

fn picture_size(renderer: &Renderer2D, rec: &Rec) -> [f32; 2] {
    if let Some(slice) = &rec.slice {
        if slice.picture[0] > 0.0 && slice.picture[1] > 0.0 {
            return slice.picture;
        }
    }
    renderer
        .pixels
        .get(&rec.sprite.image)
        .map(|(_, w, h)| [*w as f32, *h as f32])
        .unwrap_or([1.0, 1.0])
}

fn offscreen(corners: [[f32; 2]; 4], w: f32, h: f32) -> bool {
    let min_x = corners.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let max_x = corners.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
    let min_y = corners.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let max_y = corners.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
    max_x <= 0.0 || max_y <= 0.0 || min_x >= w || min_y >= h
}

fn mesh_instances(
    rect: [f32; 4],
    matrix: &Affine2,
    view: &impl Fn([f32; 2]) -> [f32; 2],
    uv: [f32; 4],
    color: [f32; 4],
    mesh: &[[f32; 2]],
) -> Vec<Instance> {
    if mesh.len() < 3 {
        return Vec::new();
    }
    let [x0, y0, x1, y1] = rect;
    let place = |point: [f32; 2]| {
        let local = [x0 + (x1 - x0) * point[0], y0 + (y1 - y0) * point[1]];
        view(matrix.apply(local))
    };
    let tex = |point: [f32; 2]| {
        [uv[0] + point[0] * (uv[2] - uv[0]), uv[1] + (1.0 - point[1]) * (uv[3] - uv[1])]
    };
    let mut out = Vec::new();
    for index in 1..mesh.len() - 1 {
        let (a, b, c) = (mesh[0], mesh[index], mesh[index + 1]);
        let corners = [place(a), place(b), place(c), place(c)];
        out.push(Instance {
            corners,
            uv,
            color,
            corner_uv: Some([tex(a), tex(b), tex(c), tex(c)]),
        });
    }
    out
}

fn tile_or_one(
    corners: [[f32; 2]; 4],
    uv: [f32; 4],
    color: [f32; 4],
    slice: Option<&SpriteSlice>,
    picture: [f32; 2],
    scale: [f32; 2],
) -> Vec<Instance> {
    let Some(slice) = slice else {
        return vec![Instance { corners, uv, color, corner_uv: None }];
    };
    let tiled = slice.mode == "tile" && slice.border.iter().any(|v| *v > 0.0);
    if !tiled {
        return vec![Instance { corners, uv, color, corner_uv: None }];
    }
    sprite_draw::tile_pieces(slice.border, uv, picture, scale)
        .into_iter()
        .map(|piece| {
            let [fx0, fy0, fx1, fy1] = piece.span;
            let [u0, v0, u1, v1] = piece.uv;
            Instance {
                corners: [
                    at(corners, fx0, fy0),
                    at(corners, fx1, fy0),
                    at(corners, fx1, fy1),
                    at(corners, fx0, fy1),
                ],
                uv: [u0, v1, u1, v0],
                color,
                corner_uv: None,
            }
        })
        .collect()
}

fn at(corners: [[f32; 2]; 4], fx: f32, fy: f32) -> [f32; 2] {
    let top = [
        corners[0][0] + (corners[1][0] - corners[0][0]) * fx,
        corners[0][1] + (corners[1][1] - corners[0][1]) * fx,
    ];
    let bottom = [
        corners[3][0] + (corners[2][0] - corners[3][0]) * fx,
        corners[3][1] + (corners[2][1] - corners[3][1]) * fx,
    ];
    [top[0] + (bottom[0] - top[0]) * fy, top[1] + (bottom[1] - top[1]) * fy]
}
