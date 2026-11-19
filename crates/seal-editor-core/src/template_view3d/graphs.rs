//! Logic graphs for 3D view templates (kept in one place for tests).

pub const FPS_LOOK: &str = r#"schema_version = 1
seal_graph = "fps_look"
vars = [
  { name = "yaw", type = "float", default = 0.0 },
  { name = "pitch", type = "float", default = -8.0 },
]
nodes = [
  { id = 1, kind = "event", name = "tick", params = [{ name = "dt", type = "float" }] },
  { id = 2, kind = "mouse_delta" },
  { id = 3, kind = "vec2_x", type = "vec2" },
  { id = 4, kind = "vec2_y", type = "vec2" },
  { id = 5, kind = "mul", type = "float", defaults = { b = 0.15 } },
  { id = 6, kind = "mul", type = "float", defaults = { b = -0.15 } },
  { id = 7, kind = "get", name = "yaw" },
  { id = 8, kind = "add", type = "float" },
  { id = 9, kind = "set", name = "yaw" },
  { id = 10, kind = "get", name = "pitch" },
  { id = 11, kind = "add", type = "float" },
  { id = 12, kind = "min", type = "float", defaults = { b = 75.0 } },
  { id = 13, kind = "max", type = "float", defaults = { b = -75.0 } },
  { id = 14, kind = "set", name = "pitch" },
  { id = 20, kind = "self" },
  { id = 21, kind = "get", name = "pitch" },
  { id = 22, kind = "get", name = "yaw" },
  { id = 23, kind = "const", type = "float", value = 0.0 },
  { id = 24, kind = "vec3" },
  { id = 25, kind = "world_set", component = "transform", field = "rotation", type = "vec3" },
  { id = 30, kind = "branch" },
  { id = 31, kind = "action_pressed", defaults = { name = "look" } },
]
links = [
  "1.then -> 30.in",
  "31.out -> 30.cond",
  "30.true -> 9.in",
  "2.out -> 3.v", "2.out -> 4.v",
  "3.out -> 5.a", "4.out -> 6.a",
  "5.out -> 8.a", "7.out -> 8.b", "8.out -> 9.value",
  "9.then -> 14.in",
  "6.out -> 11.a", "10.out -> 11.b", "11.out -> 12.a", "12.out -> 13.a", "13.out -> 14.value",
  "14.then -> 25.in",
  "20.out -> 25.entity",
  "21.out -> 24.x", "22.out -> 24.y", "23.out -> 24.z",
  "24.out -> 25.value",
]
"#;

pub const FPS_CAMERA: &str = r#"schema_version = 1
seal_graph = "fps_camera"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "find_first", prefab = "player" },
  { id = 3, kind = "spectate", on = true },
  { id = 10, kind = "event", name = "tick", params = [{ name = "dt", type = "float" }] },
  { id = 11, kind = "find_first", prefab = "player" },
  { id = 12, kind = "world_get", component = "transform", field = "translation", type = "vec3" },
  { id = 13, kind = "const", type = "float", value = 0.0 },
  { id = 14, kind = "const", type = "float", value = 0.6 },
  { id = 15, kind = "vec3" },
  { id = 16, kind = "add", type = "vec3" },
  { id = 17, kind = "self" },
  { id = 18, kind = "world_set", component = "transform", field = "translation", type = "vec3" },
  { id = 19, kind = "world_get", component = "transform", field = "rotation", type = "vec3" },
  { id = 20, kind = "world_set", component = "transform", field = "rotation", type = "vec3" },
]
links = [
  "1.then -> 3.in",
  "2.out -> 3.entity",
  "10.then -> 18.in",
  "11.out -> 12.entity",
  "13.out -> 15.x", "14.out -> 15.y", "13.out -> 15.z",
  "12.out -> 16.a", "15.out -> 16.b",
  "17.out -> 18.entity", "16.out -> 18.value",
  "18.then -> 20.in",
  "11.out -> 19.entity",
  "17.out -> 20.entity", "19.out -> 20.value",
]
"#;

