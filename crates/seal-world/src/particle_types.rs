//! Extra particle effect settings: bursts, emit shapes, motion and renderer modes.

use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq)]
pub struct Burst {
    pub time: f32,
    pub count: u32,
}

/// False = fill volume; true = only the outer shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SphereVolume {
    #[default]
    Volume,
    Surface,
}

/// Where on a mesh to spawn: triangle surfaces or vertex points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MeshSampleMode {
    #[default]
    Surface,
    Vertices,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EmitShape {
    Point,
    Circle(f32),
    Ring(f32, f32),
    Box([f32; 2]),
    Edge([f32; 4]),
    Cone(f32, f32),
    Sphere(f32, SphereVolume),
    Mesh { path: String, mode: MeshSampleMode },
}

impl Default for EmitShape {
    fn default() -> Self {
        EmitShape::Point
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlipbookMode {
    OverLife,
    Random,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RendererMode {
    Billboard,
    Stretched(f32),
    Flipbook { path: String, mode: FlipbookMode },
    Trail { points: usize, width_end: f32 },
    Mesh { path: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BillboardAnimMaterial {
    pub sheet: String,
    pub cols: u32,
    pub rows: u32,
    pub r#loop: bool,
}

impl Default for RendererMode {
    fn default() -> Self {
        RendererMode::Billboard
    }
}

/// Advanced fields stored on an effect; defaults match the original billboard point emitter.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleExtras {
    pub bursts: Vec<Burst>,
    pub burst_repeat: f32,
    pub shape: EmitShape,
    pub velocity_curve: Option<String>,
    pub drag: f32,
    pub noise_strength: f32,
    pub noise_frequency: f32,
    pub orbital: f32,
    pub rotation_start: (f32, f32),
    pub rotation_rate: (f32, f32),
    pub rotation_curve: Option<String>,
    pub renderer: RendererMode,
    pub lit: bool,
    /// World units: fade alpha when depth is within this distance behind opaque draws.
    pub soft_fade: f32,
    /// Sheet animation on billboard particles (2D material `particle_anim`).
    pub billboard_material: Option<BillboardAnimMaterial>,
    /// Keys of optional lines that were damaged while loading; sampling uses defaults.
    pub damaged: Vec<String>,
}

impl Default for ParticleExtras {
    fn default() -> Self {
        Self {
            bursts: Vec::new(),
            burst_repeat: 0.0,
            shape: EmitShape::Point,
            velocity_curve: None,
            drag: 0.0,
            noise_strength: 0.0,
            noise_frequency: 1.0,
            orbital: 0.0,
            rotation_start: (0.0, 0.0),
            rotation_rate: (0.0, 0.0),
            rotation_curve: None,
            renderer: RendererMode::Billboard,
            lit: false,
            soft_fade: 0.0,
            billboard_material: None,
            damaged: Vec::new(),
        }
    }
}

impl ParticleExtras {
    pub fn mark_damaged(&mut self, key: &str) {
        if !self.damaged.iter().any(|k| k == key) {
            self.damaged.push(key.to_string());
        }
    }

    pub fn append_optional_text(&self, out: &mut String) {
        for b in &self.bursts {
            let _ = writeln!(out, "burst {} {}", b.time, b.count);
        }
        if self.burst_repeat > 0.0 {
            let _ = writeln!(out, "burst_repeat {}", self.burst_repeat);
        }
        match &self.shape {
            EmitShape::Point => {}
            EmitShape::Circle(r) => {
                let _ = writeln!(out, "shape circle {r}");
            }
            EmitShape::Ring(a, b) => {
                let _ = writeln!(out, "shape ring {a} {b}");
            }
            EmitShape::Box(h) => {
                let _ = writeln!(out, "shape box {} {}", h[0], h[1]);
            }
            EmitShape::Edge(e) => {
                let _ = writeln!(out, "shape edge {} {} {} {}", e[0], e[1], e[2], e[3]);
            }
            EmitShape::Cone(dir, half) => {
                let _ = writeln!(out, "shape cone {dir} {half}");
            }
            EmitShape::Sphere(r, vol) => {
                let tag = match vol {
                    SphereVolume::Volume => "volume",
                    SphereVolume::Surface => "surface",
                };
                let _ = writeln!(out, "shape sphere {r} {tag}");
            }
            EmitShape::Mesh { path, mode } => {
                let tag = match mode {
                    MeshSampleMode::Surface => "surface",
                    MeshSampleMode::Vertices => "vertices",
                };
                let _ = writeln!(out, "shape mesh {path} {tag}");
            }
        }
        if let Some(t) = &self.velocity_curve {
            let _ = writeln!(out, "velocity_curve {t}");
        }
        if self.drag > 0.0 {
            let _ = writeln!(out, "drag {}", self.drag);
        }
        if self.noise_strength > 0.0 {
            let _ =
                writeln!(out, "noise {} {}", self.noise_strength, self.noise_frequency.max(0.001));
        }
        if self.orbital.abs() > 1e-6 {
            let _ = writeln!(out, "orbital {}", self.orbital);
        }
        if self.rotation_start != (0.0, 0.0) {
            let _ = writeln!(out, "rotation {} {}", self.rotation_start.0, self.rotation_start.1);
        }
        if self.rotation_rate != (0.0, 0.0) {
            let _ =
                writeln!(out, "rotation_rate {} {}", self.rotation_rate.0, self.rotation_rate.1);
        }
        if let Some(t) = &self.rotation_curve {
            let _ = writeln!(out, "rotation_curve {t}");
        }
        match &self.renderer {
            RendererMode::Billboard => {}
            RendererMode::Stretched(f) => {
                let _ = writeln!(out, "renderer stretched {f}");
            }
            RendererMode::Flipbook { path, mode } => {
                let tag = match mode {
                    FlipbookMode::OverLife => "life",
                    FlipbookMode::Random => "random",
                };
                let _ = writeln!(out, "renderer flipbook {path} {tag}");
            }
            RendererMode::Trail { points, width_end } => {
                let _ = writeln!(out, "renderer trail {points} {width_end}");
            }
            RendererMode::Mesh { path } => {
                let _ = writeln!(out, "renderer mesh {path}");
            }
        }
        if self.lit {
            let _ = writeln!(out, "lit 1");
        }
        if self.soft_fade > 0.0 {
            let _ = writeln!(out, "soft_fade {}", self.soft_fade);
        }
        if let Some(m) = &self.billboard_material {
            let tag = if m.r#loop { "loop" } else { "once" };
            let _ = writeln!(out, "billboard_material {} {} {} {tag}", m.sheet, m.cols.max(1), m.rows.max(1));
        }
    }

    pub fn parse_optional_line(
        &mut self,
        key: &str,
        nums: &[f32],
        rest: &str,
        line_no: usize,
    ) -> Result<(), String> {
        match key {
            "burst" => {
                if nums.len() != 2 {
                    self.mark_damaged("burst");
                    return Ok(());
                }
                self.bursts.push(Burst { time: nums[0].max(0.0), count: nums[1].max(0.0) as u32 });
            }
            "burst_repeat" => {
                if nums.len() != 1 {
                    self.mark_damaged("burst_repeat");
                    return Ok(());
                }
                self.burst_repeat = nums[0].max(0.0);
            }
            "shape" => {
                let mut words = rest.split_whitespace().peekable();
                let kind = words.next().unwrap_or("");
                if kind == "mesh" {
                    let path = words.next().unwrap_or("cube").to_string();
                    let mode = match words.next() {
                        Some("vertices") => MeshSampleMode::Vertices,
                        _ => MeshSampleMode::Surface,
                    };
                    self.shape = EmitShape::Mesh { path, mode };
                } else if kind == "sphere" {
                    let r: f32 = words
                        .next()
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(0.0_f32)
                        .max(0.0_f32);
                    let vol = match words.next() {
                        Some("surface") => SphereVolume::Surface,
                        _ => SphereVolume::Volume,
                    };
                    self.shape = EmitShape::Sphere(r, vol);
                } else {
                    let tail: Result<Vec<f32>, _> = words.map(str::parse).collect();
                    let tail = tail.map_err(|_| format!("line {line_no}: shape needs numbers"))?;
                    self.shape = match kind {
                        "point" if tail.is_empty() => EmitShape::Point,
                        "circle" if tail.len() == 1 => EmitShape::Circle(tail[0].max(0.0)),
                        "ring" if tail.len() == 2 => {
                            EmitShape::Ring(tail[0].max(0.0), tail[1].max(tail[0]))
                        }
                        "box" if tail.len() == 2 => {
                            EmitShape::Box([tail[0].max(0.0), tail[1].max(0.0)])
                        }
                        "edge" if tail.len() == 4 => {
                            EmitShape::Edge([tail[0], tail[1], tail[2], tail[3]])
                        }
                        "cone" if tail.len() == 2 => EmitShape::Cone(tail[0], tail[1].max(0.0)),
                        _ => {
                            self.mark_damaged("shape");
                            return Ok(());
                        }
                    };
                }
            }
            "velocity_curve" => {
                if rest.is_empty() {
                    self.mark_damaged("velocity_curve");
                    return Ok(());
                }
                self.velocity_curve = Some(rest.to_string());
            }
            "drag" => {
                if nums.len() != 1 {
                    self.mark_damaged("drag");
                    return Ok(());
                }
                self.drag = nums[0].max(0.0);
            }
            "noise" => {
                if nums.len() != 2 {
                    self.mark_damaged("noise");
                    return Ok(());
                }
                self.noise_strength = nums[0].max(0.0);
                self.noise_frequency = nums[1].max(0.001);
            }
            "orbital" => {
                if nums.len() != 1 {
                    self.mark_damaged("orbital");
                    return Ok(());
                }
                self.orbital = nums[0];
            }
            "rotation" => {
                if nums.len() != 2 {
                    self.mark_damaged("rotation");
                    return Ok(());
                }
                self.rotation_start = (nums[0], nums[1]);
            }
            "rotation_rate" => {
                if nums.len() != 2 {
                    self.mark_damaged("rotation_rate");
                    return Ok(());
                }
                self.rotation_rate = (nums[0], nums[1]);
            }
            "rotation_curve" => {
                if rest.is_empty() {
                    self.mark_damaged("rotation_curve");
                    return Ok(());
                }
                self.rotation_curve = Some(rest.to_string());
            }
            "renderer" => {
                let mut words = rest.split_whitespace();
                let mode = words.next().unwrap_or("");
                let tail: Vec<&str> = words.collect();
                self.renderer = match mode {
                    "billboard" if tail.is_empty() => RendererMode::Billboard,
                    "stretched" if tail.len() == 1 => {
                        RendererMode::Stretched(tail[0].parse::<f32>().unwrap_or(1.0).max(0.0))
                    }
                    "flipbook" if tail.len() == 2 => RendererMode::Flipbook {
                        path: tail[0].to_string(),
                        mode: if tail[1] == "random" {
                            FlipbookMode::Random
                        } else {
                            FlipbookMode::OverLife
                        },
                    },
                    "trail" if tail.len() == 2 => RendererMode::Trail {
                        points: tail[0].parse::<usize>().unwrap_or(8).max(2),
                        width_end: tail[1].parse::<f32>().unwrap_or(0.2).max(0.0),
                    },
                    "mesh" if !tail.is_empty() => RendererMode::Mesh {
                        path: tail.join(" "),
                    },
                    "mesh" if tail.is_empty() => RendererMode::Mesh { path: "cube".into() },
                    _ => {
                        self.mark_damaged("renderer");
                        return Ok(());
                    }
                };
            }
            "soft_fade" => {
                if nums.len() != 1 {
                    self.mark_damaged("soft_fade");
                    return Ok(());
                }
                self.soft_fade = nums[0].max(0.0);
            }
            "billboard_material" => {
                let mut words = rest.split_whitespace();
                let sheet = words.next().unwrap_or("").to_string();
                let cols: u32 = words.next().and_then(|s| s.parse().ok()).unwrap_or(1).max(1);
                let rows: u32 = words.next().and_then(|s| s.parse().ok()).unwrap_or(1).max(1);
                let r#loop = words.next() != Some("once");
                if sheet.is_empty() {
                    self.mark_damaged("billboard_material");
                    return Ok(());
                }
                self.billboard_material = Some(BillboardAnimMaterial { sheet, cols, rows, r#loop });
            }
            "lit" => {
                if nums.len() != 1 {
                    self.mark_damaged("lit");
                    return Ok(());
                }
                self.lit = nums[0] >= 0.5;
            }
            _ => return Err(format!("line {line_no}: unknown key {key}")),
        }
        Ok(())
    }
}
