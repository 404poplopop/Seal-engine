//! Side-view platformer play, built from the scene's roles.
//!
//! When the scene has a `player` object that draws a picture, play runs a 2D
//! platformer on the scene's own objects instead of the 3D rigid-body
//! preview. Objects whose role is `ground` become solid blocks, `coin`
//! objects can be collected, and touching the `goal` after every coin clears
//! the level. Falling off the bottom puts the player back at the start.
//! Stop restores the scene from the snapshot play took, so nothing here
//! needs undoing.

use seal_ecs::core::{SceneSettings, Transform};
use seal_ecs::Entity as Id;
use seal_physics2d::{Body2D, Collider2D, Physics2D, Physics2DSettings, Velocity2D};

use crate::{Entity, Shape, DEFAULT_GRAVITY};

/// Fixed step duration with default project Physics 2D settings (60 Hz).
pub const DEFAULT_TICK_SECONDS: f32 = 1.0 / 60.0;
const RUN_SPEED: f32 = 6.0;
const JUMP_SPEED: f32 = 12.0;
const JUMP_BUFFER: f32 = 0.15;
const COYOTE_TIME: f32 = 0.1;
/// A released jump keeps rising at least this long, so a quick tap still clears a tile.
const MIN_JUMP_TIME: f32 = 0.15;
/// How far below its picture the flag still counts as touched, in metres.
const GOAL_REACH_DOWN: f32 = 2.0;
/// How long the "collect every coin" hint stays up after touching a locked flag.
const GOAL_HINT_TIME: f32 = 2.5;
/// Pull at the default scene gravity; scene gravity scales it.
const BASE_GRAVITY: f32 = 24.0;
const PLAYER_SIZE: [f32; 2] = [0.8, 0.9];

/// Buttons the player holds this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayInput {
    pub left: bool,
    pub right: bool,
    pub jump: bool,
}

pub struct Platformer {
    ecs: seal_ecs::World,
    physics: Physics2D,
    step: f32,
    max_substeps: u32,
    player: (String, Id),
    size: [f32; 2],
    start: [f32; 2],
    coins: Vec<(String, Id)>,
    goal: Option<Id>,
    fall_limit: f32,
    jump_held: bool,
    /// Seconds a jump press stays queued, so a quick tap between steps or just
    /// before landing still jumps.
    jump_buffer: f32,
    /// Seconds since the feet last touched ground, so a jump right after a seam
    /// bump or a ledge still counts.
    coyote: f32,
    /// Seconds since the last take-off, while rising.
    air_time: Option<f32>,
    /// Jump let go early: cut the rise once `MIN_JUMP_TIME` has passed.
    cut_pending: bool,
    carry: f32,
    /// Objects with their own 2D body, simulated with the player.
    bodies: crate::phys2d::Bodies,
    pub coins_total: usize,
    pub coins_got: usize,
    pub falls: u32,
    pub won: bool,
    /// Seconds left on the hint shown after touching the flag with coins missing.
    pub goal_hint: f32,
}

fn at(x: f32, y: f32) -> Transform {
    Transform { translation: [x, y, 0.0], ..Default::default() }
}

/// Width and height an object covers in the side view.
fn extent(entity: &Entity) -> [f32; 2] {
    let s = entity.scale.map(f32::abs);
    match entity.shape {
        Some(Shape::Plane) => [40.0 * s[0], 0.1 * s[1]],
        _ => [s[0], s[1]],
    }
}

