//! The editor scene as an engine world, and back.
//!
//! Scene files are engine scenes: `seal-run`, the agent server and the C
//! interface open the same files the editor saves. Names, transforms and
//! parents use the engine's core components, gravity and time scale the
//! core scene settings, and the rest the standard scene components.

use std::collections::HashMap;

use seal_ecs::core::SceneSettings;
use seal_ecs::core::{Name, Transform};
use seal_ecs::{ChildOf, SceneId};
pub use seal_scene::{
    AudioListener, Body, Camera, Camera2d, Character2d, EditorInfo, Light, Light2d, Look,
    Material2d, NavAgent2d, Particles, PathFollow, Physics2d, Script, Shape2d, SpriteClip,
    SpriteImage, SpriteMask, SpriteShape,
};

use crate::{color_for, sprite_sort, Entity, PlayMode, Shape, World, DEFAULT_GRAVITY};

/// An engine world that knows the editor's components.
pub fn engine_world() -> seal_ecs::World {
    let mut world = seal_scene::scene_world();
    // Area effects live in the 2D physics crate, which the scene crate sits below.
    seal_physics2d::area_effect::register(&mut world);
    world.register_text::<crate::volumes::Volume>();
    world.register_text::<crate::volumes::ScriptOrder>();
    world.register_text::<crate::script_groups::ScriptGroup>();
    world.register_text::<crate::remote_transform::RemoteTransform>();
    world.register_text::<crate::collider_gate::ColliderGate>();
    world.register_text::<crate::object_merge::MergedDraw>();
    world.register_text::<crate::post_volume::PostVolume>();
    world.register_text_resource::<crate::object_files::ObjectFiles>();
    world.register_text_resource::<crate::players::LocalJoin>();
    world.register_text_resource::<crate::physics::PhysicsSceneSave>();
    world
}

