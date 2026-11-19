//! Lighting for flat 2D scenes: point, spot and global lights, hard shadows
//! cast by solid boxes, and optional normal-map shading on sprites.

/// What kind of 2D light an object is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// Reaches everywhere equally, with no falloff and no shadows.
    Global,
    /// Shines in every direction and fades to nothing at `range`.
    Point,
    /// Like a point light, but only inside a cone around `angle`.
    Spot,
    /// Parallel light from `angle`. Shadows run a long way from solid boxes.
    Directional,
    /// Fills a polygon and fades across `falloff` outside its edge.
    Freeform,
}

impl Kind {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "global" => Some(Self::Global),
            "point" => Some(Self::Point),
            "spot" => Some(Self::Spot),
            "directional" => Some(Self::Directional),
            "freeform" => Some(Self::Freeform),
            _ => None,
        }
    }
}

/// How one light mixes into the light already there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    /// Add the light. This is what older scenes do.
    Additive,
    /// Darken toward the light colour.
    Multiply,
    /// Fade from the current light toward this light's colour.
    Alpha,
}

impl Blend {
    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "multiply" => Self::Multiply,
            "alpha" => Self::Alpha,
            _ => Self::Additive,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub kind: Kind,
    pub pos: [f32; 2],
    /// Red, green and blue, each 0 to 1.
    pub color: [f32; 3],
    pub intensity: f32,
    /// Distance where a point or spot light reaches zero.
    pub range: f32,
    /// Direction a spot or directional light points, in degrees (0 = +x, 90 = +y).
    pub angle: f32,
    /// Full width of a spot light's cone, in degrees.
    pub cone: f32,
    /// Whether solid boxes block this light.
    pub shadows: bool,
    pub blend: Blend,
    /// Distance outside a freeform polygon where the light reaches zero.
    pub falloff: f32,
    /// Freeform polygon, already placed in the scene.
    pub points: Vec<[f32; 2]>,
}

/// A solid axis-aligned box that blocks light: centre and half size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blocker {
    pub center: [f32; 2],
    pub half: [f32; 2],
}

impl Blocker {
    fn contains(&self, p: [f32; 2]) -> bool {
        (p[0] - self.center[0]).abs() <= self.half[0]
            && (p[1] - self.center[1]).abs() <= self.half[1]
    }

    /// Whether the segment from `a` to `b` passes through the box.
    /// Segments that start or end inside the box do not count, so a lit
    /// surface is never shadowed by the box it belongs to.
    pub fn blocks(&self, a: [f32; 2], b: [f32; 2]) -> bool {
        if self.contains(a) || self.contains(b) {
            return false;
        }
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        for axis in 0..2 {
            let d = b[axis] - a[axis];
            let lo = self.center[axis] - self.half[axis];
            let hi = self.center[axis] + self.half[axis];
            if d.abs() < 1e-9 {
                if a[axis] < lo || a[axis] > hi {
                    return false;
                }
                continue;
            }
            let (mut s0, mut s1) = ((lo - a[axis]) / d, (hi - a[axis]) / d);
            if s0 > s1 {
                std::mem::swap(&mut s0, &mut s1);
            }
            t0 = t0.max(s0);
            t1 = t1.min(s1);
            if t0 > t1 {
                return false;
            }
        }
        true
    }

    /// Whether a ray from `origin` along `dir` hits the box. A ray that starts
    /// inside the box does not count.
    pub fn blocks_ray(&self, origin: [f32; 2], dir: [f32; 2]) -> bool {
        if self.contains(origin) {
            return false;
        }
        let mut t0 = 0.0f32;
        let mut t1 = f32::INFINITY;
        for axis in 0..2 {
            let d = dir[axis];
            let lo = self.center[axis] - self.half[axis];
            let hi = self.center[axis] + self.half[axis];
            if d.abs() < 1e-9 {
                if origin[axis] < lo || origin[axis] > hi {
                    return false;
                }
                continue;
            }
            let (mut s0, mut s1) = ((lo - origin[axis]) / d, (hi - origin[axis]) / d);
            if s0 > s1 {
                std::mem::swap(&mut s0, &mut s1);
            }
            t0 = t0.max(s0);
            t1 = t1.min(s1);
            if t0 > t1 {
                return false;
            }
        }
        t1 > 1e-4
    }
}

fn centroid(points: &[[f32; 2]]) -> [f32; 2] {
    let n = points.len() as f32;
    let sum = points.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0], a[1] + p[1]]);
    [sum[0] / n.max(1.0), sum[1] / n.max(1.0)]
}