impl Platformer {
    /// The scene as a platformer, or `None` when it has no player or is not a
    /// side-view level.
    pub fn from_entities(
        entities: &[Entity],
        gravity: f32,
        settings: &Physics2DSettings,
    ) -> Option<Self> {
        let player = entities.iter().find(|e| e.tag == "player" && !e.hidden)?;
        // A 2D body of its own is a top-down character. The side-view game
        // only claims a picture player that has no body yet.
        if !player.phys2d.body.is_empty() {
            return None;
        }
        // A 3D scene's player start has no picture; a side-view level has
        // pictures on its player or on its gameplay tiles.
        let side_view = !player.sprite.is_empty()
            || entities.iter().any(|e| {
                !e.sprite.is_empty() && matches!(e.tag.as_str(), "ground" | "coin" | "goal")
            });
        if !side_view {
            return None;
        }
        let mut ecs = seal_ecs::World::new();
        ecs.register_core();
        seal_physics2d::register_physics2d(&mut ecs);
        ecs.insert_resource(SceneSettings {
            gravity: BASE_GRAVITY * gravity / DEFAULT_GRAVITY,
            time_scale: 1.0,
            music: String::new(),
        });

        // Neighbouring ground tiles become a few rectangles, so the player
        // does not catch on the seams between them.
        let tiles: Vec<([f32; 2], [f32; 2])> = entities
            .iter()
            .filter(|e| e.tag == "ground" && !e.hidden && e.phys2d.body.is_empty())
            .map(|e| ([e.translation[0], e.translation[1]], extent(e)))
            .collect();
        let blocks = crate::tile_merge::merge_tiles(&tiles);
        let mut lowest = player.translation[1];
        for block in &blocks {
            lowest = lowest.min(block.center[1] - block.size[1] * 0.5);
            ecs.spawn((
                at(block.center[0], block.center[1]),
                Body2D::fixed(),
                Collider2D { size: block.size, friction: 0.0, ..Default::default() },
            ));
        }
        let coins: Vec<(String, Id)> = entities
            .iter()
            .filter(|e| e.tag == "coin" && !e.hidden)
            .map(|e| {
                // A little larger than the picture, so brushing past a coin counts.
                let d = extent(e)[0].max(0.1) * 1.3;
                let id = ecs.spawn((
                    at(e.translation[0], e.translation[1]),
                    Body2D::fixed(),
                    Collider2D {
                        shape: "circle".into(),
                        size: [d, d],
                        sensor: true,
                        ..Default::default()
                    },
                ));
                (e.id.clone(), id)
            })
            .collect();
        let goal = entities.iter().find(|e| e.tag == "goal" && !e.hidden).map(|e| {
            // The flag's zone reaches down to the ground, so a flag placed a tile
            // above the floor is still reached by walking into it.
            let size = extent(e);
            let reach = GOAL_REACH_DOWN;
            ecs.spawn((
                at(e.translation[0], e.translation[1] - reach * 0.5),
                Body2D::fixed(),
                Collider2D {
                    size: [size[0] * 0.6, size[1] + reach],
                    sensor: true,
                    ..Default::default()
                },
            ))
        });
        let bodies = crate::phys2d::Bodies::spawn(&mut ecs, entities);
        let s = player.scale.map(f32::abs);
        let size = [PLAYER_SIZE[0] * s[0].max(0.1), PLAYER_SIZE[1] * s[1].max(0.1)];
        let start = [player.translation[0], player.translation[1]];
        let id = ecs.spawn((
            at(start[0], start[1]),
            Body2D { lock_rotation: true, ..Default::default() },
            Collider2D { size, friction: 0.0, ..Default::default() },
            Velocity2D::default(),
        ));
        let step = settings.fixed_step();
        let max_substeps = settings.max_substeps;
        let mut physics = Physics2D::new();
        physics.settings = settings.clone();
        Some(Self {
            ecs,
            physics,
            step,
            max_substeps,
            player: (player.id.clone(), id),
            size,
            start,
            coins_total: coins.len(),
            coins,
            goal,
            fall_limit: lowest - 4.0,
            jump_held: false,
            jump_buffer: 0.0,
            coyote: 0.0,
            air_time: None,
            cut_pending: false,
            carry: 0.0,
            bodies,
            coins_got: 0,
            falls: 0,
            won: false,
            goal_hint: 0.0,
        })
    }

    pub fn start(&self) -> [f32; 2] {
        self.start
    }

    pub fn player_id(&self) -> &str {
        &self.player.0
    }

    fn support(&self) -> Option<Id> {
        let t = self.ecs.get::<Transform>(self.player.1)?;
        let [x, y, _] = t.translation;
        let foot = y - self.size[1] * 0.5 - 0.01;
        let reach = self.size[0] * 0.44;
        [-reach, 0.0, reach].iter().find_map(|dx| {
            self.physics.raycast([x + dx, foot], [0.0, -1.0], 0.08).and_then(|hit| {
                self.ecs.get::<Collider2D>(hit.entity).filter(|c| !c.sensor).map(|_| hit.entity)
            })
        })
    }

    fn grounded(&self) -> bool {
        self.support().is_some()
    }

