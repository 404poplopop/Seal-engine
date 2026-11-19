//! Shared 3D arena content for first- and third-person starter templates.

mod graphs;

use std::path::Path;

use seal_world::{Shape, World};

use crate::error::CoreError;

pub use graphs::{parse_all, FPS_CAMERA, FPS_LOOK, TPS_CAMERA};

use graphs::{FPS_CAMERA as FPS_CAMERA_GRAPH, FPS_LOOK as FPS_LOOK_GRAPH, TPS_CAMERA as TPS_CAMERA_GRAPH};

const PREVIEW_ARENA_PX: [u8; 128] = {
    const F: [u8; 4] = [120, 160, 90, 255];
    const W: [u8; 4] = [90, 70, 55, 255];
    const P: [u8; 4] = [200, 120, 60, 255];
    const D: [u8; 4] = [40, 40, 60, 255];
    let rows: [[[u8; 4]; 8]; 4] = [
        [D, D, D, D, D, D, D, D],
        [W, F, F, W, W, P, F, W],
        [W, F, P, F, F, F, F, W],
        [W, W, W, W, W, W, W, W],
    ];
    let mut out = [0u8; 128];
    let mut i = 0;
    while i < 128 {
        out[i] = rows[i / 32][(i / 4) % 8][i % 4];
        i += 1;
    }
    out
};

pub const PREVIEW_ARENA: (&'static [u8], u32, u32) = (&PREVIEW_ARENA_PX, 8, 4);

pub(crate) const INPUT: &str = "\
# First- and third-person controls
action move_forward = key.w, key.arrowup, pad.dpad_up
action move_back = key.s, key.arrowdown, pad.dpad_down
action move_left = key.a, key.arrowleft, pad.dpad_left
action move_right = key.d, key.arrowright, pad.dpad_right
action jump = key.space, pad.south
action look = mouse.right
axis move_x = pad.left_x deadzone 0.25
axis move_y = pad.left_y deadzone 0.25
";

pub(crate) const HUD: &str = r#"seal ui 1
panel Hud
  anchor 0 0 0 0
  offset 16 16 280 56
  label GoalLabel
    text "Goal: reach the pickup"
"#;

pub(crate) const GOAL_GRAPH: &str = r#"schema_version = 1
seal_graph = "goal_pickup"
vars = []
nodes = [
  { id = 1, kind = "event", name = "pickup_enter", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 3, kind = "compare", type = "str", op = "eq", defaults = { b = "player" } },
  { id = 4, kind = "branch" },
  { id = 5, kind = "add_score", defaults = { name = "goal", amount = 1 } },
  { id = 6, kind = "ui_text", widget = "Hud", element = "GoalLabel", text = "Goal reached!" },
  { id = 7, kind = "set_game_state", state = "won" },
  { id = 8, kind = "self" },
  { id = 9, kind = "despawn" },
]
links = [
  "1.then -> 4.in",
  "1.other -> 2.entity",
  "2.out -> 3.a",
  "3.out -> 4.cond",
  "4.true -> 5.in",
  "5.then -> 6.in",
  "6.then -> 7.in",
  "7.then -> 9.in",
  "8.out -> 9.entity",
]
"#;

pub(crate) const GAME_GRAPH: &str = r#"schema_version = 1
seal_graph = "game"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "find_first", prefab = "player" },
  { id = 3, kind = "possess", index = 0 },
  { id = 4, kind = "ui_show", widget = "Hud", on = true },
  { id = 5, kind = "set_game_state", state = "playing" },
]
links = [
  "1.then -> 3.in",
  "2.out -> 3.entity",
  "3.then -> 4.in",
  "4.then -> 5.in",
]
"#;

pub fn write_shared(root: &Path, first_person: bool) -> Result<(), CoreError> {
    std::fs::create_dir_all(root.join("logic"))?;
    std::fs::create_dir_all(root.join("ui"))?;
    std::fs::write(root.join("input.sealinput"), INPUT)?;
    std::fs::write(root.join("ui/Hud.sealui"), HUD)?;
    std::fs::write(root.join("logic/game.graph"), GAME_GRAPH)?;
    std::fs::write(root.join("logic/goal_pickup.graph"), GOAL_GRAPH)?;
    if first_person {
        std::fs::write(root.join("logic/fps_look.graph"), FPS_LOOK_GRAPH)?;
        std::fs::write(root.join("logic/fps_camera.graph"), FPS_CAMERA_GRAPH)?;
    } else {
        std::fs::write(root.join("logic/tps_camera.graph"), TPS_CAMERA_GRAPH)?;
    }
    Ok(())
}

