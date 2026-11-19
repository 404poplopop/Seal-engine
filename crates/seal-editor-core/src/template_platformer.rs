//! Platformer starter template: scenes, art, scripts, UI and audio written on create.

use std::path::Path;

use seal_world::World;

use crate::error::CoreError;

fn io_core(err: std::io::Error) -> CoreError {
    CoreError::new(err.to_string())
}

pub fn preview_rgba() -> (u32, u32, &'static [u8]) {
    static PREVIEW: std::sync::OnceLock<(u32, u32, Vec<u8>)> = std::sync::OnceLock::new();
    let (w, h, buf) = PREVIEW.get_or_init(|| {
        let (w, h) = (64u32, 48u32);
        let mut px = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if y > h * 3 / 4 {
                    px[i..i + 4].copy_from_slice(&[70, 160, 60, 255]);
                } else if y > h / 2 {
                    px[i..i + 4].copy_from_slice(&[140, 100, 64, 255]);
                } else {
                    px[i..i + 4].copy_from_slice(&[120, 190, 255, 255]);
                }
            }
        }
        (w, h, px)
    });
    (*w, *h, buf.as_slice())
}

pub fn write(root: &Path, name: &str) -> Result<(), CoreError> {
    write_art(root).map_err(io_core)?;
    write_audio(root).map_err(io_core)?;
    write_input(root)?;
    write_logic(root).map_err(io_core)?;
    write_ui(root).map_err(io_core)?;
    std::fs::write(root.join("scene.seal"), build_level(true)?.to_text())?;
    std::fs::create_dir_all(root.join("scenes"))?;
    std::fs::write(root.join("scenes/level2.seal"), build_level(false)?.to_text())?;
    let _ = name;
    Ok(())
}

fn write_input(root: &Path) -> Result<(), CoreError> {
    // Same bindings the play loop uses when a project has no input file.
    let text = "\
# Side-view controls: keyboard and gamepad.
action move_left = key.a, key.arrowleft, pad.dpad_left
action move_right = key.d, key.arrowright, pad.dpad_right
action jump = key.space, key.w, key.arrowup, pad.south
action pause = key.escape, pad.start
axis move_x = pad.left_x deadzone 0.25
";
    std::fs::write(root.join("input.sealinput"), text)?;
    Ok(())
}

fn write_audio(root: &Path) -> std::io::Result<()> {
    use seal_audio::Sound;
    std::fs::create_dir_all(root.join("audio"))?;
    for (file, hz, len, amp) in [
        ("coin.wav", 1318.5_f32, 0.07, 0.35),
        ("jump.wav", 523.25, 0.05, 0.2),
        ("win.wav", 659.25, 0.15, 0.3),
    ] {
        let tone = Sound::tone(22050, hz, len, amp);
        std::fs::write(root.join("audio").join(file), tone.to_wav16())?;
    }
    Ok(())
}

fn write_ui(root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root.join("ui"))?;
    let hud = r#"seal ui 1
# Coin counter during play
panel Hud
  anchor 0 0 0 0
  offset 16 16 220 56
  label CoinLabel
    text "Coins: 0"
"#;
    let pause = r#"seal ui 1
# Pause overlay. Button order is the keyboard and gamepad focus order.
panel Pause
  anchor 0 0 1 1
  vbox Menu
    anchor 0.5 0.5 0.5 0.5
    offset -120 -110 120 110
    spacing 12
    label Title
      text "Paused"
      align center
    button Resume
    button Restart
    button Quit
"#;
    std::fs::write(root.join("ui/Hud.sealui"), hud)?;
    std::fs::write(root.join("ui/Pause.sealui"), pause)?;
    let win = r#"seal ui 1
# End screen at the level 2 flag.
panel Win
  anchor 0 0 1 1
  vbox Menu
    anchor 0.5 0.5 0.5 0.5
    offset -120 -90 120 90
    spacing 12
    label Title
      text "You Win!"
      align center
    button PlayAgain
      text "Play Again"
    button WinQuit
      text "Quit"
"#;
    std::fs::write(root.join("ui/Win.sealui"), win)?;
    Ok(())
}

fn write_logic(root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root.join("logic"))?;
    for (name, text) in [
        ("manager.graph", MANAGER),
        ("player.graph", PLAYER),
        ("coin.graph", COIN),
        ("goal.graph", GOAL),
        ("goal2.graph", GOAL2),
        ("hazard.graph", HAZARD),
        ("hazard2.graph", HAZARD2),
    ] {
        std::fs::write(root.join("logic").join(name), text)?;
    }
    Ok(())
}

