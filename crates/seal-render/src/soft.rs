//! A device that draws on the CPU.
//!
//! Only `+ - * /`, comparisons and rounding are used, all exactly specified
//! by IEEE-754, so the same frame gives the same bytes on every machine.
//! Triangles follow the top-left fill rule: a pixel on an edge shared by two
//! triangles is drawn by exactly one of them, so blended quads show no seams.

use crate::device::{BlendMode, ColourSpace, Device, Instance, QuadBatch, TextureDesc, TextureId};
use crate::picture::{self, use_mip};

struct Texture {
    desc: TextureDesc,
    rgba: Vec<u8>,
    mip: Vec<u8>,
    mip_w: u32,
    mip_h: u32,
}

#[derive(Default)]
pub struct SoftDevice {
    textures: Vec<Option<Texture>>,
    /// Pixels written since creation, for tests and statistics.
    pub pixels_drawn: u64,
}

impl SoftDevice {
    pub fn new() -> Self {
        Self::default()
    }

    fn tex(&self, id: TextureId) -> &Texture {
        self.textures[id.0 as usize].as_ref().expect("live texture")
    }

    /// Live textures, for leak checks.
    pub fn live_textures(&self) -> usize {
        self.textures.iter().filter(|t| t.is_some()).count()
    }
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn edge(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

/// For a triangle wound so that `edge` is positive inside (clockwise on a
/// y-down screen), a top edge runs exactly left to right and a left edge
/// runs upwards.
fn top_left(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[1] == b[1] && b[0] > a[0]) || b[1] < a[1]
}

impl Device for SoftDevice {
    fn create_texture(&mut self, desc: TextureDesc) -> TextureId {
        let rgba = vec![0; desc.width as usize * desc.height as usize * 4];
        let texture = Texture { desc, rgba, mip: Vec::new(), mip_w: 0, mip_h: 0 };
        match self.textures.iter().position(Option::is_none) {
            Some(i) => {
                self.textures[i] = Some(texture);
                TextureId(i as u32)
            }
            None => {
                self.textures.push(Some(texture));
                TextureId(self.textures.len() as u32 - 1)
            }
        }
    }

    fn destroy_texture(&mut self, texture: TextureId) {
        self.textures[texture.0 as usize] = None;
    }

    fn desc(&self, texture: TextureId) -> TextureDesc {
        self.tex(texture).desc
    }

    fn upload(&mut self, texture: TextureId, rgba: &[u8]) {
        let t = self.textures[texture.0 as usize].as_mut().expect("live texture");
        assert_eq!(rgba.len(), t.rgba.len(), "upload size must match the texture");
        t.rgba.copy_from_slice(rgba);
        if t.desc.mipmaps {
            let (mip, mw, mh) = picture::box_mip(rgba, t.desc.width, t.desc.height);
            t.mip = mip;
            t.mip_w = mw;
            t.mip_h = mh;
        } else {
            t.mip.clear();
            t.mip_w = 0;
            t.mip_h = 0;
        }
    }

    fn clear(&mut self, target: TextureId, color: [f32; 4]) {
        let px = color.map(to_u8);
        let t = self.textures[target.0 as usize].as_mut().expect("live texture");
        #[allow(clippy::chunks_exact_to_as_chunks)]
        for chunk in t.rgba.chunks_exact_mut(4) {
            chunk.copy_from_slice(&px);
        }
    }

    fn read_back(&mut self, texture: TextureId) -> Vec<u8> {
        self.tex(texture).rgba.clone()
    }

    fn copy(&mut self, source: TextureId, target: TextureId) {
        let pixels = self.tex(source).rgba.clone();
        self.upload(target, &pixels);
    }

