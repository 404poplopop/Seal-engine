//! The device interface (render hardware interface).

/// A texture on a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureFormat {
    /// 8-bit red, green, blue and alpha, straight (not premultiplied) alpha.
    Rgba8,
}

/// GPU texture shape beyond a single 2D image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextureShape {
    /// One 2D image (default).
    #[default]
    D2,
    /// Array of 2D layers with the same width and height.
    D2Array { layers: u32 },
    /// Volume texture: depth slices in the z dimension.
    D3 { depth: u32 },
    /// Six cube faces in +X −X +Y −Y +Z −Z order.
    Cube,
}

/// How a texture is sampled when a sprite is drawn larger or smaller than its pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextureFilter {
    /// Take the nearest pixel. Crisp pictures stay crisp.
    #[default]
    Nearest,
    /// Blend the neighbouring pixels.
    Linear,
}

/// What happens when a texture coordinate falls outside 0..1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextureWrap {
    /// The edge pixel continues. This is how every picture was drawn before wrap existed.
    #[default]
    Clamp,
    /// The picture tiles.
    Repeat,
    /// The picture tiles, mirrored on each repeat.
    Mirror,
}

/// How byte colours are read. sRGB is today's path: the byte is the colour.
/// Linear encodes the byte for display, so the same file looks different.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ColourSpace {
    #[default]
    Srgb,
    Linear,
}

/// Sampler descriptor for one texture. Defaults match pictures that have no import sidecar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureDesc {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub filter: TextureFilter,
    pub wrap: TextureWrap,
    pub colour_space: ColourSpace,
    /// When set, a minified draw samples the half-resolution mip.
    pub mipmaps: bool,
    pub shape: TextureShape,
}

impl TextureDesc {
    pub fn rgba8(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            format: TextureFormat::Rgba8,
            filter: TextureFilter::Nearest,
            wrap: TextureWrap::Clamp,
            colour_space: ColourSpace::Srgb,
            mipmaps: false,
            shape: TextureShape::D2,
        }
    }

    pub fn array_rgba8(width: u32, height: u32, layers: u32) -> Self {
        Self {
            shape: TextureShape::D2Array { layers: layers.max(1) },
            ..Self::rgba8(width, height)
        }
    }

    pub fn volume_rgba8(width: u32, height: u32, depth: u32) -> Self {
        Self {
            shape: TextureShape::D3 { depth: depth.max(1) },
            ..Self::rgba8(width, height)
        }
    }

    pub fn cubemap_rgba8(side: u32) -> Self {
        Self {
            shape: TextureShape::Cube,
            ..Self::rgba8(side, side)
        }
    }

    /// Depth or array layer count for wgpu `Extent3d::depth_or_array_layers`.
    pub fn depth_or_layers(self) -> u32 {
        match self.shape {
            TextureShape::D2 => 1,
            TextureShape::D2Array { layers } => layers.max(1),
            TextureShape::D3 { depth } => depth.max(1),
            TextureShape::Cube => 6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum BlendMode {
    /// Replace the target.
    Opaque,
    /// Source over destination by source alpha.
    #[default]
    Alpha,
    /// Add source times alpha to the destination.
    Additive,
}

/// One textured, tinted quad in target pixel space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    /// Corners in target pixels, in order: top-left, top-right, bottom-right, bottom-left.
    pub corners: [[f32; 2]; 4],
    /// Texture rectangle in 0..1 coordinates: min u, min v, max u, max v.
    pub uv: [f32; 4],
    /// Tint multiplied with the texture, red, green, blue and alpha in 0..1.
    pub color: [f32; 4],
    /// Per-corner texture coordinates. `None` uses the rectangle in `uv`.
    pub corner_uv: Option<[[f32; 2]; 4]>,
}

impl Instance {
    /// Texture coordinate at each corner. A missing override uses `uv` as a rectangle.
    pub fn corner_uvs(self) -> [[f32; 2]; 4] {
        if let Some(uvs) = self.corner_uv {
            uvs
        } else {
            let [u0, v0, u1, v1] = self.uv;
            [[u0, v0], [u1, v0], [u1, v1], [u0, v1]]
        }
    }
}

/// Quads that share a texture and blend mode, drawn in order.
#[derive(Clone, Debug, PartialEq)]
pub struct QuadBatch {
    /// `None` draws the tint color alone.
    pub texture: Option<TextureId>,
    pub blend: BlendMode,
    pub instances: Vec<Instance>,
}

/// What a backend must do. Kept small so new backends are cheap.
pub trait Device {
    fn create_texture(&mut self, desc: TextureDesc) -> TextureId;
    fn destroy_texture(&mut self, texture: TextureId);
    fn desc(&self, texture: TextureId) -> TextureDesc;
    /// Replace a texture's pixels (RGBA8, rows top to bottom).
    fn upload(&mut self, texture: TextureId, rgba: &[u8]);
    fn clear(&mut self, target: TextureId, color: [f32; 4]);
    fn draw_quads(&mut self, target: TextureId, batch: &QuadBatch);
    /// Copy pixels back (RGBA8, rows top to bottom).
    fn read_back(&mut self, texture: TextureId) -> Vec<u8>;
    /// Copy `source` onto `target` (same size).
    fn copy(&mut self, source: TextureId, target: TextureId);
}
