//! Low-level physics: worlds, bodies, shapes and joints by handle, with no scene objects.
//!
//! Each world has its own backend and is stepped on its own. A world made from
//! the scene steps during play. A world made from a graph node steps only when
//! a step node says so.

use std::collections::BTreeMap;
use std::path::Path;

use seal_graph::Value;

use super::backend::{self, BodyDesc, JointDesc, JointKind, PhysicsBackend, RayHit};
use super::load::{self, MeshData};
use super::shapes::{self, Geom};

struct Slot {
    /// Scene worlds step with play. Graph worlds wait for a step node.
    auto: bool,
    backend: Box<dyn PhysicsBackend>,
}

#[derive(Default)]
pub struct PhysicsServer {
    worlds: BTreeMap<String, Slot>,
}

impl PhysicsServer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.worlds.clear();
    }

    pub fn worlds(&self) -> Vec<String> {
        self.worlds.keys().cloned().collect()
    }

    pub fn auto_names(&self) -> Vec<String> {
        self.worlds.iter().filter(|(_, slot)| slot.auto).map(|(name, _)| name.clone()).collect()
    }

    pub fn body_count(&self, world: &str) -> usize {
        self.worlds.get(world).map(|slot| slot.backend.body_count()).unwrap_or(0)
    }

    pub fn create_world(
        &mut self,
        name: &str,
        gravity: [f32; 3],
        backend_name: &str,
        auto: bool,
    ) -> Result<(), String> {
        let name = check_name(name)?;
        let mut backend = backend::open_backend(backend_name)?;
        backend.set_gravity(finite_gravity(gravity));
        self.worlds.insert(name, Slot { auto, backend });
        Ok(())
    }

    pub fn set_backend(&mut self, world: &str, backend_name: &str) -> Result<(), String> {
        let slot = self.worlds.get_mut(world).ok_or_else(|| format!("no physics world {world}"))?;
        let gravity = slot.backend.gravity();
        let auto = slot.auto;
        let mut backend = backend::open_backend(backend_name)?;
        backend.set_gravity(gravity);
        *slot = Slot { auto, backend };
        Ok(())
    }

    pub fn create_body(&mut self, world: &str, desc: BodyDesc) -> Result<u64, String> {
        self.ensure(world)?;
        let slot = self.worlds.get_mut(world).unwrap();
        Ok(slot.backend.create_body(desc))
    }

    pub fn create_shape(&mut self, world: &str, body: u64, geom: Geom) -> Result<u64, String> {
        let slot = self.worlds.get_mut(world).ok_or_else(|| format!("no physics world {world}"))?;
        slot.backend.create_shape(body, geom)
    }

    pub fn create_joint(&mut self, world: &str, desc: JointDesc) -> Result<u64, String> {
        let slot = self.worlds.get_mut(world).ok_or_else(|| format!("no physics world {world}"))?;
        slot.backend.create_joint(desc)
    }

    pub fn step_world(&mut self, world: &str, dt: f32) -> Result<(), String> {
        let slot = self.worlds.get_mut(world).ok_or_else(|| format!("no physics world {world}"))?;
        slot.backend.step(dt);
        Ok(())
    }

    pub fn translation(&self, world: &str, body: u64) -> Option<[f32; 3]> {
        self.worlds.get(world).and_then(|slot| slot.backend.translation(body))
    }

    pub fn velocity(&self, world: &str, body: u64) -> Option<[f32; 3]> {
        self.worlds.get(world).and_then(|slot| slot.backend.velocity(body))
    }

    pub fn ray(&self, world: &str, from: [f32; 3], dir: [f32; 3], max_t: f32) -> Option<RayHit> {
        self.worlds.get(world).and_then(|slot| slot.backend.ray(from, dir, max_t))
    }

    pub fn overlap_sphere(&self, world: &str, center: [f32; 3], radius: f32) -> Vec<u64> {
        self.worlds
            .get(world)
            .map(|slot| slot.backend.overlap_sphere(center, radius))
            .unwrap_or_default()
    }

    fn ensure(&mut self, world: &str) -> Result<(), String> {
        if self.worlds.contains_key(world) {
            return Ok(());
        }
        self.create_world(world, [0.0, -9.81, 0.0], "builtin", false)
    }
}

fn check_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 32
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("physics world name must be 1 to 32 letters, digits, _ or -".into());
    }
    Ok(name.to_string())
}

fn finite_gravity(v: [f32; 3]) -> [f32; 3] {
    v.map(|n| if n.is_finite() { n } else { 0.0 })
}