impl World {
    /// The scene as engine entities, in outliner order.
    pub fn to_ecs(&self) -> seal_ecs::World {
        let mut world = engine_world();
        let mut by_key = HashMap::with_capacity(self.entities.len());
        let mut spawned = Vec::with_capacity(self.entities.len());
        for (index, e) in self.entities.iter().enumerate() {
            let id = world.spawn((
                SceneId(index as u64),
                Name(e.name.clone()),
                Transform { translation: e.translation, rotation: e.rotation, scale: e.scale },
                Look {
                    shape: e.shape.map(Shape::label).unwrap_or("").into(),
                    color: e.color,
                    roughness: e.roughness,
                    metal: e.metal,
                },
                EditorInfo {
                    key: e.id.clone(),
                    tag: e.tag.clone(),
                    hidden: e.hidden,
                    locked: e.locked,
                    source_kind: e.source_kind.clone(),
                    source_asset: e.source_asset.clone(),
                    unsaved: e.outliner_unsaved,
                    play_spawned: e.outliner_play_spawned,
                    warning: e.outliner_warning_key.clone(),
                    overrides: e.overrides.clone(),
                },
            ));
            if !e.sprite.is_empty() {
                world.insert(
                    id,
                    SpriteImage {
                        image: e.sprite.clone(),
                        layer: e.layer,
                        frames: e.frames.max(1),
                        fps: e.fps,
                        animation: e.animation.clone(),
                        flip_x: e.flip[0],
                        flip_y: e.flip[1],
                        tint: e.tint,
                        order: e.order,
                        slice: e.slice,
                        normal_map: e.normal_map.clone(),
                        parallax: e.parallax,
                        repeat_x: e.repeat_x,
                        slice_mode: e.slice_mode.clone(),
                        slice_size: e.slice_size,
                        pivot: e.pivot,
                        sort_mode: e.sort_mode.name().into(),
                    },
                );
            }
            if e.mesh_material != seal_scene::MeshMaterial::default() {
                world.insert(id, e.mesh_material.clone());
            }
            if !e.script.is_empty() {
                world.insert(id, Script { graph: e.script.clone() });
            }
            if !e.volume.kind.is_empty() {
                world.insert(id, e.volume.clone());
            }
            if e.volume.kind == "post" {
                world.insert(id, e.post.clone());
            }
            if !e.merged.kind.is_empty() {
                world.insert(id, e.merged.clone());
            }
            if e.script_order != 0 {
                world.insert(id, crate::volumes::ScriptOrder { order: e.script_order });
            }
            if crate::script_groups::name(&e.script_group) != crate::script_groups::BEFORE {
                world.insert(
                    id,
                    crate::script_groups::ScriptGroup {
                        group: crate::script_groups::name(&e.script_group).into(),
                    },
                );
            }
            if e.remote.on {
                world.insert(id, e.remote.clone());
            }
            if !e.collider_enabled {
                world.insert(id, crate::collider_gate::ColliderGate { enabled: false });
            }
            if !e.particles.is_empty() {
                world.insert(id, Particles { effect: e.particles.clone() });
            }
            let body = Body {
                dynamic: e.dynamic,
                friction: e.friction,
                restitution: e.restitution,
                walk_speed: e.walk_speed,
            };
            if body != Body::default() {
                world.insert(id, body);
            }
            if e.intensity != Light::default().intensity {
                world.insert(id, Light { intensity: e.intensity });
            }
            if !e.light2d.kind.is_empty() {
                world.insert(id, e.light2d.clone());
            }
            if e.camera2d != Camera2d::default() {
                world.insert(id, e.camera2d.clone());
            }
            // A compound part has a collider but no body of its own.
            if !e.phys2d.body.is_empty() || e.phys2d.part {
                world.insert(id, e.phys2d.clone());
            }
            if !e.area.mode.is_empty() {
                world.insert(id, e.area.clone());
            }
            if e.surface.speed != 0.0 {
                world.insert(id, e.surface.clone());
            }
            if e.nav2d != NavAgent2d::default() {
                world.insert(id, e.nav2d.clone());
            }
            if e.character2d != Character2d::default() {
                world.insert(id, e.character2d.clone());
            }
            if e.shape2d != Shape2d::default() {
                world.insert(id, e.shape2d.clone());
            }
            if e.sprite_shape != SpriteShape::default() {
                world.insert(id, e.sprite_shape.clone());
            }
            if e.follow != PathFollow::default() {
                world.insert(id, e.follow.clone());
            }
            if e.material2d != Material2d::default() {
                world.insert(id, e.material2d.clone());
            }
            if !e.sprite_mask.shape.is_empty() {
                world.insert(id, e.sprite_mask.clone());
            }
            if !e.mask.is_empty() {
                world.insert(id, SpriteClip { mode: e.mask.clone() });
            }
            let camera = Camera { fov: e.fov, near: e.near, far: e.far };
            if camera != Camera::default() {
                world.insert(id, camera);
            }
            by_key.insert(e.id.as_str(), id);
            spawned.push(id);
        }
        for (e, &id) in self.entities.iter().zip(&spawned) {
            if let Some(parent) = e.parent.as_deref().and_then(|p| by_key.get(p)) {
                // Keys are unique and parents form a tree in a valid scene.
                let _ = world.set_parent(id, Some(*parent));
            }
        }
        world
    }

