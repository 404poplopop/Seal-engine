//! A post-process volume blends glow, tint, exposure and saturation while the camera is inside.
//!
//! The box is the outer edge. `blend` is the width of the fade just inside that edge.
//! Outside the box the weight is 0. Deep inside, farther from the edge than `blend`, it is 1.
//! Higher `priority` is applied last, so it wins where volumes overlap.

use crate::Entity;

/// Settings used only when [`crate::volumes::Volume::kind`] is `post`.
#[derive(Clone, Debug, PartialEq)]
pub struct PostVolume {
    pub glow_threshold: f32,
    pub glow_intensity: f32,
    /// Colour multiplied into the picture. White leaves it unchanged.
    pub tint: [f32; 3],
    /// 1 leaves brightness unchanged.
    pub exposure: f32,
    /// 1 leaves colour unchanged. 0 is grey.
    pub saturation: f32,
    /// Width of the fade inside the box edge, in scene units.
    pub blend: f32,
    /// Higher values are applied after lower ones.
    pub priority: i32,
}

impl Default for PostVolume {
    fn default() -> Self {
        Self {
            glow_threshold: 0.8,
            glow_intensity: 1.0,
            tint: [1.0, 1.0, 1.0],
            exposure: 1.0,
            saturation: 1.0,
            blend: 1.0,
            priority: 0,
        }
    }
}

seal_ecs::text_component!(PostVolume as "post_volume" {
    glow_threshold,
    glow_intensity,
    tint,
    exposure,
    saturation,
    blend,
    priority
});

pub const KEYS: &[&str] = &[
    "post_glow_threshold",
    "post_glow_intensity",
    "post_tint",
    "post_exposure",
    "post_saturation",
    "post_blend",
    "post_priority",
];

pub const NUMERIC: &[&str] = &[
    "post_glow_threshold",
    "post_glow_intensity",
    "post_exposure",
    "post_saturation",
    "post_blend",
    "post_priority",
];

fn num(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".into()
    } else {
        trimmed.into()
    }
}

pub fn fields(post: &PostVolume) -> Vec<(String, String)> {
    vec![
        ("post_glow_threshold".into(), num(post.glow_threshold)),
        ("post_glow_intensity".into(), num(post.glow_intensity)),
        ("post_tint".into(), crate::tint_hex(&[post.tint[0], post.tint[1], post.tint[2], 1.0])),
        ("post_exposure".into(), num(post.exposure)),
        ("post_saturation".into(), num(post.saturation)),
        ("post_blend".into(), num(post.blend)),
        ("post_priority".into(), post.priority.to_string()),
    ]
}

fn finite(text: &str) -> Option<f32> {
    text.trim().parse::<f32>().ok().filter(|n| n.is_finite())
}

pub fn check(key: &str, value: &str) -> Option<&'static str> {
    let ok = match key {
        "post_glow_threshold" => finite(value).is_some_and(|n| (0.0..=4.0).contains(&n)),
        "post_glow_intensity" => finite(value).is_some_and(|n| (0.0..=8.0).contains(&n)),
        "post_exposure" => finite(value).is_some_and(|n| (0.0..=8.0).contains(&n)),
        "post_saturation" => finite(value).is_some_and(|n| (0.0..=4.0).contains(&n)),
        "post_blend" => finite(value).is_some_and(|n| n >= 0.0),
        "post_priority" => value.trim().parse::<i32>().is_ok(),
        "post_tint" => crate::parse_tint(value.trim()).is_some(),
        _ => return Some("unknown"),
    };
    (!ok).then_some("inspector-volume-invalid")
}

pub fn apply(post: &mut PostVolume, key: &str, value: &str) -> bool {
    if !KEYS.contains(&key) || check(key, value).is_some() {
        return false;
    }
    match key {
        "post_glow_threshold" => post.glow_threshold = finite(value).unwrap_or(post.glow_threshold),
        "post_glow_intensity" => post.glow_intensity = finite(value).unwrap_or(post.glow_intensity),
        "post_exposure" => post.exposure = finite(value).unwrap_or(post.exposure),
        "post_saturation" => post.saturation = finite(value).unwrap_or(post.saturation),
        "post_blend" => post.blend = finite(value).unwrap_or(post.blend),
        "post_priority" => post.priority = value.trim().parse().unwrap_or(post.priority),
        "post_tint" => {
            if let Some(tint) = crate::parse_tint(value.trim()) {
                post.tint = [tint[0], tint[1], tint[2]];
            }
        }
        _ => return false,
    }
    true
}