/// One coin count, named `coins`. The HUD, the pickups and the goal all use it.
/// `bank` is that count at the moment the current level opened, so a hazard
/// can put it back. The manager is the only object kept between levels.
const MANAGER: &str = r#"schema_version = 1
seal_graph = "manager"
vars = [{ name = "pause_down", type = "bool" }]
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "sequence", count = 5 },
  { id = 3, kind = "ui_show", defaults = { widget = "Hud", visible = true } },
  { id = 4, kind = "self" },
  { id = 5, kind = "keep_across_scenes", defaults = { keep = true } },
  { id = 6, kind = "set_saved", defaults = { key = "scene", value = "scene.seal" } },
  { id = 7, kind = "set_saved", defaults = { key = "bank", value = "0" } },
  { id = 8, kind = "set_game_state", defaults = { state = "playing" } },
  { id = 10, kind = "event", name = "tick", params = [{ name = "dt", type = "float" }] },
  { id = 11, kind = "sequence", count = 3 },
  { id = 12, kind = "score", defaults = { name = "coins" } },
  { id = 13, kind = "to_str", type = "int" },
  { id = 14, kind = "concat", defaults = { a = "Coins: ", b = "0" } },
  { id = 15, kind = "ui_text", defaults = { element = "CoinLabel", text = "Coins: 0" } },
  { id = 16, kind = "action_pressed", defaults = { name = "pause" } },
  { id = 17, kind = "get", name = "pause_down" },
  { id = 18, kind = "not" },
  { id = 19, kind = "and" },
  { id = 20, kind = "branch" },
  { id = 21, kind = "sequence", count = 2 },
  { id = 22, kind = "ui_show", defaults = { widget = "Pause", visible = true } },
  { id = 23, kind = "set_game_state", defaults = { state = "paused" } },
  { id = 24, kind = "set", name = "pause_down" },
  { id = 30, kind = "event", name = "scene_loaded" },
  { id = 31, kind = "saved", defaults = { key = "scene" } },
  { id = 32, kind = "current_scene" },
  { id = 33, kind = "compare", type = "str", op = "eq" },
  { id = 34, kind = "branch" },
  { id = 35, kind = "saved", defaults = { key = "bank" } },
  { id = 36, kind = "parse_int" },
  { id = 37, kind = "score", defaults = { name = "coins" } },
  { id = 38, kind = "sub", type = "int" },
  { id = 39, kind = "add_score", defaults = { name = "coins", amount = 0 } },
  { id = 40, kind = "sequence", count = 2 },
  { id = 41, kind = "score", defaults = { name = "coins" } },
  { id = 42, kind = "to_str", type = "int" },
  { id = 43, kind = "set_saved", defaults = { key = "bank", value = "0" } },
  { id = 44, kind = "current_scene" },
  { id = 45, kind = "set_saved", defaults = { key = "scene", value = "" } },
  { id = 50, kind = "event", name = "ui_Resume_clicked" },
  { id = 51, kind = "sequence", count = 2 },
  { id = 52, kind = "set_game_state", defaults = { state = "playing" } },
  { id = 53, kind = "ui_show", defaults = { widget = "Pause", visible = false } },
  { id = 60, kind = "event", name = "ui_Restart_clicked" },
  { id = 61, kind = "sequence", count = 3 },
  { id = 62, kind = "set_game_state", defaults = { state = "playing" } },
  { id = 63, kind = "ui_show", defaults = { widget = "Pause", visible = false } },
  { id = 64, kind = "current_scene" },
  { id = 65, kind = "open_scene", defaults = { path = "scene.seal" } },
  { id = 70, kind = "event", name = "ui_Quit_clicked" },
  { id = 71, kind = "set_game_state", defaults = { state = "quit" } },
  { id = 80, kind = "event", name = "ui_PlayAgain_clicked" },
  { id = 81, kind = "sequence", count = 3 },
  { id = 82, kind = "ui_show", defaults = { widget = "Win", visible = false } },
  { id = 83, kind = "score", defaults = { name = "coins" } },
  { id = 84, kind = "neg", type = "int" },
  { id = 85, kind = "add_score", defaults = { name = "coins", amount = 0 } },
  { id = 86, kind = "event", name = "ui_WinQuit_clicked" },
  { id = 87, kind = "sequence", count = 2 },
  { id = 88, kind = "set_game_state", defaults = { state = "playing" } },
  { id = 89, kind = "open_scene", defaults = { path = "scene.seal" } },
]
links = [
  "1.then -> 2.in",
  "2.then0 -> 3.in",
  "2.then1 -> 5.in", "4.out -> 5.entity",
  "2.then2 -> 6.in",
  "2.then3 -> 7.in",
  "2.then4 -> 8.in",
  "10.then -> 11.in",
  "11.then0 -> 15.in", "12.out -> 13.a", "13.out -> 14.b", "14.out -> 15.text",
  "11.then1 -> 20.in", "16.out -> 19.a", "17.out -> 18.a", "18.out -> 19.b", "19.out -> 20.cond",
  "20.true -> 21.in", "21.then0 -> 22.in", "21.then1 -> 23.in",
  "11.then2 -> 24.in", "16.out -> 24.value",
  "30.then -> 34.in", "31.out -> 33.a", "32.out -> 33.b", "33.out -> 34.cond",
  "34.true -> 39.in", "35.out -> 36.a", "36.out -> 38.a", "37.out -> 38.b", "38.out -> 39.amount",
  "34.false -> 40.in",
  "40.then0 -> 43.in", "41.out -> 42.a", "42.out -> 43.value",
  "40.then1 -> 45.in", "44.out -> 45.value",
  "50.then -> 51.in", "51.then0 -> 52.in", "51.then1 -> 53.in",
  "60.then -> 61.in", "61.then0 -> 62.in", "61.then1 -> 63.in", "61.then2 -> 65.in", "64.out -> 65.path",
  "70.then -> 71.in",
  "80.then -> 81.in", "81.then0 -> 82.in", "81.then1 -> 85.in", "83.out -> 84.a", "84.out -> 85.amount",
  "81.then2 -> 87.in", "87.then0 -> 88.in", "87.then1 -> 89.in",
  "86.then -> 71.in",
]
"#;

const PLAYER: &str = r#"schema_version = 1
seal_graph = "player"
vars = [{ name = "jump_down", type = "bool" }]
nodes = [
  { id = 1, kind = "event", name = "tick", params = [{ name = "dt", type = "float" }] },
  { id = 2, kind = "sequence", count = 2 },
  { id = 3, kind = "action_pressed", defaults = { name = "jump" } },
  { id = 4, kind = "get", name = "jump_down" },
  { id = 5, kind = "not" },
  { id = 6, kind = "and" },
  { id = 7, kind = "branch" },
  { id = 8, kind = "play_sound", defaults = { sound = "audio/jump.wav", volume = 0.5, pitch = 1.0 } },
  { id = 9, kind = "set", name = "jump_down" },
]
links = [
  "1.then -> 2.in",
  "2.then0 -> 7.in", "3.out -> 6.a", "4.out -> 5.a", "5.out -> 6.b", "6.out -> 7.cond",
  "7.true -> 8.in",
  "2.then1 -> 9.in", "3.out -> 9.value",
]
"#;

