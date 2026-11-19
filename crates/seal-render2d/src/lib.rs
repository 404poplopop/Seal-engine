//! 2D rendering: sprites placed by the entity's transform, a 2D camera, and
//! batching that turns many sprites into few draws.
//!
//! World units are scene units with y pointing up; the camera maps them to
//! pixels with y pointing down. Rotation is `transform.rotation[2]` in
//! degrees, counter-clockwise, computed with the engine's deterministic
//! sine and cosine so frames match on every machine.

use std::collections::HashMap;

use seal_ecs::core::Transform;
use seal_ecs::{text_component, ChildOf, Entity, World};
use seal_render::graph::TexturePool;
use seal_render::{BlendMode, Device, Instance, QuadBatch, RenderGraph, TextureDesc, TextureId};

pub use seal_render::clip2d as mask;

pub mod glow;
pub mod grade;
mod scene_draw;

pub use scene_draw::{Glow, SortSettings, SpriteOrder, SpriteSlice};

/// A textured rectangle drawn at the entity's transform.
#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    /// Image name in the renderer's image table; empty draws a solid colour.
    pub image: String,
    /// Size in world units before the transform's scale.
    pub size: [f32; 2],
    /// Pivot inside the sprite: 0,0 is bottom left, 0.5,0.5 the centre.
    pub anchor: [f32; 2],
    pub color: [f32; 4],
    /// Higher layers draw on top. Within a layer, lower entity slots draw first.
    pub layer: i32,
    /// Texture area: u0, v0, u1, v1.
    pub uv: [f32; 4],
    pub flip: [bool; 2],
    pub additive: bool,
    /// Not drawn while set; the sprite keeps its place and settings.
    pub hidden: bool,
    /// Draw order among sprites on the same layer: higher draws on top.
    pub order: i32,
    /// `inside`, `outside`, or empty for no mask clipping.
    pub mask: String,
}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            image: String::new(),
            size: [1.0, 1.0],
            anchor: [0.5, 0.5],
            color: [1.0; 4],
            layer: 0,
            uv: [0.0, 0.0, 1.0, 1.0],
            flip: [false, false],
            additive: false,
            hidden: false,
            order: 0,
            mask: String::new(),
        }
    }
}
text_component!(Sprite as "sprite" { image, size, anchor, color, layer, uv, flip, additive, hidden, order, mask });

/// A sprite mask. `shape` is `rect`, `circle` or `alpha`; empty draws nothing.
/// The shape fills the entity's scale, from -0.5 to 0.5 before that scale, and
/// turns with `transform.rotation[2]`. An alpha mask uses the named image.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteMask {
    pub shape: String,
    pub image: String,
    pub cutoff: f32,
    pub layer_min: i32,
    pub layer_max: i32,
    pub order_min: i32,
    pub order_max: i32,
}

impl Default for SpriteMask {
    fn default() -> Self {
        Self {
            shape: String::new(),
            image: String::new(),
            cutoff: 0.5,
            layer_min: -1000,
            layer_max: 1000,
            order_min: -1000,
            order_max: 1000,
        }
    }
}
text_component!(SpriteMask as "sprite_mask" {
    shape, image, cutoff, layer_min, layer_max, order_min, order_max
});

/// The 2D view, kept as a world resource.
#[derive(Clone, Debug, PartialEq)]
pub struct Camera2D {
    /// World point at the centre of the screen.
    pub center: [f32; 2],
    /// Pixels per world unit.
    pub zoom: f32,
    pub clear: [f32; 4],
}

impl Default for Camera2D {
    fn default() -> Self {
        Self { center: [0.0, 0.0], zoom: 1.0, clear: [0.0, 0.0, 0.0, 1.0] }
    }
}
text_component!(Camera2D as "camera2d" { center, zoom, clear });

/// Register the 2D components on a world so scenes can save them.
pub fn register_2d(world: &mut World) {
    world.register_text::<Sprite>();
    world.register_text::<SpriteMask>();
    world.register_text::<SpriteOrder>();
    world.register_text::<SpriteSlice>();
    world.register_text_resource::<Camera2D>();
    world.register_text_resource::<SortSettings>();
    world.register_text_resource::<Glow>();
}