fn inside_polygon(p: [f32; 2], poly: &[[f32; 2]]) -> bool {
    let mut inside = false;
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[j], poly[i]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0];
            if p[0] < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

fn dist_to_polygon(p: [f32; 2], poly: &[[f32; 2]]) -> f32 {
    let n = poly.len();
    let mut best = f32::MAX;
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let ab = [b[0] - a[0], b[1] - a[1]];
        let ap = [p[0] - a[0], p[1] - a[1]];
        let ab2 = ab[0] * ab[0] + ab[1] * ab[1];
        let t =
            if ab2 < 1e-12 { 0.0 } else { ((ap[0] * ab[0] + ap[1] * ab[1]) / ab2).clamp(0.0, 1.0) };
        let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
        let d = [p[0] - q[0], p[1] - q[1]];
        best = best.min((d[0] * d[0] + d[1] * d[1]).sqrt());
    }
    best
}

/// Place local polygon corners into the scene.
pub fn place_points(
    origin: [f32; 2],
    angle_deg: f32,
    scale: [f32; 2],
    local: &[[f32; 2]],
) -> Vec<[f32; 2]> {
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    local
        .iter()
        .map(|p| {
            let (x, y) = (p[0] * scale[0], p[1] * scale[1]);
            [origin[0] + x * cos - y * sin, origin[1] + x * sin + y * cos]
        })
        .collect()
}

/// How strongly `light` reaches `p` before colour: falloff and cone, 0 to 1.
fn reach(light: &Light, p: [f32; 2]) -> f32 {
    match light.kind {
        Kind::Global | Kind::Directional => 1.0,
        Kind::Freeform => {
            if light.points.len() < 3 {
                return 0.0;
            }
            if inside_polygon(p, &light.points) {
                return 1.0;
            }
            let falloff = light.falloff.max(1e-3);
            let dist = dist_to_polygon(p, &light.points);
            if dist >= falloff {
                return 0.0;
            }
            let t = 1.0 - dist / falloff;
            t * t
        }
        Kind::Point | Kind::Spot => {
            let (dx, dy) = (p[0] - light.pos[0], p[1] - light.pos[1]);
            let dist = (dx * dx + dy * dy).sqrt();
            let range = light.range.max(1e-3);
            if dist >= range {
                return 0.0;
            }
            let t = 1.0 - dist / range;
            let mut amount = t * t;
            if light.kind == Kind::Spot && dist > 1e-6 {
                let toward = dy.atan2(dx).to_degrees();
                let mut off = (toward - light.angle).rem_euclid(360.0);
                if off > 180.0 {
                    off = 360.0 - off;
                }
                let half = (light.cone * 0.5).clamp(0.0, 180.0);
                // A soft edge over the outer fifth of the cone.
                let soft = (half * 0.2).max(0.5);
                if off >= half {
                    return 0.0;
                }
                amount *= ((half - off) / soft).min(1.0);
            }
            amount
        }
    }
}

fn light_vector(light: &Light, p: [f32; 2]) -> Option<[f32; 3]> {
    match light.kind {
        Kind::Global => None,
        Kind::Directional => {
            let r = light.angle.to_radians();
            Some([-r.cos(), -r.sin(), 0.5])
        }
        Kind::Freeform => {
            if light.points.len() < 3 {
                return None;
            }
            let c = centroid(&light.points);
            Some([c[0] - p[0], c[1] - p[1], 0.5])
        }
        Kind::Point | Kind::Spot => Some([light.pos[0] - p[0], light.pos[1] - p[1], 0.5]),
    }
}

fn shadowed(light: &Light, p: [f32; 2], blockers: &[Blocker]) -> bool {
    if !light.shadows || light.kind == Kind::Global {
        return false;
    }
    match light.kind {
        Kind::Directional => {
            let r = light.angle.to_radians();
            let toward = [-r.cos(), -r.sin()];
            blockers.iter().any(|b| b.blocks_ray(p, toward))
        }
        Kind::Freeform => {
            if light.points.len() < 3 {
                return false;
            }
            let c = centroid(&light.points);
            blockers.iter().any(|b| b.blocks(c, p))
        }
        _ => blockers.iter().any(|b| b.blocks(light.pos, p)),
    }
}

fn mix_light(sum: &mut [f32; 3], color: [f32; 3], amount: f32, blend: Blend) {
    match blend {
        Blend::Additive => {
            for c in 0..3 {
                sum[c] += color[c] * amount;
            }
        }
        Blend::Multiply => {
            let k = amount.clamp(0.0, 1.0);
            for c in 0..3 {
                sum[c] *= 1.0 + (color[c] - 1.0) * k;
            }
        }
        Blend::Alpha => {
            let k = amount.clamp(0.0, 1.0);
            for c in 0..3 {
                sum[c] = sum[c] * (1.0 - k) + color[c] * k;
            }
        }
    }
}