/// Run one graph call against `server`. `mesh` and picture paths are read from `project`.
pub fn apply_call(
    server: &mut PhysicsServer,
    project: Option<&Path>,
    op: &str,
    args: &[Value],
) -> Result<Value, String> {
    match op {
        "body" => {
            let world = text_at(args, 0);
            let desc = BodyDesc {
                position: [num_at(args, 1), num_at(args, 2), num_at(args, 3)],
                dynamic: args.get(4).is_some_and(Value::bool),
                ..BodyDesc::default()
            };
            let id = server.create_body(&world, desc)?;
            Ok(Value::Float(id as f64))
        }
        "shape" => {
            let world = text_at(args, 0);
            let body = id_at(args, 1)?;
            let kind = text_at(args, 2);
            let a = num_at(args, 3);
            let b = num_at(args, 4);
            let c = num_at(args, 5);
            let mesh = text_at(args, 6);
            let geom = geom_from_args(&kind, a, b, c, &mesh, project)?;
            let id = server.create_shape(&world, body, geom)?;
            Ok(Value::Float(id as f64))
        }
        "joint" => {
            let world = text_at(args, 0);
            let kind = JointKind::parse(&text_at(args, 1))
                .ok_or("joint kind must be fixed, distance or hinge")?;
            let desc = JointDesc {
                kind,
                body_a: id_at(args, 2)?,
                body_b: id_at(args, 3)?,
                length: num_at(args, 4),
            };
            let id = server.create_joint(&world, desc)?;
            Ok(Value::Float(id as f64))
        }
        "step" => {
            let world = text_at(args, 0);
            server.step_world(&world, num_at(args, 1))?;
            Ok(Value::Float(0.0))
        }
        "position" => {
            let world = text_at(args, 0);
            let body = id_at(args, 1)?;
            let p = server.translation(&world, body).unwrap_or([0.0; 3]);
            Ok(Value::Vec3(seal_graph::V3 { x: p[0] as f64, y: p[1] as f64, z: p[2] as f64 }))
        }
        other => Err(format!("unknown physics call {other}")),
    }
}

fn text_at(args: &[Value], index: usize) -> String {
    args.get(index).map(Value::str).unwrap_or_default()
}

fn num_at(args: &[Value], index: usize) -> f32 {
    let n = args.get(index).map(Value::float).unwrap_or(0.0) as f32;
    if n.is_finite() {
        n
    } else {
        0.0
    }
}

fn id_at(args: &[Value], index: usize) -> Result<u64, String> {
    let n = num_at(args, index);
    if n <= 0.0 {
        return Err("physics handle is missing".into());
    }
    Ok(n.round() as u64)
}

pub fn geom_from_args(
    kind: &str,
    a: f32,
    b: f32,
    c: f32,
    source: &str,
    project: Option<&Path>,
) -> Result<Geom, String> {
    match kind {
        "cylinder" => {
            let radius = if a > 0.0 { a } else { 0.5 };
            let half = if b > 0.0 { b } else { 0.5 };
            Ok(Geom::Cylinder { radius, half_height: half })
        }
        "trimesh" => {
            let mesh = read_mesh(source, project)?;
            Ok(Geom::Mesh { vertices: mesh.vertices, indices: mesh.indices })
        }
        "convex" => {
            let mesh = read_mesh(source, project)?;
            let max = if c >= 1.0 { c as u32 } else { 4 };
            Ok(Geom::Hulls { parts: shapes::decompose(&mesh.vertices, &mesh.indices, max) })
        }
        "heightfield" => {
            let size = [
                if a > 0.0 { a } else { 4.0 },
                if b.is_finite() { b } else { 1.0 },
                if c > 0.0 { c } else { 4.0 },
            ];
            let (columns, rows, heights) = read_field(source, project, size)?;
            Ok(Geom::Heightfield { columns, rows, heights, size })
        }
        other => {
            Err(format!("shape must be cylinder, trimesh, heightfield or convex, not {other}"))
        }
    }
}

fn read_mesh(source: &str, project: Option<&Path>) -> Result<MeshData, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("mesh text or path is empty".into());
    }
    if source.starts_with("v ") || source.contains("\nv ") || source.starts_with("v\t") {
        return load::mesh_from_text(source);
    }
    let path = resolve(project, source)?;
    load::mesh_from_path(&path)
}

fn read_field(
    source: &str,
    project: Option<&Path>,
    size: [f32; 3],
) -> Result<(usize, usize, Vec<f32>), String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("height picture path is empty".into());
    }
    if source.starts_with("grid") {
        let picture = load::picture_from_bytes(source.as_bytes())?;
        return load::heights_from_picture(&picture, size);
    }
    let path = resolve(project, source)?;
    let picture = load::picture_from_path(&path)?;
    load::heights_from_picture(&picture, size)
}

fn resolve(project: Option<&Path>, relative: &str) -> Result<std::path::PathBuf, String> {
    let rel = Path::new(relative);
    if rel.is_absolute() {
        return Ok(rel.to_path_buf());
    }
    let Some(root) = project else {
        return Err(format!("no project folder for {relative}"));
    };
    if relative.split(['/', '\\']).any(|part| part == "..") {
        return Err("path must stay inside the project".into());
    }
    Ok(root.join(rel))
}