impl From<Affine2> for mask::Xform {
    fn from(m: Affine2) -> Self {
        Self { a: m.a, b: m.b, c: m.c, d: m.d, tx: m.tx, ty: m.ty }
    }
}

/// A 2D affine transform: x' = a*x + c*y + tx, y' = b*x + d*y + ty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine2 {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Affine2 {
    pub const IDENTITY: Self = Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };

    pub fn from_transform(t: &Transform) -> Self {
        let radians = t.rotation[2] * (std::f32::consts::PI / 180.0);
        let (s, c) = seal_math::sin_cos(radians);
        let (sx, sy) = (t.scale[0], t.scale[1]);
        Self {
            a: c * sx,
            b: s * sx,
            c: -s * sy,
            d: c * sy,
            tx: t.translation[0],
            ty: t.translation[1],
        }
    }

    /// `self` applied after `inner`.
    pub fn then(self, inner: Self) -> Self {
        Self {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            tx: self.a * inner.tx + self.c * inner.ty + self.tx,
            ty: self.b * inner.tx + self.d * inner.ty + self.ty,
        }
    }

    pub fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        [self.a * p[0] + self.c * p[1] + self.tx, self.b * p[0] + self.d * p[1] + self.ty]
    }
}

/// World transforms for every entity with a [`Transform`], parents applied.
pub fn world_transforms(world: &mut World) -> HashMap<Entity, Affine2> {
    let local: HashMap<Entity, (Affine2, Option<Entity>)> = {
        let rows =
            world.query::<(Entity, &Transform)>().collect(|(e, t)| (e, Affine2::from_transform(t)));
        rows.into_iter()
            .map(|(e, m)| (e, (m, world.get::<ChildOf>(e).map(|c| c.parent()))))
            .collect()
    };
    let mut out = HashMap::with_capacity(local.len());
    fn resolve(
        e: Entity,
        local: &HashMap<Entity, (Affine2, Option<Entity>)>,
        out: &mut HashMap<Entity, Affine2>,
    ) -> Affine2 {
        if let Some(m) = out.get(&e) {
            return *m;
        }
        let Some((m, parent)) = local.get(&e) else { return Affine2::IDENTITY };
        let global = match parent {
            Some(p) => resolve(*p, local, out).then(*m),
            None => *m,
        };
        out.insert(e, global);
        global
    }
    for e in local.keys() {
        resolve(*e, &local, &mut out);
    }
    out
}

/// How sprites in one layer may be reordered for fewer draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortMode {
    /// Keep spawn order inside a layer; overlaps always look right.
    #[default]
    Stable,
    /// Group by image inside a layer; fewer draws, for sprites that don't overlap.
    ByImage,
}

/// What the last frame did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub sprites: usize,
    pub offscreen: usize,
    pub batches: usize,
    pub missing_images: usize,
}

struct Draw {
    layer: i32,
    /// Sprite order in layer. Entity index breaks ties.
    sprite_order: i32,
    order: u32,
    texture: Option<TextureId>,
    blend: BlendMode,
    instance: Instance,
    mask: String,
    image: String,
}

/// Owns images and transient textures between frames.
#[derive(Default)]
pub struct Renderer2D {
    images: HashMap<String, TextureId>,
    /// CPU copy of each loaded image, so a picture mask can be tested per pixel.
    pixels: HashMap<String, (Vec<u8>, u32, u32)>,
    /// Tight mesh in cell space (y up) for pictures imported that way.
    meshes: HashMap<String, Vec<[f32; 2]>>,
    pool: TexturePool,
    pub sort: SortMode,
    /// Frame pixels per game pixel (render scale); 0 is read as 1.
    pub pixel_scale: f32,
}

impl Renderer2D {
    pub fn new() -> Self {
        Self::default()
    }

    /// Make an RGBA8 image available to sprites under `name` (nearest sampling).
    pub fn load_image<D: Device>(
        &mut self,
        device: &mut D,
        name: &str,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) {
        self.load_image_sampled(
            device,
            name,
            width,
            height,
            rgba,
            seal_render::picture::PictureSampling::default(),
        );
    }

