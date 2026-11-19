//! CPU particle emitters: deterministic spawn, motion and over-life colour/size.

#[path = "particle_draw.rs"]
mod particle_draw;
#[path = "particle_emit_mesh.rs"]
mod particle_emit_mesh;
#[path = "particle_flipbook.rs"]
mod particle_flipbook;
#[path = "particle_soft.rs"]
mod particle_soft;
#[path = "particle_spawn.rs"]
mod particle_spawn;
#[path = "particle_types.rs"]
mod particle_types;

pub use particle_draw::{collect_draws, collect_draws_depth, ParticleDraw};
pub use particle_emit_mesh::{load_emit_mesh, mesh_renderer_primitive, parse_mesh_text, primitive_mesh};
pub use particle_flipbook::{frame_count, frame_picture};
pub use particle_soft::soft_fade_factor;
pub use particle_spawn::{bursts_due, offset_inside_shape, sample_emit_offset, sample_emit_offset_project};
pub use particle_types::{
    BillboardAnimMaterial, Burst, EmitShape, FlipbookMode, MeshSampleMode, ParticleExtras, RendererMode,
    SphereVolume,
};


#[derive(Clone, Debug, PartialEq)]
pub struct EmitterDef {
    /// Particles per second.
    pub rate: f32,
    /// Seconds each particle lives.
    pub lifetime: f32,
    /// Initial velocity; each axis is spread by +/- `spread`.
    pub velocity: [f32; 3],
    pub spread: f32,
    pub gravity: [f32; 3],
    pub size: (f32, f32),
    pub color: ([f32; 4], [f32; 4]),
    /// SealText curve for size over life. Absent keeps the straight fade from `size`.
    pub size_curve: Option<String>,
    /// SealText curve for opacity over life. Absent keeps the straight fade of alpha.
    pub alpha_curve: Option<String>,
    /// SealText gradient for colour over life. Absent keeps the straight fade of `color`.
    pub color_gradient: Option<String>,
    /// Alive cap; spawning pauses at it.
    pub max: usize,
    pub seed: u32,
    pub extras: ParticleExtras,
}

impl Default for EmitterDef {
    fn default() -> Self {
        EmitterDef {
            rate: 10.0,
            lifetime: 1.0,
            velocity: [0.0, 1.0, 0.0],
            spread: 0.0,
            gravity: [0.0, 0.0, 0.0],
            size: (0.1, 0.0),
            color: ([1.0; 4], [1.0, 1.0, 1.0, 0.0]),
            size_curve: None,
            alpha_curve: None,
            color_gradient: None,
            max: 1000,
            seed: 1,
            extras: ParticleExtras::default(),
        }
    }
}