    /// Rebuild the editor scene from engine entities (in `order`).
    pub fn from_ecs(world: &seal_ecs::World, order: &[seal_ecs::Entity]) -> Result<Self, String> {
        let mut keys: HashMap<seal_ecs::Entity, String> = HashMap::new();
        for (index, &id) in order.iter().enumerate() {
            let key = world
                .get::<EditorInfo>(id)
                .map(|i| i.key.clone())
                .filter(|k| !k.is_empty())
                .unwrap_or_else(|| format!("entity{index}"));
            if keys.values().any(|k| *k == key) {
                return Err(format!("entity key {key} appears twice"));
            }
            keys.insert(id, key);
        }
        let mut entities = Vec::with_capacity(order.len());
        for &id in order {
            let info = world.get::<EditorInfo>(id).cloned().unwrap_or_default();
            let transform = world.get::<Transform>(id).cloned().unwrap_or_default();
            let look = world.get::<Look>(id).cloned().unwrap_or_else(|| Look {
                color: color_for(&info.tag),
                roughness: 0.5,
                ..Look::default()
            });
            let light = world.get::<Light>(id).cloned().unwrap_or_default();
            let camera = world.get::<Camera>(id).cloned().unwrap_or_default();
            let key = keys[&id].clone();
            let sprite = world.get::<SpriteImage>(id).cloned().unwrap_or_default();
            entities.push(Entity {
                name: world.get::<Name>(id).map_or_else(|| key.clone(), |n| n.0.clone()),
                parent: world.get::<ChildOf>(id).and_then(|c| keys.get(&c.parent()).cloned()),
                id: key,
                translation: transform.translation,
                rotation: transform.rotation,
                scale: transform.scale,
                shape: Shape::parse(&look.shape),
                dynamic: world.get::<Body>(id).is_some_and(|b| b.dynamic),
                friction: world.get::<Body>(id).map_or(Body::default().friction, |b| b.friction),
                restitution: world
                    .get::<Body>(id)
                    .map_or(Body::default().restitution, |b| b.restitution),
                walk_speed: world.get::<Body>(id).map_or(0.0, |b| b.walk_speed),
                velocity: [0.0; 3],
                tag: info.tag,
                color: look.color,
                hidden: info.hidden,
                locked: info.locked,
                unselectable: false,
                roughness: look.roughness,
                metal: look.metal,
                intensity: light.intensity,
                fov: camera.fov,
                near: camera.near,
                far: camera.far,
                source_kind: info.source_kind,
                source_asset: info.source_asset,
                outliner_unsaved: info.unsaved,
                outliner_play_spawned: info.play_spawned,
                outliner_warning_key: info.warning,
                overrides: info.overrides,
                sprite: sprite.image,
                layer: sprite.layer,
                frames: sprite.frames.max(1),
                fps: sprite.fps,
                frame_override: None,
                animation: sprite.animation.clone(),
                anim_start: 0.0,
                flip: [sprite.flip_x, sprite.flip_y],
                tint: sprite.tint,
                order: sprite.order,
                slice: sprite.slice,
                normal_map: sprite.normal_map.clone(),
                parallax: sprite.parallax,
                repeat_x: sprite.repeat_x,
                slice_mode: if sprite.slice_mode.is_empty() {
                    "stretch".into()
                } else {
                    sprite.slice_mode.clone()
                },
                slice_size: sprite.slice_size,
                pivot: sprite.pivot,
                sort_mode: sprite_sort::ObjectSortMode::parse(&sprite.sort_mode),
                light2d: world.get::<Light2d>(id).cloned().unwrap_or_default(),
                camera2d: world.get::<Camera2d>(id).cloned().unwrap_or_default(),
                audio_listener: world.get::<AudioListener>(id).cloned().unwrap_or_default(),
                phys2d: world.get::<Physics2d>(id).cloned().unwrap_or_default(),
                area: world.get::<seal_physics2d::AreaEffect>(id).cloned().unwrap_or_default(),
                surface: world
                    .get::<seal_physics2d::SurfaceEffect>(id)
                    .cloned()
                    .unwrap_or_default(),
                nav2d: world.get::<NavAgent2d>(id).cloned().unwrap_or_default(),
                character2d: world.get::<Character2d>(id).cloned().unwrap_or_default(),
                shape2d: world.get::<Shape2d>(id).cloned().unwrap_or_default(),
                sprite_shape: world.get::<SpriteShape>(id).cloned().unwrap_or_default(),
                follow: world.get::<PathFollow>(id).cloned().unwrap_or_default(),
                material2d: world.get::<Material2d>(id).cloned().unwrap_or_default(),
                mask: world.get::<SpriteClip>(id).map(|c| c.mode.clone()).unwrap_or_default(),
                sprite_mask: world.get::<SpriteMask>(id).cloned().unwrap_or_default(),
                script: world.get::<Script>(id).map(|s| s.graph.clone()).unwrap_or_default(),
                particles: world.get::<Particles>(id).map(|s| s.effect.clone()).unwrap_or_default(),
                socket: String::new(),
                volume: world.get::<crate::volumes::Volume>(id).cloned().unwrap_or_default(),
                script_order: world
                    .get::<crate::volumes::ScriptOrder>(id)
                    .map(|o| o.order)
                    .unwrap_or(0),
                script_group: world
                    .get::<crate::script_groups::ScriptGroup>(id)
                    .map(|g| crate::script_groups::stored(&g.group))
                    .unwrap_or_default(),
                remote: world
                    .get::<crate::remote_transform::RemoteTransform>(id)
                    .cloned()
                    .unwrap_or_default(),
                collider_enabled: world
                    .get::<crate::collider_gate::ColliderGate>(id)
                    .map(|g| g.enabled)
                    .unwrap_or(true),
                merged: world
                    .get::<crate::object_merge::MergedDraw>(id)
                    .cloned()
                    .unwrap_or_default(),
                post: world.get::<crate::post_volume::PostVolume>(id).cloned().unwrap_or_default(),
                mesh_material: world
                    .get::<seal_scene::MeshMaterial>(id)
                    .cloned()
                    .unwrap_or_default(),
            });
        }
        if entities.is_empty() {
            return Err("scene has no entities".into());
        }
        Ok(Self::with_entities(entities, DEFAULT_GRAVITY, 1.0))
    }