/// Arena with floor, walls, boxes, lights, player, goal, and game manager.
pub fn build_arena(first_person: bool) -> World {
    let mut world = World::starter();
    world.entities.retain(|e| {
        !matches!(
            e.id.as_str(),
            "crate" | "shape_red" | "shape_teal" | "shape_green" | "shape_blue" | "shape_gold" | "shape_violet"
        )
    });
    world.gravity = seal_world::DEFAULT_GRAVITY;
    for e in &mut world.entities {
        if e.id == "hero" {
            e.id = "player".into();
            e.name = "Player".into();
            e.tag = "player".into();
            e.source_asset = "player".into();
            e.shape = Some(Shape::Capsule);
            e.dynamic = true;
            e.walk_speed = 4.5;
            e.translation = [0.0, 1.0, 2.0];
            e.scale = [0.6, 1.8, 0.6];
            if first_person {
                e.script = "logic/fps_look.graph".into();
            }
        }
        if e.id == "floor" {
            e.scale = [6.0, 1.0, 6.0];
        }
    }
    add_wall(&mut world, "wall_n", "Wall North", [0.0, 1.5, -6.0], [12.0, 3.0, 0.4]);
    add_wall(&mut world, "wall_s", "Wall South", [0.0, 1.5, 6.0], [12.0, 3.0, 0.4]);
    add_wall(&mut world, "wall_w", "Wall West", [-6.0, 1.5, 0.0], [0.4, 3.0, 12.0]);
    add_wall(&mut world, "wall_e", "Wall East", [6.0, 1.5, 0.0], [0.4, 3.0, 12.0]);
    add_box(&mut world, "box_a", [2.5, 0.5, -1.0]);
    add_box(&mut world, "box_b", [-2.0, 0.5, 1.5]);
    add_box(&mut world, "box_c", [1.0, 0.5, 3.0]);
    let goal = world.add("sphere");
    if let Some(e) = world.entity_mut(&goal) {
        e.name = "Goal Pickup".into();
        e.tag = "goal".into();
        e.translation = [3.5, 0.8, -3.5];
        e.scale = [0.6, 0.6, 0.6];
        e.color = [0.95, 0.85, 0.2];
        e.metal = 0.6;
        e.script = "logic/goal_pickup.graph".into();
        e.volume.kind = "trigger".into();
        e.volume.event = "pickup".into();
        e.volume.tag = "player".into();
    }
    let cam = world.add("empty");
    if let Some(e) = world.entity_mut(&cam) {
        e.id = "play_camera".into();
        e.name = "Play Camera".into();
        e.tag = "camera".into();
        e.translation = [0.0, 2.5, 5.0];
        e.fov = 70.0;
        e.script = if first_person {
            "logic/fps_camera.graph".into()
        } else {
            "logic/tps_camera.graph".into()
        };
    }
    let game = world.add("empty");
    if let Some(e) = world.entity_mut(&game) {
        e.id = "game".into();
        e.name = "Game".into();
        e.script = "logic/game.graph".into();
        e.script_order = -10;
    }
    world
}

fn add_wall(world: &mut World, id: &str, name: &str, at: [f32; 3], scale: [f32; 3]) {
    let eid = world.add("cube");
    if let Some(e) = world.entity_mut(&eid) {
        e.id = id.into();
        e.name = name.into();
        e.parent = Some("geo".into());
        e.translation = at;
        e.scale = scale;
        e.dynamic = false;
        e.color = [0.55, 0.5, 0.48];
    }
}

fn add_box(world: &mut World, id: &str, at: [f32; 3]) {
    let eid = world.add("cube");
    if let Some(e) = world.entity_mut(&eid) {
        e.id = id.into();
        e.name = format!("Box {}", id.split('_').nth(1).unwrap_or(""));
        e.parent = Some("geo".into());
        e.translation = at;
        e.scale = [1.0, 1.0, 1.0];
        e.dynamic = false;
        e.color = [0.72, 0.45, 0.28];
    }
}