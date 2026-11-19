//! Sprite masks: a rectangle, a circle, or a picture's alpha, in the entity's
//! own space. Several masks combine as a union. A sprite is clipped only when
//! its sorting layer and order in layer fall inside a mask's range.

/// A 2D affine transform: x' = a*x + c*y + tx, y' = b*x + d*y + ty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Xform {
    /// Map a local point to world space.
    pub fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        [self.a * p[0] + self.c * p[1] + self.tx, self.b * p[0] + self.d * p[1] + self.ty]
    }
}

/// How a sprite uses the masks that cover its layer and order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interaction {
    /// Drawn whole. Masks do not change it.
    None,
    /// Drawn only where at least one affecting mask covers the pixel.
    Inside,
    /// Drawn only where no affecting mask covers the pixel.
    Outside,
}

impl Interaction {
    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "inside" => Self::Inside,
            "outside" => Self::Outside,
            _ => Self::None,
        }
    }
}

/// Which shape a mask uses. `Alpha` reads the picture stored on [`Mask`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rect,
    Circle,
    Alpha,
}

impl Kind {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "rect" => Some(Self::Rect),
            "circle" => Some(Self::Circle),
            "alpha" => Some(Self::Alpha),
            _ => None,
        }
    }
}

/// Decoded picture for an alpha mask. Alpha is the last byte of each pixel.
#[derive(Clone, Debug, PartialEq)]
pub struct AlphaImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One mask in view-plane units. `xform` maps local coordinates, where the
/// shape occupies -0.5..0.5 on each axis, into the plane.
#[derive(Clone, Debug, PartialEq)]
pub struct Mask {
    pub xform: Xform,
    pub kind: Kind,
    pub alpha: Option<AlphaImage>,
    /// Alpha at or above this (0 to 1) counts as inside a picture mask.
    pub cutoff: f32,
    pub layer_min: i32,
    pub layer_max: i32,
    pub order_min: i32,
    pub order_max: i32,
}

impl Mask {
    pub fn affects(&self, layer: i32, order: i32) -> bool {
        in_range(layer, self.layer_min, self.layer_max)
            && in_range(order, self.order_min, self.order_max)
    }
}

fn in_range(value: i32, min: i32, max: i32) -> bool {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    (lo..=hi).contains(&value)
}

/// Local point of a plane position, or None when the transform cannot be inverted.
pub fn to_local(xform: &Xform, world: [f32; 2]) -> Option<[f32; 2]> {
    let det = xform.a * xform.d - xform.c * xform.b;
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let x = world[0] - xform.tx;
    let y = world[1] - xform.ty;
    Some([(xform.d * x - xform.c * y) / det, (-xform.b * x + xform.a * y) / det])
}

/// Whether `world` lies inside the mask shape.
pub fn covers(mask: &Mask, world: [f32; 2]) -> bool {
    let Some(local) = to_local(&mask.xform, world) else {
        return false;
    };
    // A hair past the edge still counts, so a boundary pixel is not dropped by rounding.
    if local[0].abs() > 0.501 || local[1].abs() > 0.501 {
        return false;
    }
    match mask.kind {
        Kind::Rect => true,
        Kind::Circle => local[0] * local[0] + local[1] * local[1] <= 0.251,
        Kind::Alpha => {
            let Some(image) = &mask.alpha else {
                return false;
            };
            if image.width == 0 || image.height == 0 {
                return false;
            }
            // Picture row 0 is the top, which is local +y.
            let u = (local[0] + 0.5).clamp(0.0, 0.999_999);
            let v = (0.5 - local[1]).clamp(0.0, 0.999_999);
            let x = (u * image.width as f32).floor() as u32;
            let y = (v * image.height as f32).floor() as u32;
            let i = ((y * image.width + x) * 4 + 3) as usize;
            let alpha = image.rgba.get(i).copied().unwrap_or(0) as f32 / 255.0;
            alpha + f32::EPSILON >= mask.cutoff
        }
    }
}

/// Whether a sprite pixel at `world` is drawn.
///
/// Sprites outside every mask's layer and order range are left whole.
/// Inside a range, masks combine as a union.
pub fn visible(mode: Interaction, layer: i32, order: i32, world: [f32; 2], masks: &[Mask]) -> bool {
    if mode == Interaction::None {
        return true;
    }
    let mut any = false;
    let mut inside = false;
    for mask in masks {
        if !mask.affects(layer, order) {
            continue;
        }
        any = true;
        if covers(mask, world) {
            inside = true;
            break;
        }
    }
    if !any {
        return true;
    }
    match mode {
        Interaction::Inside => inside,
        Interaction::Outside => !inside,
        Interaction::None => true,
    }
}

