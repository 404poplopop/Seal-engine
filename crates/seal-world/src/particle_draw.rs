//! Draw descriptions for live particles (viewport and tests).

use std::path::Path;

use super::particle_flipbook::{frame_count, frame_picture};
use super::particle_types::{FlipbookMode, RendererMode};
use super::Emitter;

fn anim_frame_index(cols: u32, rows: u32, life_u: f32, r#loop: bool) -> u32 {
    let total = cols.max(1) * rows.max(1);
    if total <= 1 {
        return 0;
    }
    let u = life_u.clamp(0.0, 1.0);
    let idx = (u * total as f32).floor() as u32;
    if r#loop {
        idx.min(total - 1)
    } else {
        idx.min(total - 1)
    }
}

fn sheet_uv(cols: u32, rows: u32, index: u32) -> [f32; 4] {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let total = cols * rows;
    let i = index.min(total.saturating_sub(1));
    let col = i % cols;
    let row = i / cols;
    let du = 1.0 / cols as f32;
    let dv = 1.0 / rows as f32;
    [col as f32 * du, row as f32 * dv, (col + 1) as f32 * du, (row + 1) as f32 * dv]
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColorDraw {
    pub rgba: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParticleDraw {
    Billboard {
        pos: [f32; 3],
        half: f32,
        color: ColorDraw,
        lit: bool,
    },
    Stretched {
        pos: [f32; 3],
        half: f32,
        angle: f32,
        stretch: f32,
        color: ColorDraw,
        lit: bool,
    },
    Flipbook {
        pos: [f32; 3],
        half: f32,
        angle: f32,
        frame_index: u32,
        picture: String,
        uv: [f32; 4],
        color: ColorDraw,
        lit: bool,
    },
    Trail {
        points: Vec<[f32; 3]>,
        widths: Vec<f32>,
        colors: Vec<ColorDraw>,
        lit: bool,
    },
    Mesh {
        pos: [f32; 3],
        half: f32,
        angle: f32,
        mesh: String,
        color: ColorDraw,
        lit: bool,
    },
}

/// Build draw items for every visible particle in `emitter`.
pub fn collect_draws(emitter: &Emitter, project: Option<&Path>) -> Vec<ParticleDraw> {
    collect_draws_depth(emitter, project, &[])
}

/// Like [`collect_draws`], with opaque depths (same axis as particle `pos[2]`) for soft fading.
pub fn collect_draws_depth(
    emitter: &Emitter,
    project: Option<&Path>,
    occluder_depths: &[f32],
) -> Vec<ParticleDraw> {
    let lit = emitter.def.extras.lit;
    let fade = emitter.def.extras.soft_fade;
    let mut out = Vec::new();
    for p in &emitter.particles {
        let (size, rgba) = emitter.look(p);
        if size <= 0.0 || rgba[3] <= 0.02 {
            continue;
        }
        let mut rgba = rgba;
        if fade > 0.0 {
            let f = super::particle_soft::soft_fade_factor(p.pos[2], occluder_depths, fade);
            rgba[3] *= f;
        }
        let color = ColorDraw { rgba };
        let half = (size * 0.5).max(0.01);
        match &emitter.def.extras.renderer {
            RendererMode::Billboard => {
                if let Some(mat) = &emitter.def.extras.billboard_material {
                    let u = emitter.life_u(p);
                    let frame = anim_frame_index(mat.cols, mat.rows, u, mat.r#loop);
                    let uv = sheet_uv(mat.cols, mat.rows, frame);
                    out.push(ParticleDraw::Flipbook {
                        pos: p.pos,
                        half,
                        angle: p.angle,
                        frame_index: frame,
                        picture: mat.sheet.clone(),
                        uv,
                        color,
                        lit,
                    });
                } else {
                    out.push(ParticleDraw::Billboard {
                        pos: p.pos,
                        half,
                        color,
                        lit,
                    });
                }
            }
            RendererMode::Stretched(factor) => {
                let speed =
                    (p.vel[0] * p.vel[0] + p.vel[1] * p.vel[1] + p.vel[2] * p.vel[2]).sqrt();
                let stretch =
                    if speed > 1e-5 { (half + speed * factor.max(0.0)).max(half) } else { half };
                let angle = p.vel[1].atan2(p.vel[0]);
                out.push(ParticleDraw::Stretched { pos: p.pos, half, angle, stretch, color, lit })
            }
            RendererMode::Flipbook { path, mode } => {
                let u = emitter.life_u(p);
                let n = frame_count(path, project).max(1);
                let frame = match mode {
                    FlipbookMode::Random => p.flip_frame % n,
                    FlipbookMode::OverLife => {
                        if n <= 1 {
                            0
                        } else {
                            ((u * n as f32).floor() as u32).min(n - 1)
                        }
                    }
                };
                let (picture, uv) = frame_picture(path, frame, project);
                out.push(ParticleDraw::Flipbook {
                    pos: p.pos,
                    half,
                    angle: p.angle,
                    frame_index: frame,
                    picture,
                    uv,
                    color,
                    lit,
                });
            }
            RendererMode::Trail { width_end, .. } => {
                if p.trail.len() < 2 {
                    out.push(ParticleDraw::Billboard {
                        pos: p.pos,
                        half,
                        color: color.clone(),
                        lit,
                    });
                    continue;
                }
                let n = p.trail.len();
                let mut widths = Vec::with_capacity(n);
                let mut colors = Vec::with_capacity(n);
                for (i, _) in p.trail.iter().enumerate() {
                    let t = i as f32 / (n as f32 - 1.0).max(1.0);
                    widths.push(half * (1.0 - t) + half * width_end * t);
                    let mut c = color.rgba;
                    c[3] *= 1.0 - t * 0.85;
                    colors.push(ColorDraw { rgba: c });
                }
                out.push(ParticleDraw::Trail { points: p.trail.clone(), widths, colors, lit });
            }
            RendererMode::Mesh { path } => {
                out.push(ParticleDraw::Mesh {
                    pos: p.pos,
                    half,
                    angle: p.angle,
                    mesh: path.clone(),
                    color,
                    lit,
                });
            }
        }
    }
    out
}
