//! Several physics worlds in one scene, a handle-based server, and a pluggable backend.
//!
//! The built-in solver is the default backend. Objects with a physics world name
//! step in that world and stay out of the ordinary play simulation. Cylinder,
//! triangle mesh, heightfield and convex pieces are collider shapes on those
//! bodies, and the same shapes are handed to the 3D play preview when an object
//! is not assigned to an extra world.

mod backend;
mod load;
mod server;
pub mod shapes;

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use seal_ecs::{text_component, LoadCtx, SaveCtx, SealField, SealValue};

use crate::Entity;

pub use backend::{
    open_backend, BodyDesc, BuiltinBackend, DriftBackend, JointDesc, JointKind, PhysicsBackend,
    RayHit, BACKENDS,
};
pub use load::{mesh_from_text, picture_from_bytes, MeshData, Picture};
pub use server::{apply_call, geom_from_args, PhysicsServer};
pub use shapes::{decompose, overlap_sphere, ray, Geom, Hull};

/// Saved description of one extra world. Gravity is a positive downward pull, matching the scene.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldRec {
    pub name: String,
    pub gravity: f32,
    pub backend: String,
}

impl Default for WorldRec {
    fn default() -> Self {
        Self { name: String::new(), gravity: 9.81, backend: "builtin".into() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemberRec {
    pub entity: String,
    pub world: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColliderRec {
    pub entity: String,
    pub shape: String,
    pub mesh: String,
    pub picture: String,
    pub radius: f32,
    pub half_height: f32,
    pub size: [f32; 3],
    pub max_hulls: u32,
}

impl Default for ColliderRec {
    fn default() -> Self {
        Self {
            entity: String::new(),
            shape: String::new(),
            mesh: String::new(),
            picture: String::new(),
            radius: 0.5,
            half_height: 0.5,
            size: [4.0, 1.0, 4.0],
            max_hulls: 4,
        }
    }
}

/// Scene file resource. Absent when the scene uses only the ordinary simulation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PhysicsSceneSave {
    pub worlds: Vec<WorldRec>,
    pub members: Vec<MemberRec>,
    pub colliders: Vec<ColliderRec>,
}

text_component!(PhysicsSceneSave as "physics_scene" { worlds, members, colliders });

impl SealField for WorldRec {
    fn save(&self, ctx: &SaveCtx) -> SealValue {
        SealValue::Map(seal_ecs::SealMap::new(vec![
            ("name".into(), self.name.save(ctx)),
            ("gravity".into(), self.gravity.save(ctx)),
            ("backend".into(), self.backend.save(ctx)),
        ]))
    }
    fn load(value: &SealValue, ctx: &LoadCtx) -> Result<Self, String> {
        if value.as_map().is_none() {
            return Err("physics world must be a map".into());
        }
        let mut out = Self::default();
        if let Some(v) = value.get("name") {
            out.name = String::load(v, ctx)?;
        }
        if let Some(v) = value.get("gravity") {
            out.gravity = f32::load(v, ctx)?;
        }
        if let Some(v) = value.get("backend") {
            out.backend = String::load(v, ctx)?;
        }
        Ok(out)
    }
}

impl SealField for MemberRec {
    fn save(&self, ctx: &SaveCtx) -> SealValue {
        SealValue::Map(seal_ecs::SealMap::new(vec![
            ("entity".into(), self.entity.save(ctx)),
            ("world".into(), self.world.save(ctx)),
        ]))
    }
    fn load(value: &SealValue, ctx: &LoadCtx) -> Result<Self, String> {
        if value.as_map().is_none() {
            return Err("physics member must be a map".into());
        }
        Ok(Self {
            entity: value
                .get("entity")
                .map(|v| String::load(v, ctx))
                .transpose()?
                .unwrap_or_default(),
            world: value
                .get("world")
                .map(|v| String::load(v, ctx))
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

impl SealField for ColliderRec {
    fn save(&self, ctx: &SaveCtx) -> SealValue {
        SealValue::Map(seal_ecs::SealMap::new(vec![
            ("entity".into(), self.entity.save(ctx)),
            ("shape".into(), self.shape.save(ctx)),
            ("mesh".into(), self.mesh.save(ctx)),
            ("picture".into(), self.picture.save(ctx)),
            ("radius".into(), self.radius.save(ctx)),
            ("half_height".into(), self.half_height.save(ctx)),
            ("size".into(), self.size.save(ctx)),
            ("max_hulls".into(), self.max_hulls.save(ctx)),
        ]))
    }
    fn load(value: &SealValue, ctx: &LoadCtx) -> Result<Self, String> {
        if value.as_map().is_none() {
            return Err("physics collider must be a map".into());
        }
        let mut out = Self::default();
        let str_field = |key: &str| -> Result<Option<String>, String> {
            value.get(key).map(|v| String::load(v, ctx)).transpose()
        };
        if let Some(v) = str_field("entity")? {
            out.entity = v;
        }
        if let Some(v) = str_field("shape")? {
            out.shape = v;
        }
        if let Some(v) = str_field("mesh")? {
            out.mesh = v;
        }
        if let Some(v) = str_field("picture")? {
            out.picture = v;
        }
        if let Some(v) = value.get("radius") {
            out.radius = f32::load(v, ctx)?;
        }
        if let Some(v) = value.get("half_height") {
            out.half_height = f32::load(v, ctx)?;
        }
        if let Some(v) = value.get("size") {
            out.size = <[f32; 3]>::load(v, ctx)?;
        }
        if let Some(v) = value.get("max_hulls") {
            out.max_hulls = u32::load(v, ctx)?;
        }
        Ok(out)
    }
}

impl PhysicsSceneSave {
    pub fn is_blank(&self) -> bool {
        self.worlds.is_empty() && self.members.is_empty() && self.colliders.is_empty()
    }

    pub fn world_names(&self) -> Vec<String> {
        let mut names = vec!["main".to_string()];
        for world in &self.worlds {
            if world.name != "main" && !names.iter().any(|n| n == &world.name) {
                names.push(world.name.clone());
            }
        }
        names
    }

    pub fn gravity_of(&self, name: &str, scene_gravity: f32) -> f32 {
        self.worlds.iter().find(|w| w.name == name).map(|w| w.gravity).unwrap_or(scene_gravity)
    }

    pub fn backend_of(&self, name: &str) -> String {
        self.worlds
            .iter()
            .find(|w| w.name == name)
            .map(|w| w.backend.clone())
            .filter(|b| BACKENDS.contains(&b.as_str()))
            .unwrap_or_else(|| "builtin".into())
    }

    pub fn add_world(&mut self, gravity: f32) -> String {
        let mut name = "preview".to_string();
        let mut n = 2u32;
        while self.worlds.iter().any(|w| w.name == name) || name == "main" {
            name = format!("preview-{n}");
            n += 1;
        }
        self.worlds.push(WorldRec { name: name.clone(), gravity, backend: "builtin".into() });
        name
    }

    pub fn remove_world(&mut self, name: &str) {
        if name == "main" {
            return;
        }
        self.worlds.retain(|w| w.name != name);
        self.members.retain(|m| m.world != name);
    }

    pub fn set_world_field(
        &mut self,
        name: &str,
        field: &str,
        value: &str,
        scene_gravity: f32,
    ) -> Result<(), String> {
        if name.is_empty() {
            return Err("missing world name".into());
        }
        let parsed = match field {
            "gravity" => {
                let g: f32 = value.parse().map_err(|_| "gravity must be a number".to_string())?;
                if !g.is_finite() {
                    return Err("gravity must be a number".into());
                }
                FieldWrite::Gravity(g)
            }
            "backend" => {
                if !BACKENDS.contains(&value) {
                    return Err("unknown physics backend".into());
                }
                FieldWrite::Backend(value.to_string())
            }
            _ => return Err("unknown physics world field".into()),
        };
        let world = if let Some(world) = self.worlds.iter_mut().find(|w| w.name == name) {
            world
        } else {
            self.worlds.push(WorldRec {
                name: name.to_string(),
                gravity: scene_gravity,
                backend: "builtin".into(),
            });
            self.worlds.last_mut().unwrap()
        };
        match parsed {
            FieldWrite::Gravity(g) => world.gravity = g,
            FieldWrite::Backend(name) => world.backend = name,
        }
        Ok(())
    }

    pub fn member<'a>(&'a self, entity: &str) -> Option<&'a str> {
        self.members.iter().find(|m| m.entity == entity).map(|m| m.world.as_str())
    }

    pub fn set_member(&mut self, entity: &str, world: &str) {
        self.members.retain(|m| m.entity != entity);
        if !world.is_empty() {
            self.members.push(MemberRec { entity: entity.to_string(), world: world.to_string() });
        }
    }

    pub fn collider<'a>(&'a self, entity: &str) -> Option<&'a ColliderRec> {
        self.colliders.iter().find(|c| c.entity == entity)
    }

    pub fn set_collider_field(
        &mut self,
        entity: &str,
        field: &str,
        value: &str,
    ) -> Result<(), String> {
        if field == "shape" && (value.is_empty() || value == "none") {
            self.colliders.retain(|c| c.entity != entity);
            return Ok(());
        }
        if !matches!(
            field,
            "shape"
                | "mesh"
                | "picture"
                | "radius"
                | "half_height"
                | "size_x"
                | "size_y"
                | "size_z"
                | "max_hulls"
        ) {
            return Err("unknown collider field".into());
        }
        if field == "shape" && !matches!(value, "cylinder" | "trimesh" | "heightfield" | "convex") {
            return Err("unknown collider shape".into());
        }
        if matches!(field, "radius" | "half_height" | "size_x" | "size_z") {
            let _ = positive_num(value, field)?;
        }
        if field == "size_y" {
            let _ = finite_num(value, "height")?;
        }
        if field == "max_hulls" {
            let n: u32 = value.parse().map_err(|_| "hull count must be a number".to_string())?;
            if n == 0 {
                return Err("hull count must be at least 1".into());
            }
        }
        if !self.colliders.iter().any(|c| c.entity == entity) {
            self.colliders.push(ColliderRec { entity: entity.to_string(), ..Default::default() });
        }
        let rec = self.colliders.iter_mut().find(|c| c.entity == entity).unwrap();
        match field {
            "shape" => {
                if !matches!(value, "cylinder" | "trimesh" | "heightfield" | "convex") {
                    return Err("unknown collider shape".into());
                }
                rec.shape = value.to_string();
            }
            "mesh" => rec.mesh = value.to_string(),
            "picture" => rec.picture = value.to_string(),
            "radius" => rec.radius = positive_num(value, "radius")?,
            "half_height" => rec.half_height = positive_num(value, "half height")?,
            "size_x" => rec.size[0] = positive_num(value, "size")?,
            "size_y" => rec.size[1] = finite_num(value, "height")?,
            "size_z" => rec.size[2] = positive_num(value, "size")?,
            "max_hulls" => {
                let n: u32 =
                    value.parse().map_err(|_| "hull count must be a number".to_string())?;
                rec.max_hulls = n.clamp(1, 64);
            }
            _ => return Err("unknown collider field".into()),
        }
        Ok(())
    }
}

enum FieldWrite {
    Gravity(f32),
    Backend(String),
}

fn positive_num(value: &str, label: &str) -> Result<f32, String> {
    let n = finite_num(value, label)?;
    if n <= 0.0 {
        return Err(format!("{label} must be greater than 0"));
    }
    Ok(n)
}

fn finite_num(value: &str, label: &str) -> Result<f32, String> {
    let n: f32 = value.parse().map_err(|_| format!("{label} must be a number"))?;
    if !n.is_finite() {
        return Err(format!("{label} must be a number"));
    }
    Ok(n)
}

/// Saved scene data plus the running server. The server is not written to the scene file.
pub struct PhysicsState {
    pub scene: PhysicsSceneSave,
    pub server: Arc<Mutex<PhysicsServer>>,
    links: Vec<(String, String, u64)>,
}

impl Default for PhysicsState {
    fn default() -> Self {
        Self {
            scene: PhysicsSceneSave::default(),
            server: Arc::new(Mutex::new(PhysicsServer::new())),
            links: Vec::new(),
        }
    }
}

impl PhysicsState {
    pub fn linked_ids(&self) -> Vec<String> {
        self.links.iter().map(|(id, _, _)| id.clone()).collect()
    }

    /// Build bodies for objects that picked a world. Returns notes about files that did not load.
    pub fn begin_play(
        &mut self,
        entities: &[Entity],
        project: Option<&Path>,
        scene_gravity: f32,
    ) -> Vec<String> {
        self.links.clear();
        let mut notes = Vec::new();
        {
            let mut server = lock(&self.server);
            server.clear();
            if self.scene.is_blank() {
                return notes;
            }
            let mut names = self.scene.world_names();
            for member in &self.scene.members {
                if !names.iter().any(|n| n == &member.world) {
                    names.push(member.world.clone());
                }
            }
            for name in &names {
                let gravity = [0.0, -self.scene.gravity_of(name, scene_gravity), 0.0];
                let backend = self.scene.backend_of(name);
                if let Err(err) = server.create_world(name, gravity, &backend, true) {
                    notes.push(err);
                }
            }
        }
        let use_server_for_loose = !cfg!(feature = "play-preview");
        let mut wanted: Vec<(String, String)> =
            self.scene.members.iter().map(|m| (m.entity.clone(), m.world.clone())).collect();
        if use_server_for_loose {
            for collider in &self.scene.colliders {
                if collider.shape.is_empty() {
                    continue;
                }
                if !wanted.iter().any(|(id, _)| id == &collider.entity) {
                    wanted.push((collider.entity.clone(), "main".into()));
                }
            }
        }
        for (entity_id, world) in wanted {
            let Some(entity) = entities.iter().find(|e| e.id == entity_id) else {
                notes.push(format!("physics world {world} names missing object {entity_id}"));
                continue;
            };
            let desc = BodyDesc {
                position: entity.translation,
                velocity: entity.velocity,
                dynamic: entity.dynamic,
                gravity_scale: 1.0,
            };
            let body = {
                let mut server = lock(&self.server);
                match server.create_body(&world, desc) {
                    Ok(id) => id,
                    Err(err) => {
                        notes.push(err);
                        continue;
                    }
                }
            };
            if let Some(collider) = self.scene.collider(&entity_id) {
                if !collider.shape.is_empty() {
                    match geom_of(collider, project) {
                        Ok(geom) => {
                            if let Err(err) = lock(&self.server).create_shape(&world, body, geom) {
                                notes.push(err);
                            }
                        }
                        Err(err) => notes.push(format!("{entity_id}: {err}")),
                    }
                }
            }
            self.links.push((entity_id, world, body));
        }
        notes
    }

    pub fn end_play(&mut self) {
        self.links.clear();
        lock(&self.server).clear();
    }

    pub fn step_linked(&mut self, dt: f32, entities: &mut [Entity]) {
        if self.links.is_empty() || dt <= 0.0 {
            return;
        }
        let mut server = lock(&self.server);
        let names = server.auto_names();
        for name in names {
            let _ = server.step_world(&name, dt);
        }
        let links = self.links.clone();
        for (id, world, body) in links {
            let Some(position) = server.translation(&world, body) else { continue };
            let velocity = server.velocity(&world, body).unwrap_or([0.0; 3]);
            if let Some(entity) = entities.iter_mut().find(|e| e.id == id) {
                entity.translation = position;
                entity.velocity = velocity;
            }
        }
    }

    /// Shapes for the 3D play preview: colliders whose object was not moved into another world.
    pub fn preview_solids(&self, project: Option<&Path>) -> (Vec<(String, Geom)>, Vec<String>) {
        let mut solids = Vec::new();
        let mut notes = Vec::new();
        let linked = self.linked_ids();
        for collider in &self.scene.colliders {
            if collider.shape.is_empty() || linked.iter().any(|id| id == &collider.entity) {
                continue;
            }
            match geom_of(collider, project) {
                Ok(geom) => solids.push((collider.entity.clone(), geom)),
                Err(err) => notes.push(format!("{}: {err}", collider.entity)),
            }
        }
        (solids, notes)
    }
}

fn geom_of(collider: &ColliderRec, project: Option<&Path>) -> Result<Geom, String> {
    let (a, b, c, source) = match collider.shape.as_str() {
        "cylinder" => (collider.radius, collider.half_height, 0.0, ""),
        "heightfield" => {
            (collider.size[0], collider.size[1], collider.size[2], collider.picture.as_str())
        }
        "trimesh" => (0.0, 0.0, 0.0, collider.mesh.as_str()),
        "convex" => (0.0, 0.0, collider.max_hulls as f32, collider.mesh.as_str()),
        other => return Err(format!("unknown collider shape {other}")),
    };
    server::geom_from_args(&collider.shape, a, b, c, source, project)
}

pub fn lock(server: &Mutex<PhysicsServer>) -> MutexGuard<'_, PhysicsServer> {
    server.lock().unwrap_or_else(|err| err.into_inner())
}