/// A mask placed on the view plane from its centre, turn (radians, y up) and half size.
pub fn placed(center: [f32; 2], angle_rad: f32, half: [f32; 2]) -> Xform {
    let (s, c) = angle_rad.sin_cos();
    let (sx, sy) = (half[0] * 2.0, half[1] * 2.0);
    Xform { a: c * sx, b: s * sx, c: -s * sy, d: c * sy, tx: center[0], ty: center[1] }
}

fn edge(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

fn top_left(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[1] == b[1] && b[0] > a[0]) || b[1] < a[1]
}

/// Texture coordinate of screen point `p` inside a quad, using the same
/// top-left fill rule as the software rasteriser. None when `p` is outside.
pub fn quad_uv(corners: [[f32; 2]; 4], uv: [f32; 4], p: [f32; 2]) -> Option<[f32; 2]> {
    let uvs = [[uv[0], uv[1]], [uv[2], uv[1]], [uv[2], uv[3]], [uv[0], uv[3]]];
    for tri in [[0usize, 1, 2], [0, 2, 3]] {
        let mut v = [corners[tri[0]], corners[tri[1]], corners[tri[2]]];
        let mut t = [uvs[tri[0]], uvs[tri[1]], uvs[tri[2]]];
        let mut area = edge(v[0], v[1], v[2]);
        if area == 0.0 {
            continue;
        }
        if area < 0.0 {
            v.swap(1, 2);
            t.swap(1, 2);
            area = -area;
        }
        let edges = [(v[1], v[2]), (v[2], v[0]), (v[0], v[1])];
        let bias = edges.map(|(a, b)| top_left(a, b));
        let e = edges.map(|(a, b)| edge(a, b, p));
        if (0..3).any(|i| e[i] < 0.0 || (e[i] == 0.0 && !bias[i])) {
            continue;
        }
        let b = e.map(|value| value / area);
        let u = b[0] * t[0][0] + b[1] * t[1][0] + b[2] * t[2][0];
        let v = b[0] * t[0][1] + b[1] * t[1][1] + b[2] * t[2][1];
        return Some([u, v]);
    }
    None
}

fn sample(image: &(Vec<u8>, u32, u32), uv: [f32; 2]) -> [f32; 4] {
    let (rgba, w, h) = image;
    let w = (*w).max(1);
    let h = (*h).max(1);
    let x = ((uv[0] * w as f32).floor() as i64).clamp(0, w as i64 - 1) as u32;
    let y = ((uv[1] * h as f32).floor() as i64).clamp(0, h as i64 - 1) as u32;
    let i = ((y * w + x) * 4) as usize;
    let px = rgba.get(i..i + 4).unwrap_or(&[0, 0, 0, 0]);
    [px[0], px[1], px[2], px[3]].map(|c| c as f32 / 255.0)
}