pub const TPS_CAMERA: &str = r#"schema_version = 1
seal_graph = "tps_camera"
vars = [
  { name = "yaw", type = "float", default = 35.0 },
  { name = "back", type = "float", default = 5.0 },
]
nodes = [
  { id = 1, kind = "event", name = "tick", params = [{ name = "dt", type = "float" }] },
  { id = 2, kind = "mouse_delta" },
  { id = 3, kind = "vec2_x", type = "vec2" },
  { id = 4, kind = "mul", type = "float", defaults = { b = -0.12 } },
  { id = 5, kind = "get", name = "yaw" },
  { id = 6, kind = "add", type = "float" },
  { id = 7, kind = "set", name = "yaw" },
  { id = 20, kind = "find_first", prefab = "player" },
  { id = 21, kind = "get", name = "yaw" },
  { id = 22, kind = "const", type = "float", value = 0.0 },
  { id = 23, kind = "vec3" },
  { id = 24, kind = "world_set", component = "transform", field = "rotation", type = "vec3" },
  { id = 30, kind = "self" },
  { id = 31, kind = "find_first", prefab = "player" },
  { id = 32, kind = "world_get", component = "transform", field = "translation", type = "vec3" },
  { id = 33, kind = "get", name = "yaw" },
  { id = 34, kind = "sin", type = "float" },
  { id = 35, kind = "cos", type = "float" },
  { id = 36, kind = "mul", type = "float", defaults = { b = 0.017453292 } },
  { id = 39, kind = "neg", type = "float" },
  { id = 50, kind = "const", type = "float", value = 0.0 },
  { id = 51, kind = "vec3" },
  { id = 52, kind = "scale", type = "vec3", defaults = { s = 0.6 } },
  { id = 53, kind = "add", type = "vec3" },
  { id = 54, kind = "get", name = "back" },
  { id = 55, kind = "sub", type = "float", defaults = { b = 0.6 } },
  { id = 56, kind = "ray_distance" },
  { id = 57, kind = "add", type = "float", defaults = { b = 0.4 } },
  { id = 58, kind = "max", type = "float", defaults = { b = 0.6 } },
  { id = 60, kind = "scale", type = "vec3" },
  { id = 41, kind = "add", type = "vec3" },
  { id = 42, kind = "world_set", component = "transform", field = "translation", type = "vec3" },
  { id = 43, kind = "spectate", on = true },
  { id = 44, kind = "find_first", prefab = "player" },
  { id = 61, kind = "const", type = "float", value = 1.2 },
  { id = 62, kind = "vec3" },
  { id = 63, kind = "add", type = "vec3" },
  { id = 70, kind = "get", name = "yaw" },
  { id = 71, kind = "mul", type = "float", defaults = { b = -1.0 } },
  { id = 72, kind = "const", type = "float", value = -12.0 },
  { id = 73, kind = "vec3" },
  { id = 74, kind = "world_set", component = "transform", field = "rotation", type = "vec3" },
  { id = 80, kind = "branch" },
  { id = 81, kind = "action_pressed", defaults = { name = "look" } },
]
links = [
  "1.then -> 80.in",
  "81.out -> 80.cond",
  "80.true -> 7.in",
  "80.false -> 24.in",
  "2.out -> 3.v",
  "3.out -> 4.a",
  "4.out -> 6.a",
  "5.out -> 6.b",
  "6.out -> 7.value",
  "7.then -> 24.in",
  "20.out -> 24.entity",
  "21.out -> 23.y",
  "22.out -> 23.x",
  "22.out -> 23.z",
  "23.out -> 24.value",
  "24.then -> 42.in",
  "30.out -> 42.entity",
  "31.out -> 32.entity",
  "33.out -> 36.a",
  "36.out -> 34.a",
  "36.out -> 35.a",
  "35.out -> 39.a",
  "34.out -> 51.x",
  "50.out -> 51.y",
  "39.out -> 51.z",
  "51.out -> 52.v",
  "32.out -> 53.a",
  "52.out -> 53.b",
  "54.out -> 55.a",
  "53.out -> 56.from",
  "51.out -> 56.direction",
  "55.out -> 56.max_distance",
  "56.out -> 57.a",
  "57.out -> 58.a",
  "51.out -> 60.v",
  "58.out -> 60.s",
  "32.out -> 41.a",
  "60.out -> 41.b",
  "50.out -> 62.x", "61.out -> 62.y", "50.out -> 62.z",
  "41.out -> 63.a", "62.out -> 63.b",
  "63.out -> 42.value",
  "42.then -> 74.in",
  "70.out -> 71.a",
  "72.out -> 73.x", "71.out -> 73.y", "50.out -> 73.z",
  "30.out -> 74.entity", "73.out -> 74.value",
  "74.then -> 43.in",
  "44.out -> 43.entity",
]
"#;

pub fn parse_all() -> Result<(), String> {
    for (name, text) in [
        ("fps_look", FPS_LOOK),
        ("fps_camera", FPS_CAMERA),
        ("tps_camera", TPS_CAMERA),
        ("goal", super::GOAL_GRAPH),
        ("game", super::GAME_GRAPH),
    ] {
        let graph = seal_graph::Graph::from_text(text).map_err(|e| format!("{name}: {e}"))?;
        seal_graph::Program::build(&graph).map_err(|e| format!("{name}: {e:?}"))?;
    }
    Ok(())
}