const COIN: &str = r#"schema_version = 1
seal_graph = "coin"
vars = []
nodes = [
  { id = 1, kind = "event", name = "overlap_begin", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 3, kind = "const", type = "str", value = "player" },
  { id = 4, kind = "compare", type = "str", op = "eq" },
  { id = 5, kind = "branch" },
  { id = 6, kind = "sequence", count = 3 },
  { id = 7, kind = "add_score", defaults = { name = "coins", amount = 1 } },
  { id = 8, kind = "play_sound", defaults = { sound = "audio/coin.wav", volume = 0.6, pitch = 1.0 } },
  { id = 9, kind = "self" },
  { id = 10, kind = "despawn" },
]
links = [
  "1.then -> 5.in", "1.other -> 2.entity", "2.out -> 4.a", "3.out -> 4.b", "4.out -> 5.cond",
  "5.true -> 6.in", "6.then0 -> 7.in", "6.then1 -> 8.in", "6.then2 -> 10.in", "9.out -> 10.entity",
]
"#;

const GOAL: &str = r#"schema_version = 1
seal_graph = "goal"
vars = []
nodes = [
  { id = 1, kind = "event", name = "overlap_begin", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 3, kind = "const", type = "str", value = "player" },
  { id = 4, kind = "compare", type = "str", op = "eq" },
  { id = 5, kind = "score", defaults = { name = "coins" } },
  { id = 6, kind = "const", type = "int", value = 2 },
  { id = 7, kind = "compare", type = "int", op = "ge" },
  { id = 8, kind = "and" },
  { id = 9, kind = "branch" },
  { id = 10, kind = "sequence", count = 2 },
  { id = 11, kind = "play_sound", defaults = { sound = "audio/win.wav", volume = 0.5, pitch = 1.0 } },
  { id = 12, kind = "open_scene", defaults = { path = "scenes/level2.seal" } },
]
links = [
  "1.then -> 9.in", "1.other -> 2.entity",
  "2.out -> 4.a", "3.out -> 4.b", "4.out -> 8.a",
  "5.out -> 7.a", "6.out -> 7.b", "7.out -> 8.b", "8.out -> 9.cond",
  "9.true -> 10.in", "10.then0 -> 11.in", "10.then1 -> 12.in",
]
"#;

/// The level 2 flag ends the game: win sound, the Win screen, and a paused world.
const GOAL2: &str = r#"schema_version = 1
seal_graph = "goal2"
vars = []
nodes = [
  { id = 1, kind = "event", name = "overlap_begin", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 3, kind = "const", type = "str", value = "player" },
  { id = 4, kind = "compare", type = "str", op = "eq" },
  { id = 5, kind = "branch" },
  { id = 6, kind = "sequence", count = 3 },
  { id = 7, kind = "play_sound", defaults = { sound = "audio/win.wav", volume = 0.5, pitch = 1.0 } },
  { id = 8, kind = "ui_show", defaults = { widget = "Win", visible = true } },
  { id = 9, kind = "set_game_state", defaults = { state = "paused" } },
]
links = [
  "1.then -> 5.in", "1.other -> 2.entity", "2.out -> 4.a", "3.out -> 4.b", "4.out -> 5.cond",
  "5.true -> 6.in", "6.then0 -> 7.in", "6.then1 -> 8.in", "6.then2 -> 9.in",
]
"#;

const HAZARD: &str = r#"schema_version = 1
seal_graph = "hazard"
vars = []
nodes = [
  { id = 1, kind = "event", name = "overlap_begin", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 3, kind = "const", type = "str", value = "player" },
  { id = 4, kind = "compare", type = "str", op = "eq" },
  { id = 5, kind = "branch" },
  { id = 6, kind = "open_scene", defaults = { path = "scene.seal" } },
]
links = [
  "1.then -> 5.in", "1.other -> 2.entity", "2.out -> 4.a", "3.out -> 4.b", "4.out -> 5.cond",
  "5.true -> 6.in",
]
"#;

const HAZARD2: &str = r#"schema_version = 1
seal_graph = "hazard2"
vars = []
nodes = [
  { id = 1, kind = "event", name = "overlap_begin", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 3, kind = "const", type = "str", value = "player" },
  { id = 4, kind = "compare", type = "str", op = "eq" },
  { id = 5, kind = "branch" },
  { id = 6, kind = "open_scene", defaults = { path = "scenes/level2.seal" } },
]
links = [
  "1.then -> 5.in", "1.other -> 2.entity", "2.out -> 4.a", "3.out -> 4.b", "4.out -> 5.cond",
  "5.true -> 6.in",
]
"#;

struct Put<'a> {
    parent: &'a str,
    name: String,
    at: [f32; 3],
    scale: [f32; 2],
    sprite: &'a str,
    tag: &'a str,
    dynamic: bool,
    script: &'a str,
}

fn put(w: &mut World, p: Put<'_>) -> String {
    let id = w.add("empty");
    let e = w.entity_mut(&id).expect("just added");
    e.parent = Some(p.parent.to_string());
    e.name = p.name;
    e.translation = p.at;
    e.scale = [p.scale[0], p.scale[1], 1.0];
    e.dynamic = p.dynamic;
    e.tag = p.tag.into();
    if !p.sprite.is_empty() {
        e.sprite = format!("sprites/{}.png", p.sprite);
    }
    if !p.script.is_empty() {
        e.script = p.script.into();
    }
    id
}