    fn tick(&mut self, input: PlayInput, entities: &mut [Entity]) {
        if self.grounded() {
            self.coyote = COYOTE_TIME;
        }
        let axis = input.right as i32 as f32 - input.left as i32 as f32;
        if let Some(v) = self.ecs.get_mut::<Velocity2D>(self.player.1) {
            v.linear[0] = axis * RUN_SPEED;
            if self.jump_buffer > 0.0 && self.coyote > 0.0 && v.linear[1] <= 0.5 {
                v.linear[1] = JUMP_SPEED;
                self.jump_buffer = 0.0;
                self.coyote = 0.0;
                self.air_time = Some(0.0);
                self.cut_pending = !input.jump;
            }
            if let Some(t) = self.air_time {
                if v.linear[1] <= 0.0 {
                    self.air_time = None;
                    self.cut_pending = false;
                } else if self.cut_pending && t >= MIN_JUMP_TIME {
                    if v.linear[1] > JUMP_SPEED * 0.5 {
                        v.linear[1] *= 0.5;
                    }
                    self.air_time = None;
                    self.cut_pending = false;
                } else {
                    self.air_time = Some(t + self.step);
                }
            }
        }
        self.jump_buffer = (self.jump_buffer - self.step).max(0.0);
        self.coyote = (self.coyote - self.step).max(0.0);
        self.goal_hint = (self.goal_hint - self.step).max(0.0);
        // The player collider has no friction, so a kinematic platform sliding
        // sideways would leave them behind. Move them with the platform they stand on.
        let shifts = self.bodies.kinematic_deltas(&self.ecs, entities);
        if let Some(support) = self.support() {
            if let Some((_, [dx, dy])) = shifts.iter().find(|(id, _)| *id == support) {
                if let Some(t) = self.ecs.get_mut::<Transform>(self.player.1) {
                    t.translation[0] += dx;
                    t.translation[1] += dy;
                }
            }
        }
        self.bodies.pull(&mut self.ecs, entities);
        if self.physics.step(&mut self.ecs, self.step).is_err() {
            return;
        }
        for contact in self.physics.contacts.clone() {
            if !contact.started || !contact.sensor {
                continue;
            }
            let other = if contact.a == self.player.1 {
                contact.b
            } else if contact.b == self.player.1 {
                contact.a
            } else {
                continue;
            };
            if Some(other) == self.goal {
                if self.coins_got == self.coins_total {
                    self.won = true;
                } else {
                    self.goal_hint = GOAL_HINT_TIME;
                }
            } else if let Some(i) = self.coins.iter().position(|c| c.1 == other) {
                let (scene_id, id) = self.coins.remove(i);
                self.ecs.despawn(id);
                self.coins_got += 1;
                if let Some(e) = entities.iter_mut().find(|e| e.id == scene_id) {
                    e.hidden = true;
                }
            }
        }
        let y = self.ecs.get::<Transform>(self.player.1).map(|t| t.translation[1]).unwrap_or(0.0);
        if y < self.fall_limit {
            self.falls += 1;
            if let Some(t) = self.ecs.get_mut::<Transform>(self.player.1) {
                t.translation = [self.start[0], self.start[1], 0.0];
            }
            if let Some(v) = self.ecs.get_mut::<Velocity2D>(self.player.1) {
                *v = Velocity2D::default();
            }
        }
    }

    /// Run whole fixed steps covering `dt` seconds, then copy the player back into the scene.
    pub fn step(&mut self, dt: f32, input: PlayInput, entities: &mut [Entity]) {
        self.bodies.sync_colliders(&mut self.ecs, entities);
        // Presses are read once per frame: a frame can hold no fixed step at all.
        let pressed = input.jump && !self.jump_held;
        let released = !input.jump && self.jump_held;
        self.jump_held = input.jump;
        if pressed {
            self.jump_buffer = JUMP_BUFFER;
        }
        // Letting go early gives a short hop.
        if released {
            self.cut_pending = true;
        }
        if pressed {
            self.cut_pending = false;
        }
        let cap = self.step * self.max_substeps as f32;
        self.carry = (self.carry + dt.max(0.0)).min(cap);
        while self.carry >= self.step {
            self.carry -= self.step;
            self.tick(input, entities);
        }
        if self.physics.settings.render_interpolation {
            let alpha = if self.step > 0.0 { self.carry / self.step } else { 0.0 };
            self.physics.write_transforms(&mut self.ecs, alpha);
        }
        self.bodies.push(&self.ecs, entities);
        let Some(t) = self.ecs.get::<Transform>(self.player.1).map(|t| t.translation) else {
            return;
        };
        let v = self.ecs.get::<Velocity2D>(self.player.1).map(|v| v.linear).unwrap_or_default();
        if let Some(e) = entities.iter_mut().find(|e| e.id == self.player.0) {
            e.translation[0] = t[0];
            e.translation[1] = t[1];
            e.velocity = [v[0], v[1], 0.0];
        }
    }

