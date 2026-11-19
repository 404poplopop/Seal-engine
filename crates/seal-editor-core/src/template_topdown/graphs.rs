use std::fs;
use std::path::Path;

use crate::error::CoreError;

pub fn write_all(folder: &Path) -> Result<(), CoreError> {
    for (name, body) in GRAPHS {
        let path = folder.join(format!("logic/{name}.graph"));
        fs::write(path, body)?;
    }
    Ok(())
}

const GRAPHS: &[(&str, &str)] = &[
    ("player", PLAYER),
    ("pickup", PICKUP),
    ("enemy", ENEMY),
    ("npc", NPC),
    ("door_village", DOOR_VILLAGE),
    ("door_cave", DOOR_CAVE),
    ("save", SAVE),
    ("game", GAME),
];

const PLAYER: &str = r#"schema_version = 1
seal_graph = "player"
vars = [
  { name = "facing", type = "str", default = "down" },
  { name = "safe", type = "float", default = 0.0 },
]
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "self" },
  { id = 3, kind = "keep_across_scenes", defaults = { keep = true } },
  { id = 4, kind = "event", name = "tick" },
  { id = 5, kind = "input_axis", defaults = { name = "move_x" } },
  { id = 6, kind = "input_axis", defaults = { name = "move_y" } },
  { id = 7, kind = "mul", type = "float", defaults = { b = 4.5 } },
  { id = 8, kind = "mul", type = "float", defaults = { b = 4.5 } },
  { id = 9, kind = "vec3" },
  { id = 10, kind = "self" },
  { id = 11, kind = "world_set", component = "body", field = "velocity", type = "vec3" },
  { id = 12, kind = "self" },
  { id = 13, kind = "world_set", component = "transform", field = "rotation", type = "vec3", defaults = { value = [0.0, 0.0, 0.0] } },
  { id = 14, kind = "abs", type = "float" },
  { id = 15, kind = "abs", type = "float" },
  { id = 16, kind = "compare", type = "float", op = "gt" },
  { id = 17, kind = "compare", type = "float", op = "gt", defaults = { b = 0.2 } },
  { id = 18, kind = "compare", type = "float", op = "gt", defaults = { b = 0.2 } },
  { id = 19, kind = "or" },
  { id = 20, kind = "compare", type = "float", op = "gt", defaults = { b = 0.0 } },
  { id = 21, kind = "compare", type = "float", op = "gt", defaults = { b = 0.0 } },
  { id = 22, kind = "const", type = "str", value = "right" },
  { id = 23, kind = "const", type = "str", value = "left" },
  { id = 24, kind = "const", type = "str", value = "up" },
  { id = 25, kind = "const", type = "str", value = "down" },
  { id = 26, kind = "select", type = "str" },
  { id = 27, kind = "select", type = "str" },
  { id = 28, kind = "select", type = "str" },
  { id = 29, kind = "get", name = "facing" },
  { id = 30, kind = "select", type = "str" },
  { id = 31, kind = "set", name = "facing" },
  { id = 32, kind = "const", type = "str", value = "walk_" },
  { id = 33, kind = "const", type = "str", value = "idle_" },
  { id = 34, kind = "select", type = "str" },
  { id = 35, kind = "concat" },
  { id = 36, kind = "self" },
  { id = 37, kind = "play_clip" },
  { id = 38, kind = "event", name = "scene_loaded" },
  { id = 39, kind = "current_scene" },
  { id = 40, kind = "const", type = "str", value = "scenes/cave.seal" },
  { id = 41, kind = "compare", type = "str", op = "eq" },
  { id = 42, kind = "game_state" },
  { id = 43, kind = "const", type = "str", value = "restart" },
  { id = 44, kind = "compare", type = "str", op = "eq" },
  { id = 45, kind = "const", type = "float", value = -2.4 },
  { id = 46, kind = "const", type = "float", value = -4.5 },
  { id = 47, kind = "const", type = "float", value = 4.4 },
  { id = 48, kind = "const", type = "float", value = 0.0 },
  { id = 49, kind = "select", type = "float" },
  { id = 50, kind = "select", type = "float" },
  { id = 51, kind = "vec3" },
  { id = 52, kind = "self" },
  { id = 53, kind = "world_set", component = "transform", field = "translation", type = "vec3" },
  { id = 54, kind = "event", name = "overlap_begin", params = [{ name = "other", type = "entity" }] },
  { id = 55, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 56, kind = "compare", type = "str", op = "eq", defaults = { b = "enemy" } },
  { id = 57, kind = "game_time" },
  { id = 58, kind = "get", name = "safe" },
  { id = 59, kind = "compare", type = "float", op = "ge" },
  { id = 60, kind = "and" },
  { id = 61, kind = "branch" },
  { id = 62, kind = "add_score", defaults = { name = "hearts", amount = -1 } },
  { id = 63, kind = "play_sound", defaults = { sound = "audio/hurt.wav", volume = 0.8, pitch = 1.0 } },
  { id = 64, kind = "const", type = "float", value = 0.5 },
  { id = 65, kind = "add", type = "float" },
  { id = 66, kind = "set", name = "safe" },
]
links = [
  "1.then -> 3.in", "2.out -> 3.entity",
  "4.then -> 11.in", "11.then -> 13.in", "13.then -> 31.in", "31.then -> 37.in",
  "5.out -> 7.a", "7.out -> 9.x", "6.out -> 8.a", "8.out -> 9.y",
  "10.out -> 11.entity", "9.out -> 11.value", "12.out -> 13.entity",
  "5.out -> 14.a", "5.out -> 20.a", "6.out -> 15.a", "6.out -> 21.a",
  "14.out -> 16.a", "15.out -> 16.b", "14.out -> 17.a", "15.out -> 18.a",
  "17.out -> 19.a", "18.out -> 19.b", "16.out -> 28.cond",
  "20.out -> 26.cond", "22.out -> 26.a", "23.out -> 26.b",
  "21.out -> 27.cond", "24.out -> 27.a", "25.out -> 27.b",
  "26.out -> 28.a", "27.out -> 28.b",
  "19.out -> 30.cond", "28.out -> 30.a", "29.out -> 30.b", "30.out -> 31.value",
  "19.out -> 34.cond", "32.out -> 34.a", "33.out -> 34.b",
  "34.out -> 35.a", "30.out -> 35.b", "35.out -> 37.clip", "36.out -> 37.entity",
  "38.then -> 53.in", "39.out -> 41.a", "40.out -> 41.b", "42.out -> 44.a", "43.out -> 44.b",
  "44.out -> 49.cond", "46.out -> 49.a", "47.out -> 49.b",
  "41.out -> 50.cond", "45.out -> 50.a", "49.out -> 50.b",
  "50.out -> 51.x", "48.out -> 51.y", "48.out -> 51.z",
  "52.out -> 53.entity", "51.out -> 53.value",
  "54.then -> 61.in", "54.other -> 55.entity", "55.out -> 56.a", "56.out -> 60.a",
  "57.out -> 59.a", "58.out -> 59.b", "59.out -> 60.b", "60.out -> 61.cond",
  "61.true -> 62.in", "62.then -> 63.in", "63.then -> 66.in",
  "57.out -> 65.a", "64.out -> 65.b", "65.out -> 66.value",
]
"#;