/// Weight of one post volume at `camera`. 0 outside, 1 in the core, between on the inner fade.
pub fn weight(entity: &Entity, camera: [f32; 2]) -> f32 {
    if entity.hidden || entity.volume.kind != "post" {
        return 0.0;
    }
    let half = [
        entity.phys2d.size[0].abs() * entity.scale[0].abs() * 0.5,
        entity.phys2d.size[1].abs() * entity.scale[1].abs() * 0.5,
    ];
    if half[0] <= 0.0 || half[1] <= 0.0 {
        return 0.0;
    }
    let dx = (camera[0] - entity.translation[0]).abs();
    let dy = (camera[1] - entity.translation[1]).abs();
    if dx > half[0] || dy > half[1] {
        return 0.0;
    }
    let edge = (half[0] - dx).min(half[1] - dy);
    let band = entity.post.blend.max(0.0);
    if band <= 1e-6 {
        return 1.0;
    }
    (edge / band).clamp(0.0, 1.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// The picture grade after every overlapping post volume. `active` is false when none apply,
/// so the caller can leave the frame untouched.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub active: bool,
    /// Weight of the last volume applied (the highest priority that contains the camera).
    pub weight: f32,
    pub threshold: f32,
    pub intensity: f32,
    pub radius: f32,
    pub tint: [f32; 3],
    pub exposure: f32,
    pub saturation: f32,
}

impl Resolved {
    pub fn off(base: &seal_scene::Glow2d) -> Self {
        Self {
            active: false,
            weight: 0.0,
            threshold: base.threshold,
            intensity: base.intensity,
            radius: base.radius,
            tint: [1.0, 1.0, 1.0],
            exposure: 1.0,
            saturation: 1.0,
        }
    }
}

pub fn resolve(entities: &[Entity], camera: [f32; 2], base: &seal_scene::Glow2d) -> Resolved {
    let mut items: Vec<(i32, f32, usize)> = entities
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let w = weight(e, camera);
            (w > 0.0).then_some((e.post.priority, w, i))
        })
        .collect();
    if items.is_empty() {
        return Resolved {
            active: false,
            weight: 0.0,
            threshold: base.threshold,
            intensity: base.intensity,
            radius: base.radius,
            tint: [1.0, 1.0, 1.0],
            exposure: 1.0,
            saturation: 1.0,
        };
    }
    items.sort_by_key(|(priority, _, index)| (*priority, *index));
    let mut threshold = base.threshold;
    let mut intensity = if base.enabled { base.intensity } else { 0.0 };
    let mut tint = [1.0, 1.0, 1.0];
    let mut exposure = 1.0;
    let mut saturation = 1.0;
    let mut top = 0.0;
    for (_, w, index) in items {
        let post = &entities[index].post;
        threshold = lerp(threshold, post.glow_threshold, w);
        intensity = lerp(intensity, post.glow_intensity, w);
        tint = [
            lerp(tint[0], post.tint[0], w),
            lerp(tint[1], post.tint[1], w),
            lerp(tint[2], post.tint[2], w),
        ];
        exposure = lerp(exposure, post.exposure, w);
        saturation = lerp(saturation, post.saturation, w);
        top = w;
    }
    Resolved {
        active: true,
        weight: top,
        threshold,
        intensity,
        radius: base.radius,
        tint,
        exposure,
        saturation,
    }
}

/// Multiply `rgb` by tint and exposure, then move it toward grey by `saturation`.
/// White tint, exposure 1 and saturation 1 return `rgb`.
pub fn grade_rgb(rgb: [f32; 3], tint: [f32; 3], exposure: f32, saturation: f32) -> [f32; 3] {
    let c = [rgb[0] * tint[0] * exposure, rgb[1] * tint[1] * exposure, rgb[2] * tint[2] * exposure];
    let luma = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
    [
        luma + (c[0] - luma) * saturation,
        luma + (c[1] - luma) * saturation,
        luma + (c[2] - luma) * saturation,
    ]
}