    /// One fixed step, for stepping frame by frame while paused.
    /// Drop any partly accumulated time, so the next `step` runs exactly one tick.
    pub fn clear_carry(&mut self) {
        self.carry = 0.0;
    }

    /// Seconds per physics tick.
    pub fn tick_seconds(&self) -> f32 {
        self.step
    }

    /// Who entered, left, and remains inside each area, including the player.
    pub fn joint_break_scene_ids(&self) -> Vec<String> {
        crate::phys2d::joint_break_scene_ids(&self.physics, &self.bodies)
    }

    pub fn clear_broken_joints(&mut self, entities: &mut [Entity]) {
        for scene_id in self.joint_break_scene_ids() {
            crate::phys2d::clear_joint_on_entity(entities, &scene_id);
        }
    }

    pub fn area_report(&self) -> crate::area_effect::Report {
        let mut ids = self.bodies.scene_of();
        ids.insert(self.player.1, self.player.0.clone());
        crate::area_effect::report(&self.physics, &ids)
    }
}

#[cfg(test)]
mod tests {
    use crate::{PlayInput, PlayMode, World};

    fn level() -> World {
        let mut w = World::demo();
        w.entities.retain(|e| e.id == "root");
        let mut put = |kind: &str, tag: &str, x: f32, y: f32| {
            let id = w.add(kind);
            let e = w.entity_mut(&id).unwrap();
            e.tag = tag.into();
            e.translation = [x, y, 0.0];
            e.dynamic = false;
            if tag == "player" {
                e.sprite = "sprites/seal.png".into();
            }
            if tag == "coin" {
                e.scale = [0.6, 0.6, 1.0];
            }
            id
        };
        for x in 0..12 {
            put("empty", "ground", x as f32 + 0.5, 0.5);
        }
        put("empty", "coin", 5.5, 1.5);
        put("empty", "goal", 10.5, 2.0);
        put("empty", "player", 1.5, 1.5);
        w
    }

    #[test]
    fn a_player_with_its_own_body_is_not_a_side_view_level() {
        let mut w = World::from_text(&World::empty_scene_text("Top")).unwrap();
        let id = w.add("empty");
        {
            let e = w.entity_mut(&id).unwrap();
            e.tag = "player".into();
            e.sprite = "sprites/hero.png".into();
            assert!(crate::phys2d::apply(&mut e.phys2d, "phys_body", "dynamic"));
        }
        assert!(super::Platformer::from_entities(
            &w.entities,
            0.0,
            &seal_physics2d::Physics2DSettings::default()
        )
        .is_none());
    }

    fn run(w: &mut World, input: PlayInput, frames: u32) {
        w.play_input = input;
        for _ in 0..frames {
            w.step(1.0 / 60.0);
        }
    }

    #[test]
    fn running_right_collects_the_coin_and_clears_the_level() {
        let mut w = level();
        w.play();
        assert_eq!(w.play_status().unwrap().coins_total, 1);
        run(&mut w, PlayInput::default(), 30);
        let player = w.play_player().unwrap().to_string();
        let rest = w.entity(&player).unwrap().translation[1];
        assert!((rest - 1.45).abs() < 0.05, "player rests on the ground, got y={rest}");
        run(&mut w, PlayInput { right: true, ..Default::default() }, 150);
        let status = w.play_status().unwrap();
        assert_eq!(status.coins, 1);
        assert!(status.won);
        let coin = w.entities.iter().find(|e| e.tag == "coin").unwrap();
        assert!(coin.hidden, "a collected coin is hidden");
        w.stop();
        assert_eq!(w.mode, PlayMode::Editing);
        assert!(!w.entities.iter().find(|e| e.tag == "coin").unwrap().hidden);
        assert_eq!(w.entity(&player).unwrap().translation[0], 1.5);
    }