/// One light's colour and strength at `p`, before it is mixed in.
/// `None` when the light does not reach `p`.
pub fn sample(
    light: &Light,
    p: [f32; 2],
    normal: Option<[f32; 3]>,
    blockers: &[Blocker],
) -> Option<([f32; 3], f32)> {
    let mut amount = reach(light, p) * light.intensity.max(0.0);
    if amount <= 0.0 {
        return None;
    }
    if let Some(n) = normal {
        if let Some(l) = light_vector(light, p) {
            let len = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt().max(1e-6);
            let dot = (n[0] * l[0] + n[1] * l[1] + n[2] * l[2]) / len;
            amount *= dot.max(0.0);
            if amount <= 0.0 {
                return None;
            }
        }
    }
    if shadowed(light, p, blockers) {
        return None;
    }
    Some((light.color, amount))
}

/// Light arriving at point `p` from all `lights`, plus `ambient`. `normal`
/// is the surface normal from a normal map (x right, y up, z toward the
/// viewer); None treats the surface as facing the viewer and fully lit.
pub fn light_at(
    p: [f32; 2],
    normal: Option<[f32; 3]>,
    lights: &[Light],
    blockers: &[Blocker],
    ambient: [f32; 3],
) -> [f32; 3] {
    let mut sum = ambient;
    for light in lights {
        let Some((color, amount)) = sample(light, p, normal, blockers) else { continue };
        mix_light(&mut sum, color, amount, light.blend);
    }
    sum
}

/// Like [`light_at`], but each light's colour is whatever `respond` returns.
/// `respond` gets the surface colour, the light colour, the light's strength,
/// the surface normal and how much the normal faces the light.
pub fn light_respond(
    p: [f32; 2],
    normal: Option<[f32; 3]>,
    lights: &[Light],
    blockers: &[Blocker],
    ambient: [f32; 3],
    albedo: [f32; 3],
    respond: &impl Fn([f32; 3], [f32; 3], f32, [f32; 3], f32) -> [f32; 3],
) -> [f32; 3] {
    let mut sum = ambient;
    let n = normal.unwrap_or([0.0, 0.0, 1.0]);
    for light in lights {
        let Some((color, amount)) = sample(light, p, normal, blockers) else {
            continue;
        };
        let toward = light_vector(light, p).unwrap_or([0.0, 0.0, 1.0]);
        let len = (toward[0] * toward[0] + toward[1] * toward[1] + toward[2] * toward[2])
            .sqrt()
            .max(1e-6);
        let dot = (n[0] * toward[0] + n[1] * toward[1] + n[2] * toward[2]) / len;
        let response = respond(albedo, color, amount, n, dot);
        mix_light(&mut sum, response, 1.0, light.blend);
    }
    sum
}