const PICKUP: &str = r#"schema_version = 1
seal_graph = "pickup"
vars = []
nodes = [
  { id = 1, kind = "event", name = "overlap_begin", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 3, kind = "compare", type = "str", op = "eq", defaults = { b = "player" } },
  { id = 4, kind = "branch" },
  { id = 5, kind = "add_score", defaults = { name = "gems", amount = 1 } },
  { id = 6, kind = "play_sound", defaults = { sound = "audio/pickup.wav", volume = 0.6, pitch = 1.0 } },
  { id = 7, kind = "self" },
  { id = 8, kind = "despawn" },
]
links = [
  "1.then -> 4.in", "1.other -> 2.entity", "2.out -> 3.a", "3.out -> 4.cond",
  "4.true -> 5.in", "5.then -> 6.in", "6.then -> 8.in", "7.out -> 8.entity",
]
"#;

const ENEMY: &str = r#"schema_version = 1
seal_graph = "enemy"
vars = [
  { name = "facing", type = "str", default = "down" },
  { name = "lx", type = "float", default = 0.0 },
  { name = "ly", type = "float", default = 0.0 },
  { name = "primed", type = "bool", default = false },
]
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "self" },
  { id = 3, kind = "world_get", component = "transform", field = "translation", type = "vec3" },
  { id = 4, kind = "vec3_x" },
  { id = 5, kind = "vec3_y" },
  { id = 6, kind = "set", name = "lx" },
  { id = 7, kind = "set", name = "ly" },
  { id = 8, kind = "const", type = "bool", value = true },
  { id = 9, kind = "set", name = "primed" },
  { id = 10, kind = "self" },
  { id = 11, kind = "play_clip", defaults = { clip = "idle_down" } },
  { id = 12, kind = "event", name = "tick" },
  { id = 13, kind = "get", name = "primed" },
  { id = 14, kind = "branch" },
  { id = 15, kind = "self" },
  { id = 16, kind = "world_get", component = "transform", field = "translation", type = "vec3" },
  { id = 17, kind = "vec3_x" },
  { id = 18, kind = "vec3_y" },
  { id = 19, kind = "get", name = "lx" },
  { id = 20, kind = "get", name = "ly" },
  { id = 21, kind = "sub", type = "float" },
  { id = 22, kind = "sub", type = "float" },
  { id = 23, kind = "abs", type = "float" },
  { id = 24, kind = "abs", type = "float" },
  { id = 25, kind = "compare", type = "float", op = "gt", defaults = { b = 0.002 } },
  { id = 26, kind = "compare", type = "float", op = "gt", defaults = { b = 0.002 } },
  { id = 27, kind = "or" },
  { id = 28, kind = "compare", type = "float", op = "gt" },
  { id = 29, kind = "compare", type = "float", op = "gt", defaults = { b = 0.0 } },
  { id = 30, kind = "compare", type = "float", op = "gt", defaults = { b = 0.0 } },
  { id = 31, kind = "const", type = "str", value = "right" },
  { id = 32, kind = "const", type = "str", value = "left" },
  { id = 33, kind = "const", type = "str", value = "up" },
  { id = 34, kind = "const", type = "str", value = "down" },
  { id = 35, kind = "select", type = "str" },
  { id = 36, kind = "select", type = "str" },
  { id = 37, kind = "select", type = "str" },
  { id = 38, kind = "get", name = "facing" },
  { id = 39, kind = "select", type = "str" },
  { id = 40, kind = "set", name = "facing" },
  { id = 41, kind = "const", type = "str", value = "walk_" },
  { id = 42, kind = "const", type = "str", value = "idle_" },
  { id = 43, kind = "select", type = "str" },
  { id = 44, kind = "concat" },
  { id = 45, kind = "self" },
  { id = 46, kind = "play_clip" },
  { id = 47, kind = "set", name = "lx" },
  { id = 48, kind = "set", name = "ly" },
]
links = [
  "1.then -> 6.in", "6.then -> 7.in", "7.then -> 9.in", "9.then -> 11.in",
  "2.out -> 3.entity", "3.out -> 4.v", "3.out -> 5.v", "4.out -> 6.value", "5.out -> 7.value",
  "8.out -> 9.value", "10.out -> 11.entity",
  "12.then -> 14.in", "13.out -> 14.cond", "14.true -> 40.in", "40.then -> 46.in", "46.then -> 47.in", "47.then -> 48.in",
  "15.out -> 16.entity", "16.out -> 17.v", "16.out -> 18.v",
  "17.out -> 21.a", "19.out -> 21.b", "18.out -> 22.a", "20.out -> 22.b",
  "21.out -> 23.a", "22.out -> 24.a", "23.out -> 25.a", "24.out -> 26.a",
  "25.out -> 27.a", "26.out -> 27.b", "23.out -> 28.a", "24.out -> 28.b",
  "21.out -> 29.a", "22.out -> 30.a",
  "29.out -> 35.cond", "31.out -> 35.a", "32.out -> 35.b",
  "30.out -> 36.cond", "33.out -> 36.a", "34.out -> 36.b",
  "28.out -> 37.cond", "35.out -> 37.a", "36.out -> 37.b",
  "27.out -> 39.cond", "37.out -> 39.a", "38.out -> 39.b", "39.out -> 40.value",
  "27.out -> 43.cond", "41.out -> 43.a", "42.out -> 43.b",
  "43.out -> 44.a", "39.out -> 44.b", "44.out -> 46.clip", "45.out -> 46.entity",
  "17.out -> 47.value", "18.out -> 48.value",
]
"#;