fn build_level(level1: bool) -> Result<World, CoreError> {
    let mut w = World::demo();
    let doomed: Vec<String> = w
        .entities
        .iter()
        .filter(|e| e.parent.as_deref() == Some("root"))
        .map(|e| e.id.clone())
        .collect();
    for id in doomed {
        w.remove(&id);
    }
    w.entity_mut("root").expect("root").name =
        if level1 { "Level 1".into() } else { "Level 2".into() };
    let folder = |w: &mut World, name: &str| {
        let id = w.add("empty");
        let e = w.entity_mut(&id).expect("just added");
        e.name = name.into();
        e.tag = "folder".into();
        id
    };
    let ground = folder(&mut w, "Ground");
    let coins = folder(&mut w, "Coins");
    let goal_folder = folder(&mut w, "Goal");
    let player_folder = folder(&mut w, "Player");
    let decor = folder(&mut w, "Decor");

    // One row of floor. Level 1 also has a patch under the one-way ledge, off the run to the flag.
    let mut tiles: Vec<f32> = if level1 {
        (-1..17).map(|i| i as f32 + 0.5).collect()
    } else {
        // Long enough that a jump off the start still lands before the hazard.
        (0..16).map(|i| i as f32 + 0.5).collect()
    };
    if level1 {
        tiles.extend([21.5, 22.5]);
    }
    for (n, x) in tiles.into_iter().enumerate() {
        put(
            &mut w,
            Put {
                parent: &ground,
                name: format!("Grass {}", n + 1),
                at: [x, 0.5, 0.0],
                scale: [1.0, 1.0],
                sprite: "grass",
                tag: "ground",
                dynamic: false,
                script: "",
            },
        );
    }

    let hero_x = if level1 { 0.0 } else { 1.0 };
    let hero = put(
        &mut w,
        Put {
            parent: &player_folder,
            name: "Hero".into(),
            at: [hero_x, 1.45, -0.2],
            scale: [1.0, 1.0],
            sprite: "hero",
            tag: "player",
            dynamic: true,
            script: "logic/player.graph",
        },
    );
    let cam = w.add("camera");
    {
        let e = w.entity_mut(&cam).expect("camera");
        e.parent = Some(hero);
        e.name = "Follow Cam".into();
        e.translation = [0.0, 1.0, 10.0];
        e.camera2d.on = true;
        e.camera2d.follow = "Hero".into();
        e.camera2d.zoom = 3.0;
        e.camera2d.smoothing = 0.0;
        e.camera2d.limits = if level1 { [-4.0, 10.0, -2.0, 8.0] } else { [-2.0, 8.0, -2.0, 6.0] };
        e.camera2d.pixel_snap = 0.0;
    }

    let hazard_at = if level1 { 3.2 } else { 9.0 };
    let hazard_script = if level1 { "logic/hazard.graph" } else { "logic/hazard2.graph" };
    put(
        &mut w,
        Put {
            parent: &decor,
            name: "Spikes".into(),
            at: [hazard_at, 1.15, 0.0],
            scale: [1.0, 0.7],
            sprite: "spike",
            tag: "hazard",
            dynamic: false,
            script: hazard_script,
        },
    );

    let coin_xs: &[f32] = if level1 { &[8.0, 11.0] } else { &[4.5] };
    for (n, x) in coin_xs.iter().enumerate() {
        put(
            &mut w,
            Put {
                parent: &coins,
                name: format!("Coin {}", n + 1),
                at: [*x, 1.7, -0.1],
                scale: [0.5, 0.5],
                sprite: "coin",
                tag: "pickup",
                dynamic: false,
                script: "logic/coin.graph",
            },
        );
    }

    let flag_x = if level1 { 14.0 } else { 12.0 };
    put(
        &mut w,
        Put {
            parent: &goal_folder,
            name: "Flag".into(),
            at: [flag_x, 2.1, -0.1],
            scale: [0.6, 1.6],
            sprite: "flag",
            tag: "flag",
            dynamic: false,
            script: if level1 { "logic/goal.graph" } else { "logic/goal2.graph" },
        },
    );

    add_parallax(&mut w, &decor, if level1 { 20.0 } else { 12.0 });
    if level1 {
        add_one_way(&mut w, &ground);
        add_moving_platform(&mut w, &ground);
        let mgr = w.add("empty");
        let e = w.entity_mut(&mgr).expect("manager");
        e.id = "game".into();
        e.name = "Game".into();
        e.parent = Some("root".into());
        e.script = "logic/manager.graph".into();
    }
    w.gravity = seal_world::DEFAULT_GRAVITY;
    Ok(w)
}

fn add_parallax(w: &mut World, decor: &str, width: f32) {
    let hills = put(
        w,
        Put {
            parent: decor,
            name: "Hills".into(),
            at: [0.0, 2.5, -5.0],
            scale: [width, 4.0],
            sprite: "hills",
            tag: "decor",
            dynamic: false,
            script: "",
        },
    );
    if let Some(e) = w.entity_mut(&hills) {
        e.parallax = [0.3, 0.0];
        e.layer = -2;
    }
    let clouds = put(
        w,
        Put {
            parent: decor,
            name: "Clouds".into(),
            at: [1.0, 5.0, -6.0],
            scale: [width * 0.8, 2.0],
            sprite: "clouds",
            tag: "decor",
            dynamic: false,
            script: "",
        },
    );
    if let Some(e) = w.entity_mut(&clouds) {
        e.parallax = [0.15, 0.0];
        e.layer = -3;
    }
}

fn add_one_way(w: &mut World, ground: &str) {
    let id = put(
        w,
        Put {
            parent: ground,
            name: "One-Way".into(),
            at: [22.0, 3.4, 0.0],
            scale: [3.0, 0.35],
            sprite: "platform",
            tag: "ledge",
            dynamic: false,
            script: "",
        },
    );
    let e = w.entity_mut(&id).expect("one-way");
    e.phys2d.body = "fixed".into();
    e.phys2d.one_way = true;
}

fn add_moving_platform(w: &mut World, ground: &str) {
    let rail = w.add("empty");
    let e = w.entity_mut(&rail).expect("rail");
    e.parent = Some(ground.to_string());
    e.name = "Mover Rail".into();
    e.translation = [18.0, 3.2, 0.0];
    e.shape2d.kind = "line".into();
    e.shape2d.points = vec![[0.0, 0.0], [4.0, 0.0]];
    e.shape2d.width = 0.05;
    e.hidden = true;
    let plat = put(
        w,
        Put {
            parent: ground,
            name: "Moving Platform".into(),
            at: [18.0, 3.2, 0.0],
            scale: [2.5, 0.35],
            sprite: "platform",
            tag: "mover",
            dynamic: false,
            script: "",
        },
    );
    let e = w.entity_mut(&plat).expect("platform");
    e.phys2d.body = "kinematic".into();
    e.follow.path = "Mover Rail".into();
    e.follow.speed = 2.0;
    e.follow.mode = "loop".into();
}