    #[test]
    fn a_flag_a_tile_above_the_floor_is_still_reached_and_hints_while_coins_are_missing() {
        let mut w = level();
        let flag = w.entities.iter().find(|e| e.tag == "goal").unwrap().id.clone();
        w.entity_mut(&flag).unwrap().translation[1] = 2.5;
        let coin = w.entities.iter().find(|e| e.tag == "coin").unwrap().id.clone();
        w.entity_mut(&coin).unwrap().translation = [9.0, 1.5, 0.0];
        let extra = w.add("empty");
        let e = w.entity_mut(&extra).unwrap();
        e.tag = "coin".into();
        e.translation = [3.0, 6.0, 0.0];
        w.play();
        run(&mut w, PlayInput { right: true, ..Default::default() }, 150);
        let status = w.play_status().unwrap();
        assert!(!status.won);
        assert!(status.goal_locked, "touching the flag with a coin missing shows the hint");

        let mut w = level();
        let flag = w.entities.iter().find(|e| e.tag == "goal").unwrap().id.clone();
        w.entity_mut(&flag).unwrap().translation[1] = 2.5;
        w.play();
        run(&mut w, PlayInput { right: true, ..Default::default() }, 150);
        assert!(w.play_status().unwrap().won, "walking under a raised flag still wins");
    }

    #[test]
    fn a_player_without_a_picture_still_plays() {
        let mut w = level();
        for e in w.entities.iter_mut() {
            e.sprite = if e.tag == "ground" { "sprites/desk.png".into() } else { String::new() };
        }
        w.play();
        let player = w.play_player().expect("an empty tagged Player still plays").to_string();
        let x0 = w.entity(&player).unwrap().translation[0];
        run(&mut w, PlayInput { right: true, ..Default::default() }, 30);
        assert!(w.entity(&player).unwrap().translation[0] > x0 + 0.5);
    }

    #[test]
    fn a_one_frame_tap_on_a_fast_frame_still_jumps() {
        let mut w = level();
        w.play();
        run(&mut w, PlayInput::default(), 20);
        let player = w.play_player().unwrap().to_string();
        let rest = w.entity(&player).unwrap().translation[1];
        // A 240 Hz frame holds no fixed step; the tap must not be lost.
        w.play_input = PlayInput { jump: true, ..Default::default() };
        w.step(1.0 / 240.0);
        run(&mut w, PlayInput::default(), 12);
        let up = w.entity(&player).unwrap().translation[1];
        assert!(up > rest + 0.5, "a tap hops, rest y={rest}, got y={up}");
    }

    #[test]
    fn a_short_tap_still_jumps_over_a_tile() {
        let mut w = level();
        w.play();
        run(&mut w, PlayInput::default(), 30);
        let player = w.play_player().unwrap().to_string();
        let y0 = w.entity(&player).unwrap().translation[1];
        // An 80 ms tap: pressed for about five frames, then let go.
        run(&mut w, PlayInput { jump: true, ..Default::default() }, 5);
        let mut top = y0;
        for _ in 0..60 {
            run(&mut w, PlayInput::default(), 1);
            top = top.max(w.entity(&player).unwrap().translation[1]);
        }
        assert!(top - y0 > 1.5, "a quick tap rises over a tile, got {}", top - y0);
    }

    #[test]
    fn jumping_leaves_the_ground_and_lands_again() {
        let mut w = level();
        w.play();
        run(&mut w, PlayInput::default(), 20);
        let player = w.play_player().unwrap().to_string();
        run(&mut w, PlayInput { jump: true, ..Default::default() }, 15);
        let up = w.entity(&player).unwrap().translation[1];
        assert!(up > 3.0, "jump rises, got y={up}");
        run(&mut w, PlayInput::default(), 90);
        let down = w.entity(&player).unwrap().translation[1];
        assert!((down - 1.45).abs() < 0.05, "lands again, got y={down}");
    }

    #[test]
    fn a_frame_step_while_paused_advances_time_and_particles_too() {
        let mut w = level();
        w.play();
        let player = w.entities.iter().find(|e| e.tag == "player").unwrap().id.clone();
        w.play_emitter(
            &player,
            crate::particles::EmitterDef { rate: 120.0, lifetime: 5.0, ..Default::default() },
        );
        w.pause();
        for _ in 0..10 {
            assert!(w.step_once());
        }
        assert_eq!(w.mode, PlayMode::Paused);
        assert!((w.play_time - 10.0 * super::DEFAULT_TICK_SECONDS).abs() < 1e-4, "{}", w.play_time);
        assert!(w.emitters[0].1.particles.len() >= 18, "{}", w.emitters[0].1.particles.len());
    }