const NPC: &str = r#"schema_version = 1
seal_graph = "npc"
vars = [
  { name = "near", type = "bool", default = false },
  { name = "talk", type = "bool", default = false },
  { name = "held", type = "bool", default = false },
]
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "self" },
  { id = 3, kind = "play_clip", defaults = { clip = "idle" } },
  { id = 4, kind = "event", name = "overlap_begin", params = [{ name = "other", type = "entity" }] },
  { id = 5, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 6, kind = "compare", type = "str", op = "eq", defaults = { b = "player" } },
  { id = 7, kind = "branch" },
  { id = 8, kind = "const", type = "bool", value = true },
  { id = 9, kind = "set", name = "near" },
  { id = 10, kind = "world_get", component = "transform", field = "translation", type = "vec3" },
  { id = 11, kind = "vec3_x" },
  { id = 12, kind = "self" },
  { id = 13, kind = "world_get", component = "transform", field = "translation", type = "vec3" },
  { id = 14, kind = "vec3_x" },
  { id = 15, kind = "compare", type = "float", op = "lt" },
  { id = 16, kind = "const", type = "str", value = "face_left" },
  { id = 17, kind = "const", type = "str", value = "face_right" },
  { id = 18, kind = "select", type = "str" },
  { id = 19, kind = "self" },
  { id = 20, kind = "play_clip" },
  { id = 21, kind = "event", name = "overlap_end", params = [{ name = "other", type = "entity" }] },
  { id = 22, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 23, kind = "compare", type = "str", op = "eq", defaults = { b = "player" } },
  { id = 24, kind = "branch" },
  { id = 25, kind = "const", type = "bool", value = false },
  { id = 26, kind = "set", name = "near" },
  { id = 27, kind = "set", name = "talk" },
  { id = 28, kind = "ui_show", defaults = { widget = "Dialog", visible = false } },
  { id = 29, kind = "self" },
  { id = 30, kind = "play_clip", defaults = { clip = "idle" } },
  { id = 31, kind = "event", name = "tick" },
  { id = 32, kind = "action_pressed", defaults = { name = "interact" } },
  { id = 33, kind = "get", name = "held" },
  { id = 34, kind = "not" },
  { id = 35, kind = "and" },
  { id = 36, kind = "set", name = "held" },
  { id = 37, kind = "get", name = "near" },
  { id = 38, kind = "and" },
  { id = 39, kind = "branch" },
  { id = 40, kind = "get", name = "talk" },
  { id = 41, kind = "not" },
  { id = 42, kind = "set", name = "talk" },
  { id = 43, kind = "ui_show", defaults = { widget = "Dialog", visible = true } },
  { id = 44, kind = "ui_text", defaults = { element = "Talk", text = "The cave to the east is dangerous. Press interact again to close." } },
  { id = 45, kind = "sequence", count = 2 },
  { id = 46, kind = "branch" },
  { id = 47, kind = "get", name = "talk" },
  { id = 48, kind = "ui_show", defaults = { widget = "Dialog", visible = false } },
]
links = [
  "1.then -> 3.in", "2.out -> 3.entity",
  "4.then -> 7.in", "4.other -> 5.entity", "5.out -> 6.a", "6.out -> 7.cond",
  "7.true -> 9.in", "9.then -> 20.in", "8.out -> 9.value",
  "4.other -> 10.entity", "10.out -> 11.v", "12.out -> 13.entity", "13.out -> 14.v",
  "11.out -> 15.a", "14.out -> 15.b", "15.out -> 18.cond", "16.out -> 18.a", "17.out -> 18.b",
  "18.out -> 20.clip", "19.out -> 20.entity",
  "21.then -> 24.in", "21.other -> 22.entity", "22.out -> 23.a", "23.out -> 24.cond",
  "24.true -> 26.in", "26.then -> 27.in", "27.then -> 28.in", "28.then -> 30.in",
  "25.out -> 26.value", "25.out -> 27.value", "29.out -> 30.entity",
  "31.then -> 45.in", "45.then0 -> 39.in", "45.then1 -> 36.in",
  "39.true -> 42.in", "42.then -> 46.in", "47.out -> 46.cond",
  "46.true -> 43.in", "43.then -> 44.in", "46.false -> 48.in",
  "32.out -> 36.value", "32.out -> 35.a", "33.out -> 34.a", "34.out -> 35.b",
  "35.out -> 38.a", "37.out -> 38.b", "38.out -> 39.cond",
  "40.out -> 41.a", "41.out -> 42.value",
]
"#;