fn write_art(root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root.join("sprites"))?;
    for (name, w, h, px) in art_images() {
        write_png(&root.join("sprites").join(format!("{name}.png")), &px, w, h)?;
    }
    Ok(())
}

fn write_png(path: &Path, rgba: &[u8], w: u32, h: u32) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
        .write_image_data(rgba)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
}

fn art_images() -> Vec<(&'static str, u32, u32, Vec<u8>)> {
    let mut out = Vec::new();
    let image = |w: u32, h: u32, f: &dyn Fn(u32, u32) -> [u8; 4]| {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&f(x, y));
            }
        }
        px
    };
    out.push((
        "hero",
        16,
        16,
        image(16, 16, &|x, y| {
            let body = (x as i32 - 8).abs() < 5 && y > 4 && y < 14;
            let head = (x as i32 - 8).pow(2) + (y as i32 - 4).pow(2) < 16;
            if head || body {
                [200, 80, 60, 255]
            } else {
                [0, 0, 0, 0]
            }
        }),
    ));
    out.push((
        "grass",
        16,
        16,
        image(16, 16, &|_, y| {
            if y < 4 {
                [70, 170, 60, 255]
            } else {
                [130, 95, 58, 255]
            }
        }),
    ));
    out.push((
        "coin",
        8,
        8,
        image(8, 8, &|x, y| {
            let d = (x as i32 * 2 - 7).pow(2) + (y as i32 * 2 - 7).pow(2);
            if d <= 30 {
                [240, 200, 40, 255]
            } else {
                [0, 0, 0, 0]
            }
        }),
    ));
    out.push((
        "flag",
        16,
        32,
        image(16, 32, &|x, y| {
            if x < 4 && y > 4 {
                [90, 90, 100, 255]
            } else if x > 3 && y < 14 {
                [230, 60, 60, 255]
            } else {
                [0, 0, 0, 0]
            }
        }),
    ));
    out.push((
        "platform",
        16,
        8,
        image(16, 8, &|_, y| {
            if y < 2 {
                [100, 180, 220, 255]
            } else {
                [70, 120, 160, 255]
            }
        }),
    ));
    out.push((
        "spike",
        16,
        12,
        image(16, 12, &|x, y| {
            let peak = (x as i32 - 8).abs() <= (11 - y as i32);
            if peak && y > 2 {
                [180, 60, 60, 255]
            } else {
                [0, 0, 0, 0]
            }
        }),
    ));
    out.push((
        "hills",
        32,
        16,
        image(32, 16, &|x, y| {
            let h = 8 + ((x * 7) % 9) as i32;
            if y as i32 > 16 - h {
                [60, 140, 80, 200]
            } else {
                [0, 0, 0, 0]
            }
        }),
    ));
    out.push((
        "clouds",
        32,
        16,
        image(32, 16, &|x, y| {
            let blob = (x as i32 - 10).pow(2) + (y as i32 - 8).pow(2) < 36
                || (x as i32 - 20).pow(2) + (y as i32 - 9).pow(2) < 28;
            if blob {
                [255, 255, 255, 220]
            } else {
                [0, 0, 0, 0]
            }
        }),
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use seal_world::{PlayInput, PlayMode};

    fn fnv(bytes: &[u8]) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("seal-tpl-platformer-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn project(name: &str) -> std::path::PathBuf {
        let dir = scratch(name);
        crate::project::create_project(&dir, name).unwrap();
        write(&dir, name).unwrap();
        dir
    }

    fn collect_paths(world: &World) -> Vec<String> {
        let mut paths = Vec::new();
        for e in &world.entities {
            for path in [&e.sprite, &e.script, &e.particles, &e.normal_map] {
                if !path.is_empty() && !path.starts_with('#') {
                    paths.push(path.clone());
                }
            }
        }
        paths
    }

    fn player_x(w: &World) -> f32 {
        w.entities.iter().find(|e| e.tag == "player").expect("player").translation[0]
    }

    fn player_y(w: &World) -> f32 {
        w.entities.iter().find(|e| e.tag == "player").expect("player").translation[1]
    }

    fn coins(w: &World) -> i64 {
        w.play_game().and_then(|g| g.scores.get("coins").copied()).unwrap_or(0)
    }

    fn drive(w: &mut World, right: bool, jump: bool, pause: bool) {
        let mut keys = Vec::new();
        if right {
            keys.push("D");
        }
        if jump {
            keys.push("Space");
        }
        if pause {
            keys.push("Escape");
        }
        w.controls.set_keys(keys);
        w.read_controls();
    }

    fn start(dir: &Path) -> World {
        let mut w =
            World::from_text(&std::fs::read_to_string(dir.join("scene.seal")).unwrap()).unwrap();
        w.project_dir = Some(dir.to_path_buf());
        w.scenes.open = "scene.seal".into();
        w.play();
        assert!(
            w.play_log.iter().all(|l| !l.contains("script on") && !l.contains("input bindings")),
            "{:?}",
            w.play_log
        );
        w
    }

    #[test]
    fn scene_roundtrip_keeps_the_player_ready_for_play() {
        let w = build_level(true).expect("level");
        let back = World::from_text(&w.to_text()).expect("scene text");
        let p = back.entities.iter().find(|e| e.tag == "player").expect("player");
        assert!(p.dynamic, "player stays dynamic");
        assert!(!p.sprite.is_empty(), "player keeps its picture");
        assert_eq!(back.gravity, seal_world::DEFAULT_GRAVITY);
        assert!(back.entities.iter().any(|e| e.id == "game"), "manager id is stable");
        assert_eq!(back.entities.iter().filter(|e| e.tag == "coin" || e.tag == "goal").count(), 0);
    }

    #[test]
    fn both_levels_have_two_parallax_layers() {
        for level1 in [true, false] {
            let w = build_level(level1).unwrap();
            let layers = w.entities.iter().filter(|e| e.parallax != [1.0, 1.0]).count();
            assert!(layers >= 2, "level1={level1} parallax layers {layers}");
        }
    }

    #[test]
    fn play_input_moves_the_player_while_project_scripts_run() {
        let dir = project("input");
        let mut w = start(&dir);
        let x0 = player_x(&w);
        for _ in 0..12 {
            w.play_input = PlayInput { right: true, ..Default::default() };
            w.step(1.0 / 60.0);
        }
        let x1 = player_x(&w);
        assert!(x1 > x0 + 0.4, "play_input with scripts: {x0} -> {x1} log {:?}", w.play_log);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn run_to_level_two(dir: &Path) -> World {
        let mut w = start(dir);
        let mut saw_clamp = false;
        for _ in 0..500 {
            drive(&mut w, true, true, false);
            w.step(1.0 / 60.0);
            if let Some(view) = w.play_camera2d() {
                let px = player_x(&w);
                let py = player_y(&w);
                assert!((view.center[0] - px).abs() < 0.05, "camera x {} vs {px}", view.center[0]);
                assert!((view.center[1] - py).abs() < 0.05, "camera y {} vs {py}", view.center[1]);
                let half = [3.0, 2.0];
                let c = seal_world::camera2d::clamp_center(view.center, half, view.limits);
                let [l, r, b, t] = view.limits.expect("limits");
                assert!(c[0] >= l + half[0] - 0.02 && c[0] <= r - half[0] + 0.02, "{c:?} {view:?}");
                assert!(c[1] >= b + half[1] - 0.02 && c[1] <= t - half[1] + 0.02, "{c:?}");
                if px > r - half[0] {
                    saw_clamp = c[0] < view.center[0] - 0.5;
                }
            }
            if w.scenes.open.replace('\\', "/").ends_with("scenes/level2.seal") {
                drive(&mut w, true, true, false);
                w.step(1.0 / 60.0);
                break;
            }
        }
        assert!(
            w.scenes.open.replace('\\', "/").ends_with("scenes/level2.seal"),
            "still on {} at x {} score {} log {:?}",
            w.scenes.open,
            player_x(&w),
            coins(&w),
            w.play_log
        );
        assert!(saw_clamp, "camera limits never held the view");
        w
    }

    #[test]
    fn scripted_run_reaches_level_two_and_is_deterministic() {
        let dir = project("run");
        let finish = |dir: &Path| {
            let w = run_to_level_two(dir);
            assert_eq!(coins(&w), 2, "one coin count");
            assert_eq!(w.game_screens.text_of("CoinLabel").as_deref(), Some("Coins: 2"));
            let status = w.play_status().expect("platformer");
            assert_eq!(status.coins, 0, "platformer must not keep a second count: {status:?}");
            assert!(!status.won);
            let sounds: Vec<_> = w.play_sounds.iter().map(|s| s.sound.as_str()).collect();
            assert!(sounds.contains(&"audio/jump.wav"), "{sounds:?}");
            assert!(sounds.iter().filter(|s| **s == "audio/coin.wav").count() >= 2, "{sounds:?}");
            assert!(sounds.contains(&"audio/win.wav"), "{sounds:?}");
            // Level 2 still has its own pickup; level 1's two are gone with that scene.
            assert_eq!(w.entities.iter().filter(|e| e.script.contains("coin.graph")).count(), 1);
            fnv(w.to_text().as_bytes())
        };
        let a = finish(&dir);
        let b = finish(&dir);
        assert_eq!(a, b, "same inputs give the same world");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn level_two_opens_once_and_the_hero_lands_in_it() {
        let dir = project("after");
        let mut w = run_to_level_two(&dir);
        let opened = |w: &World| {
            w.play_log.iter().filter(|l| l.contains("opened scenes/level2.seal")).count()
        };
        let first = opened(&w);
        for _ in 0..120 {
            drive(&mut w, false, false, false);
            w.step(1.0 / 60.0);
        }
        assert_eq!(opened(&w), first, "level 2 reopened: {:?}", w.play_log);
        let ground = w
            .entities
            .iter()
            .filter(|e| e.tag == "ground")
            .map(|e| e.translation[0])
            .fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
        let (x, y) = (player_x(&w), player_y(&w));
        assert!(
            x >= ground.0 - 1.0 && x <= ground.1 + 1.0 && y > -1.0,
            "hero at {x},{y}, ground x {ground:?}"
        );
        let heroes: Vec<_> = w.entities.iter().filter(|e| e.tag == "player").collect();
        assert_eq!(heroes.len(), 1, "one hero");
        let hero = heroes[0];
        eprintln!(
            "HERO id={} name={} t={:?} s={:?} sprite={} parent={:?}",
            hero.id, hero.name, hero.translation, hero.scale, hero.sprite, hero.parent
        );
        for e in w.entities.iter().filter(|e| e.translation[2] < -1.0) {
            eprintln!(
                "BACK id={} name={} t={:?} s={:?} sprite={}",
                e.id, e.name, e.translation, e.scale, e.sprite
            );
        }
        assert!(
            hero.translation[2].abs() < 0.5,
            "hero left the play plane: {:?}",
            hero.translation
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_level_two_flag_wins_and_play_again_starts_over() {
        let dir = project("win");
        let mut w = run_to_level_two(&dir);
        let flag = w.entities.iter().find(|e| e.tag == "flag").expect("flag").translation;
        let id = w.entities.iter().find(|e| e.tag == "player").expect("player").id.clone();
        w.entity_mut(&id).unwrap().translation = [flag[0], flag[1], 0.0];
        for _ in 0..4 {
            drive(&mut w, false, false, false);
            w.step(1.0 / 60.0);
        }
        assert!(w.game_screens.shown.iter().any(|(n, _)| n == "Win"), "{:?}", w.play_log);
        assert_eq!(w.play_game().unwrap().state, "paused");
        assert_eq!(focused(&w).as_deref(), Some("PlayAgain"));
        w.game_screens.key("Enter");
        w.step(1.0 / 60.0);
        w.step(1.0 / 60.0);
        assert!(!w.game_screens.shown.iter().any(|(n, _)| n == "Win"));
        assert_eq!(w.play_game().unwrap().state, "playing");
        assert_eq!(w.scenes.open.replace('\\', "/"), "scene.seal");
        assert_eq!(coins(&w), 0, "a new game starts with no coins");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_editor_rows_follow_a_runtime_scene_switch() {
        let dir = project("rows");
        let mut session = crate::EditorSession::new(crate::Layout::default());
        session.project = Some(crate::open_project(&dir).unwrap());
        session.open_scene_file(dir.join("scene.seal")).unwrap();
        session.dispatch("play.start", &crate::CommandArgs::default()).unwrap();
        for _ in 0..600 {
            drive(&mut session.world, true, true, false);
            session.tick(1.0 / 60.0);
            if session.world.scenes.open.replace('\\', "/").ends_with("scenes/level2.seal") {
                break;
            }
        }
        assert!(session.world.scenes.open.replace('\\', "/").ends_with("scenes/level2.seal"));
        session.tick(1.0 / 60.0);
        let world_ids: Vec<_> =
            session.world.entities.iter().map(|e| (e.id.clone(), e.name.clone())).collect();
        let rows: Vec<_> =
            session.models.nodes.iter().map(|n| (n.id.clone(), n.name.clone())).collect();
        assert_eq!(rows, world_ids, "Outliner rows match the level 2 objects");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hazard_restarts_the_level_and_resets_that_levels_coins() {
        let dir = project("hazard");
        let mut w = start(&dir);
        let mut reloaded = false;
        for _ in 0..90 {
            drive(&mut w, true, false, false);
            w.step(1.0 / 60.0);
            if w.play_log.iter().any(|l| l.contains("opened scene.seal")) {
                reloaded = true;
                break;
            }
        }
        assert!(reloaded, "x {} log {:?}", player_x(&w), w.play_log);
        assert!(player_x(&w) < 1.5, "back at the start, x {}", player_x(&w));
        assert_eq!(coins(&w), 0);
        assert_eq!(w.game_screens.text_of("CoinLabel").as_deref(), Some("Coins: 0"));
        assert_eq!(w.entities.iter().filter(|e| e.script.contains("coin.graph")).count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn level_two_hazard_restarts_level_two_and_keeps_the_banked_coins() {
        let dir = project("hazard2");
        let mut w = run_to_level_two(&dir);
        assert_eq!(coins(&w), 2);
        w.play_log.clear();
        let mut reloaded = false;
        for _ in 0..90 {
            drive(&mut w, true, false, false);
            w.step(1.0 / 60.0);
            if w.play_log.iter().any(|l| l.contains("opened scenes/level2.seal")) {
                reloaded = true;
                break;
            }
        }
        assert!(reloaded, "x {} log {:?}", player_x(&w), w.play_log);
        assert!(w.scenes.open.replace('\\', "/").ends_with("scenes/level2.seal"));
        assert!(player_x(&w) < 2.0, "restarted level 2, x {}", player_x(&w));
        assert_eq!(coins(&w), 2, "banked coins stay");
        assert_eq!(w.game_screens.text_of("CoinLabel").as_deref(), Some("Coins: 2"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_way_platform_can_be_jumped_through_and_stood_on() {
        let dir = project("oneway");
        let mut w =
            World::from_text(&std::fs::read_to_string(dir.join("scene.seal")).unwrap()).unwrap();
        let hero = w.entities.iter().position(|e| e.tag == "player").unwrap();
        w.entities[hero].translation = [22.0, 1.45, -0.2];
        w.project_dir = Some(dir.clone());
        w.scenes.open = "scene.seal".into();
        w.play();
        let mut max_y = player_y(&w);
        for _ in 0..150 {
            drive(&mut w, false, true, false);
            w.step(1.0 / 60.0);
            max_y = max_y.max(player_y(&w));
        }
        assert!(max_y > 4.0, "jumped through, max y {max_y}");
        let y = player_y(&w);
        assert!(y > 3.5, "standing on the ledge, y {y} log {:?}", w.play_log);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moving_platform_carries_the_player() {
        let dir = project("mover");
        let mut w =
            World::from_text(&std::fs::read_to_string(dir.join("scene.seal")).unwrap()).unwrap();
        let hero = w.entities.iter().position(|e| e.tag == "player").unwrap();
        // Feet on the platform top: centre 3.2, scale y 0.35, player half-height 0.45.
        w.entities[hero].translation = [18.0, 3.86, -0.2];
        w.project_dir = Some(dir.clone());
        w.scenes.open = "scene.seal".into();
        w.play();
        let x0 = player_x(&w);
        for _ in 0..90 {
            drive(&mut w, false, false, false);
            w.step(1.0 / 60.0);
        }
        let x1 = player_x(&w);
        let y = player_y(&w);
        assert!(x1 > x0 + 1.0, "carried along {x0} -> {x1}");
        assert!(y > 3.0, "stayed up, y {y}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn pause_open(w: &mut World) {
        drive(w, false, false, true);
        w.step(1.0 / 60.0);
        drive(w, false, false, false);
        w.step(1.0 / 60.0);
        assert_eq!(w.play_game().unwrap().state, "paused", "{:?}", w.play_log);
        assert!(w.game_screens.shown.iter().any(|(n, _)| n == "Pause"));
    }

    fn focused(w: &World) -> Option<String> {
        let live = &w.game_screens.shown.last()?.1;
        let i = live.focus?;
        Some(live.screen.nodes[i].name.clone())
    }

    #[test]
    fn pause_menu_keyboard_resume_restart_and_quit() {
        let dir = project("keys");
        let mut w = start(&dir);
        w.step(1.0 / 60.0);
        pause_open(&mut w);
        let held = player_x(&w);
        let t = w.play_time;
        for _ in 0..20 {
            drive(&mut w, true, true, false);
            w.step(1.0 / 60.0);
        }
        assert_eq!(player_x(&w), held);
        assert_eq!(w.play_time, t);

        assert_eq!(focused(&w).as_deref(), Some("Resume"));
        w.game_screens.key("Enter");
        w.step(1.0 / 60.0);
        w.step(1.0 / 60.0);
        assert_eq!(w.play_game().unwrap().state, "playing");
        assert!(!w.game_screens.shown.iter().any(|(n, _)| n == "Pause"));
        let x0 = player_x(&w);
        for _ in 0..12 {
            drive(&mut w, true, false, false);
            w.step(1.0 / 60.0);
        }
        assert!(player_x(&w) > x0 + 0.3, "resumed");

        pause_open(&mut w);
        w.game_screens.key("Down");
        assert_eq!(focused(&w).as_deref(), Some("Restart"));
        w.game_screens.key("Enter");
        w.step(1.0 / 60.0);
        w.step(1.0 / 60.0);
        assert_eq!(w.play_game().unwrap().state, "playing");
        assert!(player_x(&w) < 1.0, "restarted, x {}", player_x(&w));
        assert_eq!(coins(&w), 0);
        assert!(!w.game_screens.shown.iter().any(|(n, _)| n == "Pause"));

        pause_open(&mut w);
        w.game_screens.key("Down");
        w.game_screens.key("Down");
        assert_eq!(focused(&w).as_deref(), Some("Quit"));
        w.game_screens.key("Enter");
        w.step(1.0 / 60.0);
        assert_eq!(w.mode, PlayMode::Editing);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn pad(w: &mut World, button: &str) {
        w.controls.raw.clear_device("pad");
        w.controls.raw.press(seal_world::seal_input::Button::parse(button).unwrap());
    }

    #[test]
    fn pause_menu_gamepad_resume_restart_and_quit() {
        let dir = project("pad");
        let mut w = start(&dir);
        w.step(1.0 / 60.0);
        pad(&mut w, "pad.start");
        w.read_controls();
        w.step(1.0 / 60.0);
        w.controls.raw.clear_device("pad");
        w.read_controls();
        w.step(1.0 / 60.0);
        assert_eq!(w.play_game().unwrap().state, "paused", "{:?}", w.play_log);

        assert_eq!(focused(&w).as_deref(), Some("Resume"));
        w.controls.raw.clear_device("pad");
        w.step(1.0 / 60.0);
        pad(&mut w, "pad.south");
        w.step(1.0 / 60.0);
        w.controls.raw.clear_device("pad");
        w.read_controls();
        w.step(1.0 / 60.0);
        assert_eq!(w.play_game().unwrap().state, "playing");

        pad(&mut w, "pad.start");
        w.read_controls();
        w.step(1.0 / 60.0);
        w.controls.raw.clear_device("pad");
        w.read_controls();
        w.step(1.0 / 60.0);
        for expect in ["Restart"] {
            pad(&mut w, "pad.dpad_down");
            w.step(1.0 / 60.0);
            if expect == "Restart" {
                assert_eq!(focused(&w).as_deref(), Some("Restart"));
            }
            w.controls.raw.clear_device("pad");
            w.step(1.0 / 60.0);
        }
        pad(&mut w, "pad.south");
        w.step(1.0 / 60.0);
        w.controls.raw.clear_device("pad");
        w.step(1.0 / 60.0);
        assert_eq!(w.play_game().unwrap().state, "playing");
        assert!(player_x(&w) < 1.0, "pad restart, x {}", player_x(&w));

        pad(&mut w, "pad.start");
        w.read_controls();
        w.step(1.0 / 60.0);
        w.controls.raw.clear_device("pad");
        w.read_controls();
        w.step(1.0 / 60.0);
        for _ in 0..2 {
            pad(&mut w, "pad.dpad_down");
            w.step(1.0 / 60.0);
            w.controls.raw.clear_device("pad");
            w.step(1.0 / 60.0);
        }
        assert_eq!(focused(&w).as_deref(), Some("Quit"));
        pad(&mut w, "pad.south");
        w.step(1.0 / 60.0);
        assert_eq!(w.mode, PlayMode::Editing);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn template_files_exist_and_the_project_opens_clean() {
        let dir = project("files");
        for rel in [
            "scene.seal",
            "scenes/level2.seal",
            "input.sealinput",
            "ui/Hud.sealui",
            "ui/Pause.sealui",
            "ui/Win.sealui",
            "logic/goal2.graph",
            "audio/coin.wav",
            "audio/jump.wav",
            "audio/win.wav",
        ] {
            assert!(dir.join(rel).is_file(), "missing {rel}");
        }
        for level in ["scene.seal", "scenes/level2.seal"] {
            let world =
                World::from_text(&std::fs::read_to_string(dir.join(level)).unwrap()).expect(level);
            for rel in collect_paths(&world) {
                assert!(dir.join(&rel).is_file(), "{level} missing {rel}");
            }
            assert!(
                world.entities.iter().filter(|e| e.parallax != [1.0, 1.0]).count() >= 2,
                "{level}"
            );
        }
        let before = crate::log_records().len();
        let mut session = crate::EditorSession::new(crate::Layout::default());
        session
            .dispatch(
                "file.open-project",
                &crate::CommandArgs::new(&[("path", dir.to_str().unwrap())]),
            )
            .unwrap();
        session.open_scene_file(dir.join("scenes/level2.seal")).unwrap();
        session.open_scene_file(dir.join("scene.seal")).unwrap();
        session.dispatch("play.start", &crate::CommandArgs::default()).unwrap();
        for _ in 0..3 {
            session.tick(1.0 / 60.0);
        }
        let fresh = crate::collect_log_lines(before);
        let bad: Vec<_> = fresh
            .iter()
            .filter(|l| l.level == "error" || l.level == "warn")
            .map(|l| format!("{} {}", l.level, l.message))
            .collect();
        assert!(bad.is_empty(), "{bad:?}");
        assert!(
            fresh
                .iter()
                .all(|l| !l.message.contains("script on") && !l.message.contains("input bindings")),
            "{fresh:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