impl EmitterDef {
    /// Parse a `.fx` effect file: one `key values...` per line, `#` comments.
    pub fn parse(text: &str) -> Result<EmitterDef, String> {
        let mut d = EmitterDef::default();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let key = parts.next().unwrap_or("");
            let curve_keys =
                ["size_curve", "alpha_curve", "color_gradient", "velocity_curve", "rotation_curve"];
            if curve_keys.contains(&key) {
                let rest = line[line.find(key).unwrap_or(0) + key.len()..].trim();
                if rest.is_empty() {
                    return Err(format!("line {}: {key} needs a value", n + 1));
                }
                match key {
                    "size_curve" => d.size_curve = Some(rest.to_string()),
                    "alpha_curve" => d.alpha_curve = Some(rest.to_string()),
                    "color_gradient" => d.color_gradient = Some(rest.to_string()),
                    "velocity_curve" => d.extras.velocity_curve = Some(rest.to_string()),
                    "rotation_curve" => d.extras.rotation_curve = Some(rest.to_string()),
                    _ => {}
                }
                continue;
            }
            let optional_keys = [
                "burst",
                "burst_repeat",
                "shape",
                "drag",
                "noise",
                "orbital",
                "rotation",
                "rotation_rate",
                "renderer",
                "lit",
                "soft_fade",
                "billboard_material",
            ];
            let rest = line[line.find(key).unwrap_or(0) + key.len()..].trim();
            if optional_keys.contains(&key) {
                let nums: Vec<f32> = if matches!(key, "shape" | "renderer") {
                    Vec::new()
                } else {
                    let mut nums = Vec::new();
                    let mut ok = true;
                    for p in parts {
                        match p.parse::<f32>() {
                            Ok(v) => nums.push(v),
                            Err(_) => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if !ok {
                        d.extras.mark_damaged(key);
                        continue;
                    }
                    nums
                };
                if let Err(e) = d.extras.parse_optional_line(key, &nums, rest, n + 1) {
                    return Err(e);
                }
                continue;
            }
            let nums: Result<Vec<f32>, _> = parts.map(str::parse::<f32>).collect();
            let nums = nums.map_err(|_| format!("line {}: numbers expected after {key}", n + 1))?;
            let want = |k: usize| -> Result<(), String> {
                if nums.len() == k {
                    Ok(())
                } else {
                    Err(format!("line {}: {key} takes {k} numbers", n + 1))
                }
            };
            match key {
                "rate" => {
                    want(1)?;
                    d.rate = nums[0].max(0.0)
                }
                "lifetime" => {
                    want(1)?;
                    d.lifetime = nums[0].max(0.0)
                }
                "velocity" => {
                    want(3)?;
                    d.velocity = [nums[0], nums[1], nums[2]]
                }
                "spread" => {
                    want(1)?;
                    d.spread = nums[0].max(0.0)
                }
                "gravity" => {
                    want(3)?;
                    d.gravity = [nums[0], nums[1], nums[2]]
                }
                "size" => {
                    want(2)?;
                    d.size = (nums[0].max(0.0), nums[1].max(0.0))
                }
                "color" => {
                    want(8)?;
                    d.color = (
                        [nums[0], nums[1], nums[2], nums[3]],
                        [nums[4], nums[5], nums[6], nums[7]],
                    );
                }
                "max" => {
                    want(1)?;
                    d.max = nums[0].max(0.0) as usize
                }
                "seed" => {
                    want(1)?;
                    d.seed = nums[0].max(1.0) as u32
                }
                other => return Err(format!("line {}: unknown key {other}", n + 1)),
            }
        }
        particle_spawn::sort_bursts(&mut d.extras.bursts);
        Ok(d)
    }

    /// Write the effect back as `.fx` text that [`EmitterDef::parse`] reads unchanged.
    pub fn to_text(&self) -> String {
        let v = |a: [f32; 3]| format!("{} {} {}", a[0], a[1], a[2]);
        let c = |a: [f32; 4]| format!("{} {} {} {}", a[0], a[1], a[2], a[3]);
        let mut out = format!(
            "rate {}\nlifetime {}\nvelocity {}\nspread {}\ngravity {}\nsize {} {}\ncolor {}  {}\nmax {}\nseed {}\n",
            self.rate,
            self.lifetime,
            v(self.velocity),
            self.spread,
            v(self.gravity),
            self.size.0,
            self.size.1,
            c(self.color.0),
            c(self.color.1),
            self.max,
            self.seed
        );
        out.push_str(&optional_line("size_curve", &self.size_curve));
        out.push_str(&optional_line("alpha_curve", &self.alpha_curve));
        out.push_str(&optional_line("color_gradient", &self.color_gradient));
        self.extras.append_optional_text(&mut out);
        out
    }

    /// Curve shown for size: the stored curve, or a straight fade when it is absent or damaged.
    pub fn display_size_curve(&self) -> crate::curve::Curve {
        display_curve(self.size_curve.as_deref(), self.size.0, self.size.1)
    }

    /// Curve shown for opacity.
    pub fn display_alpha_curve(&self) -> crate::curve::Curve {
        display_curve(self.alpha_curve.as_deref(), self.color.0[3], self.color.1[3])
    }

    /// Gradient shown for colour.
    pub fn display_gradient(&self) -> crate::curve::Gradient {
        if let Some(text) = &self.color_gradient {
            let (gradient, error) = crate::curve_text::load_gradient(text);
            if error.is_none() {
                return gradient;
            }
        }
        crate::curve::Gradient::from_endpoints(self.color.0, self.color.1)
    }

    /// Error for a stored curve or gradient, when the text is damaged.
    pub fn curve_problem(&self, key: &str) -> Option<String> {
        let text = match key {
            "size_curve" => self.size_curve.as_deref(),
            "alpha_curve" => self.alpha_curve.as_deref(),
            "color_gradient" => self.color_gradient.as_deref(),
            "velocity_curve" => self.extras.velocity_curve.as_deref(),
            "rotation_curve" => self.extras.rotation_curve.as_deref(),
            _ => None,
        }?;
        if key == "color_gradient" {
            crate::curve_text::load_gradient(text).1
        } else {
            crate::curve_text::load_curve(text).1
        }
    }

    fn size_at(&self, u: f32) -> f32 {
        if let Some(text) = &self.size_curve {
            let (curve, error) = crate::curve_text::load_curve(text);
            if error.is_none() {
                return crate::curve::evaluate(&curve, u).max(0.0);
            }
        }
        self.size.0 + (self.size.1 - self.size.0) * u
    }

    fn color_at(&self, u: f32) -> [f32; 4] {
        if let Some(text) = &self.color_gradient {
            let (gradient, error) = crate::curve_text::load_gradient(text);
            if error.is_none() {
                return crate::curve::sample_gradient(&gradient, u);
            }
        }
        let (a, b) = self.color;
        let mut c = [0.0; 4];
        for i in 0..4 {
            c[i] = a[i] + (b[i] - a[i]) * u;
        }
        c
    }

    fn alpha_at(&self, u: f32) -> Option<f32> {
        let text = self.alpha_curve.as_deref()?;
        let (curve, error) = crate::curve_text::load_curve(text);
        if error.is_some() {
            return None;
        }
        Some(crate::curve::evaluate(&curve, u).clamp(0.0, 1.0))
    }

    fn velocity_mul(&self, u: f32) -> f32 {
        if let Some(text) = &self.extras.velocity_curve {
            let (curve, error) = crate::curve_text::load_curve(text);
            if error.is_none() {
                return crate::curve::evaluate(&curve, u).max(0.0);
            }
        }
        1.0
    }

    fn rotation_mul(&self, u: f32) -> f32 {
        if let Some(text) = &self.extras.rotation_curve {
            let (curve, error) = crate::curve_text::load_curve(text);
            if error.is_none() {
                return crate::curve::evaluate(&curve, u);
            }
        }
        1.0
    }
}

fn optional_line(key: &str, value: &Option<String>) -> String {
    match value {
        Some(value) => format!("{key} {value}\n"),
        None => String::new(),
    }
}

fn display_curve(text: Option<&str>, start: f32, end: f32) -> crate::curve::Curve {
    if let Some(text) = text {
        let (curve, error) = crate::curve_text::load_curve(text);
        if error.is_none() {
            return curve;
        }
    }
    crate::curve::Curve::linear(start, end)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Particle {
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub base_vel: [f32; 3],
    pub age: f32,
    pub angle: f32,
    pub ang_vel: f32,
    pub trail: Vec<[f32; 3]>,
    pub flip_frame: u32,
    pub noise_tag: u32,
}

#[derive(Clone, Debug)]
pub struct Emitter {
    pub def: EmitterDef,
    pub particles: Vec<Particle>,
    owed: f32,
    rng: u32,
    elapsed: f32,
    burst_cursor: f32,
    next_id: u32,
}

impl Emitter {
    pub fn new(def: EmitterDef) -> Emitter {
        let rng = def.seed.max(1);
        Emitter {
            def,
            particles: vec![],
            owed: 0.0,
            rng,
            elapsed: 0.0,
            burst_cursor: -1.0,
            next_id: 1,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }

    fn rand_u32(&mut self) -> u32 {
        self.rand();
        self.rng
    }

    fn spawn_one(&mut self, origin: [f32; 3]) {
        let d = self.def.clone();
        if self.particles.len() >= d.max {
            return;
        }
        let offset = sample_emit_offset(&d.extras.shape, || self.rand());
        let mut pos = origin;
        for i in 0..3 {
            pos[i] += offset[i];
        }
        let mut vel = d.velocity;
        for v in &mut vel {
            *v += self.rand() * d.spread;
        }
        let rs = d.extras.rotation_start;
        let t = (self.rand() + 1.0) * 0.5;
        let angle = rs.0 + (rs.1 - rs.0) * t;
        let rr = d.extras.rotation_rate;
        let ang_vel = rr.0 + (rr.1 - rr.0) * ((self.rand() + 1.0) * 0.5);
        // The frame count is only known where the picture is (draw time): keep the raw draw.
        let flip_frame = self.rand_u32();
        let id = self.next_id;
        self.next_id += 1;
        let trail_len = match &d.extras.renderer {
            RendererMode::Trail { points, .. } => *points,
            _ => 0,
        };
        let mut trail = Vec::new();
        if trail_len >= 2 {
            trail.push(pos);
        }
        self.particles.push(Particle {
            pos,
            base_vel: vel,
            vel,
            age: 0.0,
            angle,
            ang_vel,
            trail,
            flip_frame,
            noise_tag: id.wrapping_mul(d.seed),
        });
    }

    fn noise_accel(d: &EmitterDef, p: &Particle, t: f32) -> [f32; 3] {
        let s = d.extras.noise_strength;
        if s <= 0.0 {
            return [0.0; 3];
        }
        let f = d.extras.noise_frequency.max(0.001);
        let tag = p.noise_tag;
        let h = |x: u32| -> f32 {
            let mut v = x.wrapping_mul(0x9E37_79B9);
            v ^= v >> 16;
            (v & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0
        };
        let phase = t * f;
        [
            h(tag.wrapping_add(1)) * s * (phase * 1.1).sin(),
            h(tag.wrapping_add(2)) * s * (phase * 0.9).cos(),
            h(tag.wrapping_add(3)) * s * (phase * 1.3).sin(),
        ]
    }

    /// Advance `dt` seconds with the emitter at `origin`.
    pub fn step(&mut self, dt: f32, origin: [f32; 3]) {
        let d = self.def.clone();
        self.elapsed += dt;
        for burst in bursts_due(&d.extras, self.burst_cursor, self.elapsed) {
            let _ = burst.0;
            for _ in 0..burst.1 {
                self.spawn_one(origin);
            }
        }
        self.burst_cursor = self.elapsed;
        for p in &mut self.particles {
            let u = if d.lifetime > 0.0 { (p.age / d.lifetime).clamp(0.0, 1.0) } else { 1.0 };
            let v_mul = d.velocity_mul(u);
            if d.extras.drag > 0.0 {
                let keep = (1.0_f32 - d.extras.drag * dt).clamp(0.0, 1.0);
                for i in 0..3 {
                    p.base_vel[i] *= keep;
                }
            }
            let noise = Self::noise_accel(&d, p, p.age);
            for i in 0..3 {
                p.base_vel[i] += (d.gravity[i] + noise[i]) * dt;
                p.vel[i] = p.base_vel[i] * v_mul;
            }
            if d.extras.orbital.abs() > 1e-6 {
                let dx = p.pos[0] - origin[0];
                let dz = p.pos[2] - origin[2];
                let tang = d.extras.orbital * dt;
                let nx = dx * tang.cos() - dz * tang.sin();
                let nz = dx * tang.sin() + dz * tang.cos();
                p.pos[0] = origin[0] + nx;
                p.pos[2] = origin[2] + nz;
            } else {
                for i in 0..3 {
                    p.pos[i] += p.vel[i] * dt;
                }
            }
            p.angle += p.ang_vel * d.rotation_mul(u) * dt;
            p.age += dt;
            if let RendererMode::Trail { points, .. } = &d.extras.renderer {
                if *points >= 2 {
                    p.trail.push(p.pos);
                    while p.trail.len() > *points {
                        p.trail.remove(0);
                    }
                }
            }
        }
        self.particles.retain(|p| p.age < d.lifetime);
        self.owed += d.rate.max(0.0) * dt;
        while self.owed >= 1.0 {
            self.owed -= 1.0;
            self.spawn_one(origin);
        }
    }

    /// (size, rgba) of a particle from its age.
    pub fn look(&self, p: &Particle) -> (f32, [f32; 4]) {
        let u =
            if self.def.lifetime > 0.0 { (p.age / self.def.lifetime).clamp(0.0, 1.0) } else { 1.0 };
        let mut color = self.def.color_at(u);
        if let Some(alpha) = self.def.alpha_at(u) {
            color[3] = alpha;
        }
        (self.def.size_at(u), color)
    }

    pub fn life_u(&self, p: &Particle) -> f32 {
        if self.def.lifetime > 0.0 {
            (p.age / self.def.lifetime).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> EmitterDef {
        EmitterDef {
            rate: 10.0,
            lifetime: 1.0,
            velocity: [0.0, 2.0, 0.0],
            gravity: [0.0, -2.0, 0.0],
            spread: 0.5,
            ..Default::default()
        }
    }

    #[test]
    fn effect_text_round_trips() {
        let d = EmitterDef { max: 40, seed: 7, color: ([1.0, 0.5, 0.0, 1.0], [0.2; 4]), ..def() };
        assert_eq!(EmitterDef::parse(&d.to_text()).unwrap(), d);
    }

    #[test]
    fn spawns_at_the_rate_and_retires_at_end_of_life() {
        let mut e = Emitter::new(def());
        for _ in 0..60 {
            e.step(1.0 / 60.0, [0.0; 3]);
        }
        assert!((9..=11).contains(&e.particles.len()), "{}", e.particles.len());
        for _ in 0..60 {
            e.step(1.0 / 60.0, [0.0; 3]);
        }
        assert!(e.particles.iter().all(|p| p.age < 1.0));
        assert!((9..=11).contains(&e.particles.len()));
    }

    #[test]
    fn it_is_deterministic_and_obeys_gravity_and_cap() {
        let (mut a, mut b) = (Emitter::new(def()), Emitter::new(def()));
        for _ in 0..90 {
            a.step(1.0 / 60.0, [1.0, 0.0, 0.0]);
            b.step(1.0 / 60.0, [1.0, 0.0, 0.0]);
        }
        assert_eq!(a.particles, b.particles);
        let mut c = Emitter::new(EmitterDef { rate: 100.0, max: 5, spread: 0.0, ..def() });
        c.step(0.5, [0.0; 3]);
        assert_eq!(c.particles.len(), 5);
        let mut g = Emitter::new(EmitterDef { rate: 60.0, spread: 0.0, ..def() });
        g.step(1.0 / 60.0, [0.0; 3]);
        for _ in 0..30 {
            g.step(1.0 / 60.0, [0.0; 3]);
        }
        assert!(g.particles[0].vel[1] < 1.1 && g.particles[0].pos[1] > 0.0);
    }

    #[test]
    fn size_and_colour_fade_over_life() {
        let e = Emitter::new(EmitterDef { size: (1.0, 0.0), ..def() });
        let (s, c) = e.look(&Particle {
            pos: [0.0; 3],
            vel: [0.0; 3],
            base_vel: [0.0; 3],
            age: 0.5,
            angle: 0.0,
            ang_vel: 0.0,
            trail: vec![],
            flip_frame: 0,
            noise_tag: 1,
        });
        assert!((s - 0.5).abs() < 1e-6 && (c[3] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_size_curve_replaces_the_straight_fade() {
        let curve = crate::curve::Curve::constant(2.5);
        let text = crate::curve_text::curve_to_text(&curve);
        let e =
            Emitter::new(EmitterDef { size: (1.0, 0.0), size_curve: Some(text.clone()), ..def() });
        let look = |p| e.look(p);
        let p = Particle {
            pos: [0.0; 3],
            vel: [0.0; 3],
            base_vel: [0.0; 3],
            age: 0.5,
            angle: 0.0,
            ang_vel: 0.0,
            trail: vec![],
            flip_frame: 0,
            noise_tag: 1,
        };
        let (s, _) = look(&p);
        assert!((s - 2.5).abs() < 1e-4, "{s}");
        let broken = Emitter::new(EmitterDef {
            size: (1.0, 0.0),
            size_curve: Some("not-a-curve".into()),
            ..def()
        });
        let (s, _) = broken.look(&p);
        assert!((s - 0.5).abs() < 1e-4, "{s}");
        assert!(broken.def.curve_problem("size_curve").is_some_and(|e| e.contains("damaged")));
        let again = EmitterDef::parse(&e.def.to_text()).unwrap();
        assert_eq!(again.size_curve.as_deref(), Some(text.as_str()));
    }

    #[test]
    fn an_old_effect_file_has_no_curve_and_fades_in_a_straight_line() {
        let text = "rate 10\nlifetime 1\nsize 1 0\ncolor 1 1 1 1  1 1 1 0\n";
        let def = EmitterDef::parse(text).unwrap();
        assert!(
            def.size_curve.is_none() && def.alpha_curve.is_none() && def.color_gradient.is_none()
        );
        let (s, c) = Emitter::new(def).look(&Particle {
            pos: [0.0; 3],
            vel: [0.0; 3],
            base_vel: [0.0; 3],
            age: 0.5,
            angle: 0.0,
            ang_vel: 0.0,
            trail: vec![],
            flip_frame: 0,
            noise_tag: 1,
        });
        assert!((s - 0.5).abs() < 1e-6 && (c[3] - 0.5).abs() < 1e-6);
    }
}