const DOOR_VILLAGE: &str = r#"schema_version = 1
seal_graph = "door_village"
vars = []
nodes = [
  { id = 1, kind = "event", name = "overlap_begin", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 3, kind = "compare", type = "str", op = "eq", defaults = { b = "player" } },
  { id = 4, kind = "branch" },
  { id = 5, kind = "open_scene", defaults = { path = "scenes/cave.seal" } },
]
links = ["1.then -> 4.in", "1.other -> 2.entity", "2.out -> 3.a", "3.out -> 4.cond", "4.true -> 5.in"]
"#;

const DOOR_CAVE: &str = r#"schema_version = 1
seal_graph = "door_cave"
vars = []
nodes = [
  { id = 1, kind = "event", name = "overlap_begin", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 3, kind = "compare", type = "str", op = "eq", defaults = { b = "player" } },
  { id = 4, kind = "branch" },
  { id = 5, kind = "open_scene", defaults = { path = "scene.seal" } },
]
links = ["1.then -> 4.in", "1.other -> 2.entity", "2.out -> 3.a", "3.out -> 4.cond", "4.true -> 5.in"]
"#;

const SAVE: &str = r#"schema_version = 1
seal_graph = "save"
vars = []
nodes = [
  { id = 1, kind = "event", name = "overlap_begin", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 3, kind = "compare", type = "str", op = "eq", defaults = { b = "player" } },
  { id = 4, kind = "branch" },
  { id = 5, kind = "save_game", defaults = { slot = "shrine" } },
  { id = 6, kind = "play_sound", defaults = { sound = "audio/save.wav", volume = 0.5, pitch = 1.0 } },
]
links = ["1.then -> 4.in", "1.other -> 2.entity", "2.out -> 3.a", "3.out -> 4.cond", "4.true -> 5.in", "5.then -> 6.in"]
"#;