    fn draw_quads(&mut self, target: TextureId, batch: &QuadBatch) {
        // Take the target out so the source texture can be borrowed.
        let mut dst = self.textures[target.0 as usize].take().expect("live texture");
        let source = batch.texture.map(|id| self.tex(id));
        let mut drawn = 0;
        for instance in &batch.instances {
            let [p0, p1, p2, p3] = instance.corners;
            let uvs = instance.corner_uvs();
            for tri in [[0usize, 1, 2], [0, 2, 3]] {
                drawn += raster(
                    &mut dst,
                    [[p0, p1, p2, p3][tri[0]], [p0, p1, p2, p3][tri[1]], [p0, p1, p2, p3][tri[2]]],
                    [uvs[tri[0]], uvs[tri[1]], uvs[tri[2]]],
                    instance,
                    source,
                    batch.blend,
                );
            }
        }
        self.textures[target.0 as usize] = Some(dst);
        self.pixels_drawn += drawn;
    }
}

fn texel(texture: &Texture, rgba: &[u8], width: u32, height: u32, x: i64, y: i64) -> [f32; 4] {
    let x = picture::wrap_index(x, width as i64, texture.desc.wrap) as usize;
    let y = picture::wrap_index(y, height as i64, texture.desc.wrap) as usize;
    let i = (y * width as usize + x) * 4;
    let p = &rgba[i..i + 4];
    let rgb = if texture.desc.colour_space == ColourSpace::Linear {
        [
            picture::linear_channel_to_display(p[0]),
            picture::linear_channel_to_display(p[1]),
            picture::linear_channel_to_display(p[2]),
        ]
    } else {
        [p[0], p[1], p[2]]
    };
    [rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0, p[3] as f32 / 255.0]
}

fn sample(texture: &Texture, uv: [f32; 2], mip: bool) -> [f32; 4] {
    let (rgba, width, height) =
        if mip && !texture.mip.is_empty() && texture.mip_w > 0 && texture.mip_h > 0 {
            (texture.mip.as_slice(), texture.mip_w, texture.mip_h)
        } else {
            (texture.rgba.as_slice(), texture.desc.width, texture.desc.height)
        };
    if width == 0 || height == 0 {
        return [0.0; 4];
    }
    let w = width as f32;
    let h = height as f32;
    if texture.desc.filter == crate::device::TextureFilter::Linear && w >= 1.0 && h >= 1.0 {
        // Pixel centres sit on half-texels, matching a linear sampler.
        let x = uv[0] * w - 0.5;
        let y = uv[1] * h - 0.5;
        let x0 = x.floor() as i64;
        let y0 = y.floor() as i64;
        let tx = x - x0 as f32;
        let ty = y - y0 as f32;
        let row = |y: i64| {
            let a = texel(texture, rgba, width, height, x0, y);
            let b = texel(texture, rgba, width, height, x0 + 1, y);
            [0, 1, 2, 3].map(|c| a[c] + (b[c] - a[c]) * tx)
        };
        let a = row(y0);
        let b = row(y0 + 1);
        return [0, 1, 2, 3].map(|c| a[c] + (b[c] - a[c]) * ty);
    }
    let x = (uv[0] * w).floor() as i64;
    let y = (uv[1] * h).floor() as i64;
    texel(texture, rgba, width, height, x, y)
}

fn raster(
    dst: &mut Texture,
    mut v: [[f32; 2]; 3],
    mut uv: [[f32; 2]; 3],
    instance: &Instance,
    source: Option<&Texture>,
    blend: BlendMode,
) -> u64 {
    let mut area = edge(v[0], v[1], v[2]);
    if area == 0.0 {
        return 0;
    }
    if area < 0.0 {
        v.swap(1, 2);
        uv.swap(1, 2);
        area = -area;
    }
    let minified = source.is_some_and(|texture| {
        let span_u = (uv[0][0] - uv[1][0]).abs().max((uv[0][0] - uv[2][0]).abs());
        let span_px = (v[0][0] - v[1][0]).abs().max((v[0][0] - v[2][0]).abs());
        use_mip(texture.desc.mipmaps, texture.desc.width as f32, span_u, span_px)
    });
    let (w, h) = (dst.desc.width as i64, dst.desc.height as i64);
    let min_x = (v.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min).floor() as i64).max(0);
    let max_x = (v.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max).ceil() as i64).min(w);
    let min_y = (v.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min).floor() as i64).max(0);
    let max_y = (v.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max).ceil() as i64).min(h);
    let edges = [(v[1], v[2]), (v[2], v[0]), (v[0], v[1])];
    let bias = edges.map(|(a, b)| top_left(a, b));
    let mut drawn = 0;
    for y in min_y..max_y {
        for x in min_x..max_x {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let e = edges.map(|(a, b)| edge(a, b, p));
            if (0..3).any(|i| e[i] < 0.0 || (e[i] == 0.0 && !bias[i])) {
                continue;
            }
            let b = e.map(|value| value / area);
            let mut color = instance.color;
            if let Some(texture) = source {
                let u = b[0] * uv[0][0] + b[1] * uv[1][0] + b[2] * uv[2][0];
                let t = b[0] * uv[0][1] + b[1] * uv[1][1] + b[2] * uv[2][1];
                let texel = sample(texture, [u, t], minified);
                for c in 0..4 {
                    color[c] *= texel[c];
                }
            }
            let i = ((y * w + x) * 4) as usize;
            let d = &mut dst.rgba[i..i + 4];
            let old = [d[0], d[1], d[2], d[3]].map(|c| c as f32 / 255.0);
            let a = color[3].clamp(0.0, 1.0);
            let out = match blend {
                BlendMode::Opaque => color,
                BlendMode::Alpha => [
                    color[0] * a + old[0] * (1.0 - a),
                    color[1] * a + old[1] * (1.0 - a),
                    color[2] * a + old[2] * (1.0 - a),
                    a + old[3] * (1.0 - a),
                ],
                BlendMode::Additive => {
                    [old[0] + color[0] * a, old[1] + color[1] * a, old[2] + color[2] * a, old[3]]
                }
            };
            d.copy_from_slice(&out.map(to_u8));
            drawn += 1;
        }
    }
    drawn
}