    /// Like [`load_image`], sampling with `filter` when the sprite is scaled.
    pub fn load_image_filtered<D: Device>(
        &mut self,
        device: &mut D,
        name: &str,
        width: u32,
        height: u32,
        rgba: &[u8],
        filter: seal_render::TextureFilter,
    ) {
        self.load_image_sampled(
            device,
            name,
            width,
            height,
            rgba,
            seal_render::picture::PictureSampling { filter, ..Default::default() },
        );
    }

    /// Upload a picture with the import sampler: filter, wrap, colour space and mipmaps.
    pub fn load_image_sampled<D: Device>(
        &mut self,
        device: &mut D,
        name: &str,
        width: u32,
        height: u32,
        rgba: &[u8],
        sampling: seal_render::picture::PictureSampling,
    ) {
        let mut desc = TextureDesc::rgba8(width, height);
        sampling.apply(&mut desc);
        let texture = match self.images.get(name) {
            Some(t) if device.desc(*t) == desc => *t,
            existing => {
                if let Some(t) = existing {
                    device.destroy_texture(*t);
                }
                device.create_texture(desc)
            }
        };
        device.upload(texture, rgba);
        self.images.insert(name.into(), texture);
        self.pixels.insert(name.into(), (rgba.to_vec(), width, height));
    }

    /// Draw `name` with this outline instead of a full rectangle. Points are in
    /// cell space, y up. Fewer than three points clears the mesh.
    pub fn set_mesh(&mut self, name: &str, mesh: Vec<[f32; 2]>) {
        if mesh.len() >= 3 {
            self.meshes.insert(name.into(), mesh);
        } else {
            self.meshes.remove(name);
        }
    }

    pub fn image(&self, name: &str) -> Option<TextureId> {
        self.images.get(name).copied()
    }

    /// Turn the world's sprites into draw batches for a `width` x `height` target.
    pub fn build_batches(
        &self,
        world: &mut World,
        width: u32,
        height: u32,
    ) -> (Vec<QuadBatch>, FrameStats) {
        let _zone = seal_trace::zone!(seal_trace::Channel::CPU, "render2d.build");
        let (draws, mut stats) = self.collect_draws(world, width, height);
        let mut batches: Vec<QuadBatch> = Vec::new();
        for draw in draws {
            match batches.last_mut() {
                Some(b) if b.texture == draw.texture && b.blend == draw.blend => {
                    b.instances.push(draw.instance)
                }
                _ => batches.push(QuadBatch {
                    texture: draw.texture,
                    blend: draw.blend,
                    instances: vec![draw.instance],
                }),
            }
        }
        stats.batches = batches.len();
        (batches, stats)
    }

    /// Draw the world's sprites into `target`.
    pub fn render<D: Device>(
        &mut self,
        device: &mut D,
        world: &mut World,
        target: TextureId,
    ) -> FrameStats {
        let desc = device.desc(target);
        if self.masks_active(world) {
            return self.render_clipped(device, world, target, desc.width, desc.height);
        }
        let (batches, stats) = self.build_batches(world, desc.width, desc.height);
        self.draw_batches(device, world, target, batches);
        self.compose_glow(device, world, target);
        self.compose_grade(device, world, target);
        stats
    }

    fn compose_grade<D: Device>(&self, device: &mut D, world: &World, target: TextureId) {
        let Some(grade) = world.resource::<grade::PostGrade>().copied() else { return };
        if grade.is_identity() {
            return;
        }
        let src = device.read_back(target);
        let out = grade::apply(&src, grade);
        device.upload(target, &out);
    }

    fn draw_batches<D: Device>(
        &mut self,
        device: &mut D,
        world: &mut World,
        target: TextureId,
        batches: Vec<QuadBatch>,
    ) {
        let clear =
            world.resource::<Camera2D>().map(|c| c.clear).unwrap_or(Camera2D::default().clear);
        let mut graph = RenderGraph::<D>::new();
        let out = graph.import("screen", target);
        graph.add_pass(
            "sprites",
            |b| {
                b.write(out);
            },
            move |cx| {
                let t = cx.texture(out);
                cx.device.clear(t, clear);
                for batch in &batches {
                    cx.device.draw_quads(t, batch);
                }
            },
        );
        graph.execute(device, &mut self.pool).expect("the sprite graph is valid");
    }