const GAME: &str = r#"schema_version = 1
seal_graph = "game"
vars = [
  { name = "paused", type = "bool", default = false },
  { name = "pause_was", type = "bool", default = false },
]
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "self" },
  { id = 3, kind = "keep_across_scenes", defaults = { keep = true } },
  { id = 4, kind = "add_score", defaults = { name = "hearts", amount = 3 } },
  { id = 5, kind = "add_score", defaults = { name = "gems", amount = 0 } },
  { id = 6, kind = "ui_show", defaults = { widget = "Hud", visible = true } },
  { id = 7, kind = "ui_text", defaults = { element = "Gems", text = "Gems: 0" } },
  { id = 9, kind = "sequence", count = 3 },
  { id = 10, kind = "event", name = "tick" },
  { id = 11, kind = "action_pressed", defaults = { name = "pause" } },
  { id = 12, kind = "get", name = "pause_was" },
  { id = 13, kind = "not" },
  { id = 14, kind = "and" },
  { id = 15, kind = "set", name = "pause_was" },
  { id = 16, kind = "branch" },
  { id = 17, kind = "get", name = "paused" },
  { id = 18, kind = "not" },
  { id = 19, kind = "set", name = "paused" },
  { id = 20, kind = "ui_show", defaults = { widget = "Pause", visible = true } },
  { id = 21, kind = "const", type = "float", value = 0.0 },
  { id = 22, kind = "const", type = "float", value = 1.0 },
  { id = 24, kind = "self" },
  { id = 62, kind = "branch" },
  { id = 63, kind = "get", name = "paused" },
  { id = 64, kind = "ui_show", defaults = { widget = "Pause", visible = false } },
  { id = 65, kind = "world_set", component = "play", field = "time_scale", type = "float" },
  { id = 25, kind = "world_set", component = "play", field = "time_scale", type = "float" },
  { id = 30, kind = "score", defaults = { name = "hearts" } },
  { id = 31, kind = "compare", type = "int", op = "ge", defaults = { b = 1 } },
  { id = 32, kind = "compare", type = "int", op = "ge", defaults = { b = 2 } },
  { id = 33, kind = "compare", type = "int", op = "ge", defaults = { b = 3 } },
  { id = 34, kind = "const", type = "str", value = "sprites/heart.png" },
  { id = 35, kind = "const", type = "str", value = "sprites/heart_empty.png" },
  { id = 36, kind = "select", type = "str" },
  { id = 37, kind = "select", type = "str" },
  { id = 38, kind = "select", type = "str" },
  { id = 39, kind = "ui_text", defaults = { element = "H1" } },
  { id = 40, kind = "ui_text", defaults = { element = "H2" } },
  { id = 41, kind = "ui_text", defaults = { element = "H3" } },
  { id = 42, kind = "score", defaults = { name = "gems" } },
  { id = 43, kind = "num_text", type = "int" },
  { id = 44, kind = "const", type = "str", value = "Gems: " },
  { id = 45, kind = "concat" },
  { id = 46, kind = "ui_text", defaults = { element = "Gems" } },
  { id = 47, kind = "compare", type = "int", op = "le", defaults = { b = 0 } },
  { id = 48, kind = "game_state" },
  { id = 49, kind = "const", type = "str", value = "dead" },
  { id = 50, kind = "compare", type = "str", op = "ne" },
  { id = 51, kind = "and" },
  { id = 52, kind = "branch" },
  { id = 53, kind = "set_game_state", defaults = { state = "dead" } },
  { id = 54, kind = "has_save", defaults = { slot = "shrine" } },
  { id = 55, kind = "branch" },
  { id = 56, kind = "load_game", defaults = { slot = "shrine" } },
  { id = 57, kind = "set_game_state", defaults = { state = "restart" } },
  { id = 58, kind = "current_scene" },
  { id = 59, kind = "open_scene" },
  { id = 70, kind = "event", name = "scene_loaded" },
  { id = 71, kind = "game_state" },
  { id = 72, kind = "const", type = "str", value = "restart" },
  { id = 73, kind = "compare", type = "str", op = "eq" },
  { id = 74, kind = "branch" },
  { id = 75, kind = "score", defaults = { name = "hearts" } },
  { id = 76, kind = "const", type = "int", value = 3 },
  { id = 77, kind = "sub", type = "int" },
  { id = 78, kind = "add_score", defaults = { name = "hearts" } },
  { id = 79, kind = "score", defaults = { name = "gems" } },
  { id = 80, kind = "neg", type = "int" },
  { id = 81, kind = "add_score", defaults = { name = "gems" } },
  { id = 82, kind = "set_game_state", defaults = { state = "playing" } },
  { id = 83, kind = "const", type = "bool", value = false },
  { id = 84, kind = "set", name = "paused" },
  { id = 85, kind = "ui_show", defaults = { widget = "Pause", visible = false } },
  { id = 86, kind = "const", type = "float", value = 1.0 },
  { id = 87, kind = "self" },
  { id = 88, kind = "world_set", component = "play", field = "time_scale", type = "float" },
  { id = 90, kind = "event", name = "ui_Resume_clicked" },
  { id = 91, kind = "const", type = "bool", value = false },
  { id = 92, kind = "set", name = "paused" },
  { id = 93, kind = "ui_show", defaults = { widget = "Pause", visible = false } },
  { id = 94, kind = "const", type = "float", value = 1.0 },
  { id = 95, kind = "self" },
  { id = 96, kind = "world_set", component = "play", field = "time_scale", type = "float" },
  { id = 100, kind = "event", name = "ui_Restart_clicked" },
  { id = 101, kind = "set", name = "paused" },
  { id = 102, kind = "ui_show", defaults = { widget = "Pause", visible = false } },
  { id = 103, kind = "set_game_state", defaults = { state = "restart" } },
  { id = 104, kind = "open_scene", defaults = { path = "scene.seal" } },
  { id = 105, kind = "world_set", component = "play", field = "time_scale", type = "float" },
  { id = 110, kind = "event", name = "ui_Quit_clicked" },
  { id = 111, kind = "set_game_state", defaults = { state = "quit" } },
]
links = [
  "1.then -> 3.in", "3.then -> 4.in", "4.then -> 5.in", "5.then -> 6.in", "6.then -> 7.in",
  "2.out -> 3.entity",
  "10.then -> 9.in", "9.then0 -> 39.in", "39.then -> 40.in", "40.then -> 41.in", "41.then -> 46.in", "46.then -> 16.in",
  "9.then1 -> 15.in", "9.then2 -> 52.in",
  "30.out -> 31.a", "30.out -> 32.a", "30.out -> 33.a", "30.out -> 47.a",
  "31.out -> 36.cond", "32.out -> 37.cond", "33.out -> 38.cond",
  "34.out -> 36.a", "34.out -> 37.a", "34.out -> 38.a",
  "35.out -> 36.b", "35.out -> 37.b", "35.out -> 38.b",
  "36.out -> 39.text", "37.out -> 40.text", "38.out -> 41.text",
  "42.out -> 43.a", "43.out -> 45.b", "44.out -> 45.a", "45.out -> 46.text",
  "11.out -> 14.a", "11.out -> 15.value", "12.out -> 13.a", "13.out -> 14.b", "14.out -> 16.cond",
  "16.true -> 19.in", "19.then -> 62.in", "63.out -> 62.cond",
  "62.true -> 20.in", "20.then -> 25.in", "21.out -> 25.value", "24.out -> 25.entity",
  "62.false -> 64.in", "64.then -> 65.in", "22.out -> 65.value", "24.out -> 65.entity",
  "17.out -> 18.a", "18.out -> 19.value",
  "47.out -> 51.a", "48.out -> 50.a", "49.out -> 50.b", "50.out -> 51.b", "51.out -> 52.cond",
  "52.true -> 53.in", "53.then -> 55.in", "54.out -> 55.cond",
  "55.true -> 56.in", "55.false -> 57.in", "57.then -> 59.in", "58.out -> 59.path",
  "70.then -> 74.in", "71.out -> 73.a", "72.out -> 73.b", "73.out -> 74.cond",
  "74.true -> 78.in", "78.then -> 81.in", "81.then -> 82.in", "82.then -> 84.in", "84.then -> 85.in", "85.then -> 88.in",
  "76.out -> 77.a", "75.out -> 77.b", "77.out -> 78.amount", "79.out -> 80.a", "80.out -> 81.amount",
  "83.out -> 84.value", "86.out -> 88.value", "87.out -> 88.entity",
  "90.then -> 92.in", "92.then -> 93.in", "93.then -> 96.in",
  "91.out -> 92.value", "91.out -> 101.value", "94.out -> 96.value", "94.out -> 105.value",
  "95.out -> 96.entity", "95.out -> 105.entity",
  "100.then -> 101.in", "101.then -> 102.in", "102.then -> 103.in", "103.then -> 104.in", "104.then -> 105.in",
  "110.then -> 111.in",
]
"#;

#[cfg(test)]
mod tests {
    use super::GRAPHS;

    #[test]
    fn every_template_graph_builds() {
        for (name, body) in GRAPHS {
            let graph =
                seal_graph::Graph::from_text(body).unwrap_or_else(|e| panic!("{name}: {e}"));
            if let Err(errors) = seal_graph::Program::build(&graph) {
                panic!("{name}: {errors:?}");
            }
        }
    }
}