    /// The scene file, in the engine scene format.
    pub fn to_text(&self) -> String {
        let mut world = self.to_ecs();
        let settings = SceneSettings {
            gravity: self.gravity,
            time_scale: self.time_scale,
            music: self.music.clone(),
        };
        if settings != SceneSettings::default() {
            world.insert_resource(settings);
        }
        if self.glow != seal_scene::Glow2d::default() {
            world.insert_resource(self.glow.clone());
        }
        if !self.sky_cubemap.is_empty() {
            world.insert_resource(seal_scene::SkyCubemap { texture: self.sky_cubemap.clone() });
        }
        if self.objects_per_file {
            world.insert_resource(crate::object_files::ObjectFiles { on: true });
        }
        if self.join_on {
            world.insert_resource(crate::players::LocalJoin { on: true });
        }
        if !self.physics.scene.is_blank() {
            world.insert_resource(self.physics.scene.clone());
        }
        world.save_scene(None).text
    }

    pub(crate) fn from_scene_text(text: &str) -> Result<Self, String> {
        let mut world = engine_world();
        let order = world.load_scene(text)?;
        let mut out = Self::from_ecs(&world, &order)?;
        let settings = world.resource::<SceneSettings>().cloned().unwrap_or_default();
        out.gravity = settings.gravity;
        out.time_scale = settings.time_scale;
        out.music = settings.music;
        if let Some(glow) = world.resource::<seal_scene::Glow2d>() {
            out.glow = glow.clone();
        }
        if let Some(sky) = world.resource::<seal_scene::SkyCubemap>() {
            out.sky_cubemap = sky.texture.clone();
        }
        out.objects_per_file =
            world.resource::<crate::object_files::ObjectFiles>().is_some_and(|f| f.on);
        out.join_on = world.resource::<crate::players::LocalJoin>().is_some_and(|j| j.on);
        if let Some(scene) = world.resource::<crate::physics::PhysicsSceneSave>() {
            out.physics.scene = scene.clone();
        }
        out.mode = PlayMode::Editing;
        Ok(out)
    }
}