/// A sprite quad clipped to masks, as a tight RGBA image in screen pixels.
/// None when the sprite is not affected, so the caller draws the quad as usual.
pub struct Baked {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn bake(
    target_w: u32,
    target_h: u32,
    corners: [[f32; 2]; 4],
    uv: [f32; 4],
    color: [f32; 4],
    image: Option<&(Vec<u8>, u32, u32)>,
    world_at: impl Fn(f32, f32) -> [f32; 2],
    masks: &[Mask],
    layer: i32,
    order: i32,
    mode: Interaction,
) -> Option<Baked> {
    if mode == Interaction::None || !masks.iter().any(|m| m.affects(layer, order)) {
        return None;
    }
    let (tw, th) = (target_w as i32, target_h as i32);
    let min_x = corners.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min).floor() as i32;
    let max_x = corners.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
    let min_y = corners.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min).floor() as i32;
    let max_y = corners.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
    let x0 = min_x.max(0);
    let y0 = min_y.max(0);
    let x1 = max_x.min(tw);
    let y1 = max_y.min(th);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (bw, bh) = ((x1 - x0) as u32, (y1 - y0) as u32);
    // Keep a single frame cheap. A sprite larger than this is drawn unclipped.
    if bw > 2048 || bh > 2048 {
        return None;
    }
    let mut rgba = vec![0u8; (bw * bh * 4) as usize];
    for y in y0..y1 {
        for x in x0..x1 {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let Some(tex) = quad_uv(corners, uv, p) else {
                continue;
            };
            let world = world_at(p[0], p[1]);
            if !visible(mode, layer, order, world, masks) {
                continue;
            }
            let mut px = color;
            if let Some(image) = image {
                let texel = sample(image, tex);
                for c in 0..4 {
                    px[c] *= texel[c];
                }
            }
            let a = px[3].clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let i = (((y - y0) * bw as i32 + (x - x0)) * 4) as usize;
            // Straight alpha. The quad is drawn with a white tint, so the
            // texture carries the final colour.
            rgba[i] = (px[0].clamp(0.0, 1.0) * 255.0).round() as u8;
            rgba[i + 1] = (px[1].clamp(0.0, 1.0) * 255.0).round() as u8;
            rgba[i + 2] = (px[2].clamp(0.0, 1.0) * 255.0).round() as u8;
            rgba[i + 3] = (a * 255.0).round() as u8;
        }
    }
    Some(Baked { x: x0, y: y0, width: bw, height: bh, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_at(center: [f32; 2], half: [f32; 2], angle_deg: f32) -> Mask {
        Mask {
            xform: placed(center, angle_deg.to_radians(), half),
            kind: Kind::Rect,
            alpha: None,
            cutoff: 0.5,
            layer_min: -1000,
            layer_max: 1000,
            order_min: -1000,
            order_max: 1000,
        }
    }

    #[test]
    fn rectangle_circle_and_turned_rectangle() {
        let rect = rect_at([0.0, 0.0], [2.0, 1.0], 0.0);
        assert!(covers(&rect, [1.9, 0.0]));
        assert!(!covers(&rect, [2.2, 0.0]));
        assert!(!covers(&rect, [0.0, 1.2]));
        let circle = Mask { kind: Kind::Circle, ..rect.clone() };
        assert!(covers(&circle, [0.0, 0.9]));
        assert!(covers(&circle, [1.9, 0.0]), "scale stretches a circle into an ellipse");
        assert!(!covers(&circle, [0.0, 1.2]), "past the short side");
        let turned = rect_at([0.0, 0.0], [2.0, 0.2], 90.0);
        assert!(covers(&turned, [0.0, 1.5]), "a flat bar turned 90 degrees stands up");
        assert!(!covers(&turned, [1.5, 0.0]));
    }

    #[test]
    fn picture_alpha_uses_the_cutoff_and_masks_union() {
        let rgba = vec![
            255, 0, 0, 255, 0, 0, 0, 0, //
            0, 0, 0, 10, 0, 0, 0, 255,
        ];
        let alpha = Mask {
            xform: placed([0.0, 0.0], 0.0, [1.0, 1.0]),
            kind: Kind::Alpha,
            alpha: Some(AlphaImage { width: 2, height: 2, rgba }),
            cutoff: 0.5,
            layer_min: 0,
            layer_max: 0,
            order_min: 0,
            order_max: 0,
        };
        assert!(covers(&alpha, [-0.5, 0.5]), "top left pixel is opaque");
        assert!(!covers(&alpha, [0.5, 0.5]), "top right pixel is clear");
        assert!(!covers(&alpha, [-0.5, -0.5]), "a pixel under the cutoff is outside");
        let mut other = rect_at([3.0, 0.0], [1.0, 1.0], 0.0);
        other.layer_min = 0;
        other.layer_max = 0;
        other.order_min = 0;
        other.order_max = 0;
        let masks = [alpha, other];
        assert!(visible(Interaction::Inside, 0, 0, [3.0, 0.0], &masks));
        assert!(
            !visible(Interaction::Inside, 0, 0, [0.25, 0.25], &masks),
            "the clear pixel stays hidden"
        );
        assert!(
            visible(Interaction::Inside, 1, 0, [0.0, 0.0], &masks),
            "other layers are not clipped"
        );
        assert!(!visible(Interaction::Outside, 0, 0, [3.0, 0.0], &masks));
        assert!(visible(Interaction::Outside, 0, 0, [0.25, 0.25], &masks));
    }

    #[test]
    fn a_missing_picture_covers_nothing() {
        let mask = Mask {
            xform: placed([0.0, 0.0], 0.0, [1.0, 1.0]),
            kind: Kind::Alpha,
            alpha: None,
            cutoff: 0.1,
            layer_min: 0,
            layer_max: 0,
            order_min: 0,
            order_max: 0,
        };
        assert!(!covers(&mask, [0.0, 0.0]));
    }
}