    #[test]
    fn falling_off_the_edge_respawns_at_the_start() {
        let mut w = level();
        w.play();
        run(&mut w, PlayInput { left: true, ..Default::default() }, 120);
        let status = w.play_status().unwrap();
        assert!(status.falls >= 1);
        let player = w.play_player().unwrap().to_string();
        assert!(w.entity(&player).unwrap().translation[1] > -4.0);
    }

    #[test]
    fn a_moving_platform_carries_the_player() {
        let mut w = level();
        let rail = w.add("empty");
        {
            let e = w.entity_mut(&rail).unwrap();
            e.name = "Rail".into();
            e.translation = [4.0, 3.2, 0.0];
            e.shape2d.kind = "line".into();
            e.shape2d.points = vec![[0.0, 0.0], [4.0, 0.0]];
            e.hidden = true;
        }
        let plat = w.add("empty");
        {
            let e = w.entity_mut(&plat).unwrap();
            e.name = "Mover".into();
            e.translation = [4.0, 3.2, 0.0];
            e.scale = [2.5, 0.35, 1.0];
            e.phys2d.body = "kinematic".into();
            e.follow.path = "Rail".into();
            e.follow.speed = 2.0;
            e.follow.mode = "loop".into();
        }
        let player = w.entities.iter().find(|e| e.tag == "player").unwrap().id.clone();
        w.entity_mut(&player).unwrap().translation = [4.0, 3.86, 0.0];
        w.play();
        let x0 = w.entity(&player).unwrap().translation[0];
        run(&mut w, PlayInput::default(), 90);
        let p = w.entity(&player).unwrap().translation;
        assert!(p[0] > x0 + 1.0, "carried {x0} -> {}", p[0]);
        assert!(p[1] > 3.0, "stayed on it, y {}", p[1]);
    }

    #[test]
    fn a_scene_without_a_pictured_player_does_not_use_side_view_play() {
        let mut w = World::demo();
        w.play();
        assert!(w.play_status().is_none());
    }

    #[test]
    fn project_fixed_hz_runs_more_steps_per_frame() {
        let w = level();
        let mut settings = seal_physics2d::Physics2DSettings::default();
        settings.fixed_hz = 120.0;
        let mut entities = w.entities.clone();
        let mut game =
            super::Platformer::from_entities(&entities, crate::DEFAULT_GRAVITY, &settings).unwrap();
        assert!((game.tick_seconds() - 1.0 / 120.0).abs() < 1e-6);
        let before = game.physics.profiler.step_history.len();
        game.step(1.0 / 60.0, PlayInput::default(), &mut entities);
        let after = game.physics.profiler.step_history.len();
        assert_eq!(after - before, 2, "one 60 Hz display frame advances two 120 Hz physics steps");
    }

    #[test]
    fn default_settings_keep_the_same_player_path_as_60_hz() {
        let mut w = level();
        w.play();
        run(&mut w, PlayInput::default(), 25);
        run(&mut w, PlayInput { right: true, ..Default::default() }, 60);
        let player = w.play_player().unwrap().to_string();
        let pos = w.entity(&player).unwrap().translation;
        assert!(
            (pos[0] - 7.5).abs() < 0.08 && (pos[1] - 1.45).abs() < 0.08,
            "default 60 Hz path, got {:?}",
            pos
        );
    }

    #[test]
    fn render_interpolation_draws_the_player_between_fixed_steps() {
        let w = level();
        let input = PlayInput { right: true, ..Default::default() };
        let mut plain = w.entities.clone();
        let mut game =
            super::Platformer::from_entities(&plain, crate::DEFAULT_GRAVITY, &Default::default())
                .unwrap();
        game.step(1.0 / 60.0, input, &mut plain);
        game.step(1.0 / 120.0, input, &mut plain);
        let x_plain = plain.iter().find(|e| e.tag == "player").unwrap().translation[0];

        let mut settings = seal_physics2d::Physics2DSettings::default();
        settings.render_interpolation = true;
        let mut interp = w.entities.clone();
        let mut game_i =
            super::Platformer::from_entities(&interp, crate::DEFAULT_GRAVITY, &settings).unwrap();
        game_i.step(1.0 / 60.0, input, &mut interp);
        game_i.step(1.0 / 120.0, input, &mut interp);
        let x_half = interp.iter().find(|e| e.tag == "player").unwrap().translation[0];

        assert!(
            (x_half - x_plain).abs() > 1e-4,
            "interpolation shifts the draw mid-step, plain={x_plain} interp={x_half}"
        );
    }
}