    fn masks_active(&self, world: &mut World) -> bool {
        let care = world
            .query::<&Sprite>()
            .collect(|s| mask::Interaction::parse(&s.mask) != mask::Interaction::None);
        if !care.into_iter().any(|on| on) {
            return false;
        }
        world
            .query::<&SpriteMask>()
            .collect(|m| mask::Kind::parse(&m.shape).is_some())
            .into_iter()
            .any(|on| on)
    }

    fn gather_masks(&self, world: &mut World) -> Vec<mask::Mask> {
        let transforms = world_transforms(world);
        let rows = world.query::<(Entity, &SpriteMask)>().collect(|(e, m)| (e, m.clone()));
        rows.into_iter()
            .filter_map(|(entity, mask)| {
                let kind = mask::Kind::parse(&mask.shape)?;
                let xform = transforms.get(&entity).copied().unwrap_or(Affine2::IDENTITY);
                let alpha = if kind == mask::Kind::Alpha {
                    self.pixels.get(&mask.image).map(|(rgba, w, h)| mask::AlphaImage {
                        width: *w,
                        height: *h,
                        rgba: rgba.clone(),
                    })
                } else {
                    None
                };
                Some(mask::Mask {
                    xform: xform.into(),
                    kind,
                    alpha,
                    cutoff: mask.cutoff,
                    layer_min: mask.layer_min,
                    layer_max: mask.layer_max,
                    order_min: mask.order_min,
                    order_max: mask.order_max,
                })
            })
            .collect()
    }

    /// Same draw as [`build_batches`], but sprites that opt into masks are
    /// clipped per pixel into a mask texture and drawn with that.
    fn render_clipped<D: Device>(
        &mut self,
        device: &mut D,
        world: &mut World,
        target: TextureId,
        width: u32,
        height: u32,
    ) -> FrameStats {
        let camera = world.resource::<Camera2D>().cloned().unwrap_or_default();
        let masks = self.gather_masks(world);
        let (w, h) = (width as f32, height as f32);
        let zoom = camera.zoom.max(1e-6);
        let world_at = |sx: f32, sy: f32| {
            [(sx - w * 0.5) / zoom + camera.center[0], (h * 0.5 - sy) / zoom + camera.center[1]]
        };
        let (mut draws, mut stats) = self.collect_draws(world, width, height);
        let mut temps = Vec::new();
        for draw in &mut draws {
            let mode = mask::Interaction::parse(&draw.mask);
            let image = self.pixels.get(&draw.image);
            let Some(baked) = mask::bake(
                width,
                height,
                draw.instance.corners,
                draw.instance.uv,
                draw.instance.color,
                image,
                world_at,
                &masks,
                draw.layer,
                draw.sprite_order,
                mode,
            ) else {
                continue;
            };
            let tex = device.create_texture(TextureDesc::rgba8(baked.width, baked.height));
            device.upload(tex, &baked.rgba);
            temps.push(tex);
            let (x, y) = (baked.x as f32, baked.y as f32);
            let (bw, bh) = (baked.width as f32, baked.height as f32);
            draw.texture = Some(tex);
            draw.blend = BlendMode::Alpha;
            draw.instance = Instance {
                corners: [[x, y], [x + bw, y], [x + bw, y + bh], [x, y + bh]],
                uv: [0.0, 0.0, 1.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
                corner_uv: None,
            };
        }
        let mut batches: Vec<QuadBatch> = Vec::new();
        for draw in draws {
            match batches.last_mut() {
                Some(b) if b.texture == draw.texture && b.blend == draw.blend => {
                    b.instances.push(draw.instance)
                }
                _ => batches.push(QuadBatch {
                    texture: draw.texture,
                    blend: draw.blend,
                    instances: vec![draw.instance],
                }),
            }
        }
        stats.batches = batches.len();
        self.draw_batches(device, world, target, batches);
        self.compose_glow(device, world, target);
        self.compose_grade(device, world, target);
        for tex in temps {
            device.destroy_texture(tex);
        }
        stats
    }
}