/// Decodes a normal-map pixel (0..255 per channel, 128 = flat) into a unit normal.
pub fn decode_normal(rgb: [u8; 3]) -> [f32; 3] {
    let n = [rgb[0] as f32 / 127.5 - 1.0, rgb[1] as f32 / 127.5 - 1.0, rgb[2] as f32 / 127.5 - 1.0];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < 1e-6 {
        [0.0, 0.0, 1.0]
    } else {
        [n[0] / len, n[1] / len, n[2] / len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(pos: [f32; 2]) -> Light {
        Light {
            kind: Kind::Point,
            pos,
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            range: 4.0,
            angle: 0.0,
            cone: 60.0,
            shadows: true,
            blend: Blend::Additive,
            falloff: 1.0,
            points: Vec::new(),
        }
    }

    #[test]
    fn directional_light_casts_a_long_shadow() {
        let mut sun = point([0.0, 0.0]);
        sun.kind = Kind::Directional;
        sun.angle = 0.0;
        sun.range = 1.0;
        let wall = Blocker { center: [2.0, 0.0], half: [0.25, 1.0] };
        let lit = light_at([-4.0, 0.0], None, &[sun.clone()], &[wall], [0.0; 3])[0];
        assert!(lit > 0.9, "upstream of the wall stays lit: {lit}");
        assert_eq!(light_at([3.0, 0.0], None, &[sun.clone()], &[wall], [0.0; 3]), [0.0; 3]);
        assert_eq!(light_at([80.0, 0.0], None, &[sun.clone()], &[wall], [0.0; 3]), [0.0; 3]);
        assert!(light_at([80.0, 4.0], None, &[sun], &[wall], [0.0; 3])[0] > 0.9);
    }

    #[test]
    fn freeform_light_fills_its_polygon_and_falls_off() {
        let mut area = point([0.0, 0.0]);
        area.kind = Kind::Freeform;
        area.falloff = 2.0;
        area.shadows = false;
        area.points = vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
        let inside = light_at([0.0, 0.0], None, &[area.clone()], &[], [0.0; 3])[0];
        let edge = light_at([2.0, 0.0], None, &[area.clone()], &[], [0.0; 3])[0];
        let far = light_at([4.0, 0.0], None, &[area], &[], [0.0; 3])[0];
        assert!(inside > 0.9, "{inside}");
        assert!(edge > 0.0 && edge < inside, "outside the edge is dimmer: {edge} vs {inside}");
        assert_eq!(far, 0.0);
    }

    #[test]
    fn blend_modes_mix_differently() {
        let mut lamp = point([0.0, 0.0]);
        lamp.kind = Kind::Global;
        lamp.intensity = 1.0;
        lamp.color = [0.0, 0.0, 0.0];
        lamp.blend = Blend::Multiply;
        let dark = light_at([0.0, 0.0], None, &[lamp.clone()], &[], [1.0, 0.5, 0.25]);
        assert_eq!(dark, [0.0, 0.0, 0.0]);
        lamp.blend = Blend::Alpha;
        lamp.color = [1.0, 0.0, 0.0];
        lamp.intensity = 0.5;
        let mixed = light_at([0.0, 0.0], None, &[lamp], &[], [0.0, 0.0, 1.0]);
        assert!((mixed[0] - 0.5).abs() < 1e-4, "{mixed:?}");
        assert!((mixed[2] - 0.5).abs() < 1e-4, "{mixed:?}");
    }

    #[test]
    fn point_light_fades_to_zero_at_its_range() {
        let l = [point([0.0, 0.0])];
        let near = light_at([0.5, 0.0], None, &l, &[], [0.0; 3])[0];
        let far = light_at([3.0, 0.0], None, &l, &[], [0.0; 3])[0];
        assert!(near > far && far > 0.0);
        assert_eq!(light_at([4.5, 0.0], None, &l, &[], [0.0; 3]), [0.0; 3]);
    }

    #[test]
    fn spot_light_only_lights_its_cone() {
        let mut s = point([0.0, 0.0]);
        s.kind = Kind::Spot;
        s.angle = 90.0;
        let lights = [s];
        let up = light_at([0.0, 1.0], None, &lights, &[], [0.0; 3])[0];
        let side = light_at([1.0, 0.0], None, &lights, &[], [0.0; 3])[0];
        assert!(up > 0.3);
        assert_eq!(side, 0.0);
    }

    #[test]
    fn boxes_cast_shadows_but_not_on_themselves() {
        let wall = Blocker { center: [2.0, 0.0], half: [0.25, 1.0] };
        let l = [point([0.0, 0.0])];
        assert_eq!(light_at([3.0, 0.0], None, &l, &[wall], [0.0; 3]), [0.0; 3]);
        assert!(light_at([3.0, 2.0], None, &l, &[wall], [0.0; 3])[0] > 0.0);
        assert!(light_at([1.9, 0.0], None, &l, &[wall], [0.0; 3])[0] > 0.0);
    }

    #[test]
    fn global_light_and_ambient_reach_everywhere() {
        let mut g = point([0.0, 0.0]);
        g.kind = Kind::Global;
        g.intensity = 0.5;
        g.color = [1.0, 0.0, 0.0];
        let wall = Blocker { center: [2.0, 0.0], half: [0.25, 1.0] };
        let lights = [g];
        let at = light_at([99.0, 0.0], None, &lights, &[wall], [0.1, 0.1, 0.1]);
        assert_eq!(at, [0.6, 0.1, 0.1]);
    }

    #[test]
    fn normal_maps_darken_faces_turned_away() {
        let l = [point([-1.0, 0.0])];
        let toward = light_at([0.0, 0.0], Some([-1.0, 0.0, 0.0]), &l, &[], [0.0; 3])[0];
        let away = light_at([0.0, 0.0], Some([1.0, 0.0, 0.0]), &l, &[], [0.0; 3])[0];
        assert!(toward > 0.3);
        assert_eq!(away, 0.0);
        assert_eq!(decode_normal([128, 128, 255])[2] > 0.99, true);
    }
}
