//! Change advanced particle effect fields from the Asset Editor and Inspector.

use seal_world::particles::{Burst, EmitShape, EmitterDef, FlipbookMode, RendererMode};

use crate::error::CoreError;

pub fn set_effect_field(def: &mut EmitterDef, key: &str, value: &str) -> Result<(), CoreError> {
    let value = value.trim();
    if key == "burst.add" {
        let (t, c) = value
            .split_once(',')
            .or_else(|| value.split_once(' '))
            .ok_or_else(|| CoreError::new("burst needs time and count"))?;
        let time: f32 = t.parse().map_err(|_| CoreError::new("burst time must be a number"))?;
        let count: u32 =
            c.parse().map_err(|_| CoreError::new("burst count must be a whole number"))?;
        def.extras.bursts.push(Burst { time: time.max(0.0), count });
        def.extras.bursts.sort_by(|a, b| a.time.total_cmp(&b.time));
        return Ok(());
    }
    if key == "burst.clear" {
        def.extras.bursts.clear();
        return Ok(());
    }
    if key == "burst.remove" {
        let index: usize =
            value.parse().map_err(|_| CoreError::new("burst remove needs an index"))?;
        if index < def.extras.bursts.len() {
            def.extras.bursts.remove(index);
        }
        return Ok(());
    }
    if key == "burst.set" {
        let (index_s, rest) = value
            .split_once(',')
            .ok_or_else(|| CoreError::new("burst set needs index, time and count"))?;
        let (time_s, count_s) = rest
            .split_once(',')
            .or_else(|| rest.split_once(' '))
            .ok_or_else(|| CoreError::new("burst set needs index, time and count"))?;
        let index: usize =
            index_s.parse().map_err(|_| CoreError::new("burst index must be a whole number"))?;
        let time: f32 =
            time_s.parse().map_err(|_| CoreError::new("burst time must be a number"))?;
        let count: u32 =
            count_s.parse().map_err(|_| CoreError::new("burst count must be a whole number"))?;
        let burst = Burst { time: time.max(0.0), count };
        if index < def.extras.bursts.len() {
            def.extras.bursts[index] = burst;
        } else {
            return Err(CoreError::new("burst index is out of range"));
        }
        def.extras.bursts.sort_by(|a, b| a.time.total_cmp(&b.time));
        return Ok(());
    }
    match key {
        "burst_repeat" => {
            let v: f32 =
                value.parse().map_err(|_| CoreError::new("burst repeat must be a number"))?;
            def.extras.burst_repeat = v.max(0.0);
        }
        "shape" => def.extras.shape = parse_shape(value)?,
        "drag" => {
            let v: f32 = value.parse().map_err(|_| CoreError::new("drag must be a number"))?;
            def.extras.drag = v.max(0.0);
        }
        "noise_strength" => {
            let v: f32 =
                value.parse().map_err(|_| CoreError::new("noise strength must be a number"))?;
            def.extras.noise_strength = v.max(0.0);
        }
        "noise_frequency" => {
            let v: f32 =
                value.parse().map_err(|_| CoreError::new("noise frequency must be a number"))?;
            def.extras.noise_frequency = v.max(0.001);
        }
        "orbital" => {
            let v: f32 = value.parse().map_err(|_| CoreError::new("orbital must be a number"))?;
            def.extras.orbital = v;
        }
        "rotation_start.min" => {
            let v: f32 =
                value.parse().map_err(|_| CoreError::new("rotation start min must be a number"))?;
            def.extras.rotation_start.0 = v;
        }
        "rotation_start.max" => {
            let v: f32 =
                value.parse().map_err(|_| CoreError::new("rotation start max must be a number"))?;
            def.extras.rotation_start.1 = v;
        }
        "rotation_rate.min" => {
            let v: f32 =
                value.parse().map_err(|_| CoreError::new("rotation rate min must be a number"))?;
            def.extras.rotation_rate.0 = v;
        }
        "rotation_rate.max" => {
            let v: f32 =
                value.parse().map_err(|_| CoreError::new("rotation rate max must be a number"))?;
            def.extras.rotation_rate.1 = v;
        }
        "renderer" => def.extras.renderer = parse_renderer(value)?,
        "soft_fade" => {
            let v: f32 = value.parse().map_err(|_| CoreError::new("soft fade must be a number"))?;
            def.extras.soft_fade = v.max(0.0);
        }
        "billboard_material" => {
            let mut words = value.split_whitespace();
            let sheet = words
                .next()
                .ok_or_else(|| CoreError::new("billboard material needs a sheet path"))?
                .to_string();
            let cols: u32 = words
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .ok_or_else(|| CoreError::new("billboard material needs column count"))?
                .max(1);
            let rows: u32 = words.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1).max(1);
            let r#loop = words.next() != Some("once");
            def.extras.billboard_material =
                Some(seal_world::particles::BillboardAnimMaterial { sheet, cols, rows, r#loop });
        }
        "lit" => def.extras.lit = matches!(value, "1" | "true" | "on" | "yes"),
        _ => return Err(CoreError::new(format!("unknown effect setting {key}"))),
    }
    Ok(())
}

fn parse_shape(value: &str) -> Result<EmitShape, CoreError> {
    let mut parts = value.split_whitespace().peekable();
    let kind = parts.next().unwrap_or("point");
    if kind == "mesh" {
        let path = parts.next().unwrap_or("cube").to_string();
        let mode = match parts.next() {
            Some("vertices") => seal_world::particles::MeshSampleMode::Vertices,
            _ => seal_world::particles::MeshSampleMode::Surface,
        };
        return Ok(EmitShape::Mesh { path, mode });
    }
    if kind == "sphere" {
        let r = parts.next().and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.5_f32).max(0.0_f32);
        let vol = match parts.next() {
            Some("surface") => seal_world::particles::SphereVolume::Surface,
            _ => seal_world::particles::SphereVolume::Volume,
        };
        return Ok(EmitShape::Sphere(r, vol));
    }
    let nums: Result<Vec<f32>, _> = parts.map(str::parse).collect();
    let nums = nums.map_err(|_| CoreError::new("shape needs numbers"))?;
    Ok(match kind {
        "point" => EmitShape::Point,
        "circle" if nums.len() == 1 => EmitShape::Circle(nums[0].max(0.0)),
        "ring" if nums.len() == 2 => EmitShape::Ring(nums[0].max(0.0), nums[1].max(nums[0])),
        "box" if nums.len() == 2 => EmitShape::Box([nums[0].max(0.0), nums[1].max(0.0)]),
        "edge" if nums.len() == 4 => EmitShape::Edge([nums[0], nums[1], nums[2], nums[3]]),
        "cone" if nums.len() == 2 => EmitShape::Cone(nums[0], nums[1].max(0.0)),
        other => return Err(CoreError::new(format!("unknown emit shape {other}"))),
    })
}

fn parse_renderer(value: &str) -> Result<RendererMode, CoreError> {
    let mut parts = value.split_whitespace();
    let mode = parts.next().unwrap_or("billboard");
    let tail: Vec<&str> = parts.collect();
    Ok(match mode {
        "billboard" => RendererMode::Billboard,
        "stretched" if tail.len() == 1 => RendererMode::Stretched(
            tail[0].parse().map_err(|_| CoreError::new("stretch factor must be a number"))?,
        ),
        "flipbook" if tail.len() == 2 => RendererMode::Flipbook {
            path: tail[0].to_string(),
            mode: if tail[1] == "random" { FlipbookMode::Random } else { FlipbookMode::OverLife },
        },
        "trail" if tail.len() == 2 => RendererMode::Trail {
            points: tail[0]
                .parse()
                .map_err(|_| CoreError::new("trail points must be a whole number"))?,
            width_end: tail[1]
                .parse()
                .map_err(|_| CoreError::new("trail width end must be a number"))?,
        },
        "mesh" if !tail.is_empty() => RendererMode::Mesh { path: tail.join(" ") },
        "mesh" => RendererMode::Mesh { path: "cube".into() },
        other => return Err(CoreError::new(format!("unknown renderer {other}"))),
    })
}
