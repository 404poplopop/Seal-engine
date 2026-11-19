//! Starter template: a lights-out grid puzzle with menus, saves, and UI screens.

use std::path::Path;

use crate::error::CoreError;
use crate::project::CreateProjectOptions;
use seal_world::World;

/// Wizard preview (8×8 flat colours suggesting a lit grid).
pub const PREVIEW: (&'static [u8], u32, u32) = (&PREVIEW_PX, 8, 8);

const PREVIEW_PX: [u8; 256] = {
    let mut px = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        let x = (i / 4) % 8;
        let y = (i / 4) / 8;
        let on = ((x + y) % 3) != 1;
        let (r, g, b) = if on { (233u8, 69u8, 96u8) } else { (26u8, 26u8, 46u8) };
        px[i] = r;
        px[i + 1] = g;
        px[i + 2] = b;
        px[i + 3] = 255;
        i += 4;
    }
    px
};

/// Write the Puzzle template into an existing project folder (after `project.seal` exists).
pub fn create(folder: &Path, name: &str, _options: &CreateProjectOptions) -> Result<(), CoreError> {
    write_ui_assets(folder)?;
    std::fs::write(
        folder.join("input.sealinput"),
        include_str!("../templates/puzzle/input.sealinput"),
    )?;
    let logic = folder.join("logic");
    std::fs::create_dir_all(&logic)?;
    std::fs::write(logic.join("game.graph"), build_game_graph())?;
    write_png_assets(folder)?;
    write_minimal_wav(&folder.join("audio/click.wav"))?;
    std::fs::write(folder.join("audio/music.wav"), music_loop().to_wav16())?;
    let scene = build_scene(name);
    std::fs::write(folder.join("scene.seal"), scene.to_text())?;
    Ok(())
}

/// Paths every file the template references (for tests).
pub fn expected_files() -> &'static [&'static str] {
    &[
        "project.seal",
        "scene.seal",
        "input.sealinput",
        "logic/game.graph",
        "ui/puzzle.sealtheme",
        "ui/main_menu.sealui",
        "ui/level_select.sealui",
        "ui/level_play.sealui",
        "ui/win_panel.sealui",
        "ui/options.sealui",
        "ui/pause_menu.sealui",
        "sprites/tile_on.png",
        "sprites/tile_off.png",
        "sprites/background.png",
        "audio/click.wav",
        "audio/music.wav",
    ]
}

pub fn validate_assets(root: &Path) -> Result<(), String> {
    for rel in expected_files() {
        let path = root.join(rel);
        if !path.is_file() {
            return Err(format!("missing {rel}"));
        }
    }
    let scene = std::fs::read_to_string(root.join("scene.seal")).map_err(|e| e.to_string())?;
    World::from_text(&scene).map_err(|e| format!("scene: {e}"))?;
    for screen in [
        "ui/main_menu.sealui",
        "ui/level_select.sealui",
        "ui/level_play.sealui",
        "ui/win_panel.sealui",
        "ui/options.sealui",
        "ui/pause_menu.sealui",
    ] {
        let text = std::fs::read_to_string(root.join(screen)).map_err(|e| e.to_string())?;
        seal_world::gameui::Screen::from_text(&text).map_err(|e| format!("{screen}: {e}"))?;
    }
    let theme =
        std::fs::read_to_string(root.join("ui/puzzle.sealtheme")).map_err(|e| e.to_string())?;
    seal_world::gameui::Theme::from_text(&theme).map_err(|e| format!("theme: {e}"))?;
    Ok(())
}

fn write_ui_assets(base: &Path) -> std::io::Result<()> {
    let ui = base.join("ui");
    std::fs::create_dir_all(&ui)?;
    std::fs::write(
        ui.join("puzzle.sealtheme"),
        include_str!("../templates/puzzle/ui/puzzle.sealtheme"),
    )?;
    std::fs::write(
        ui.join("main_menu.sealui"),
        include_str!("../templates/puzzle/ui/main_menu.sealui"),
    )?;
    std::fs::write(
        ui.join("level_select.sealui"),
        include_str!("../templates/puzzle/ui/level_select.sealui"),
    )?;
    std::fs::write(
        ui.join("level_play.sealui"),
        include_str!("../templates/puzzle/ui/level_play.sealui"),
    )?;
    std::fs::write(
        ui.join("win_panel.sealui"),
        include_str!("../templates/puzzle/ui/win_panel.sealui"),
    )?;
    std::fs::write(
        ui.join("options.sealui"),
        include_str!("../templates/puzzle/ui/options.sealui"),
    )?;
    std::fs::write(
        ui.join("pause_menu.sealui"),
        include_str!("../templates/puzzle/ui/pause_menu.sealui"),
    )?;
    Ok(())
}

fn build_scene(name: &str) -> World {
    let mut world = World::from_text(&World::empty_scene_text(name)).expect("empty scene parses");
    world.gravity = 0.0;
    world.music = "audio/music.wav".into();
    if let Some(root) = world.entities.iter_mut().find(|e| e.id == "root") {
        root.name = name.into();
    }
    let cam_id = world.add("empty");
    {
        let e = world.entity_mut(&cam_id).unwrap();
        e.name = "Play Camera".into();
        e.parent = Some("root".into());
        e.translation = [0.0, 0.0, 10.0];
        e.tag = "camera".into();
        e.camera2d.on = true;
        e.camera2d.zoom = 48.0;
        e.sprite = "sprites/background.png".into();
    }
    let game_id = world.add("empty");
    {
        let e = world.entity_mut(&game_id).unwrap();
        e.name = "Game".into();
        e.parent = Some("root".into());
        e.script = "logic/game.graph".into();
    }
    world
}

fn write_png_assets(folder: &Path) -> std::io::Result<()> {
    let sprites = folder.join("sprites");
    std::fs::create_dir_all(&sprites)?;
    let on = solid_px(16, 16, [233, 69, 96, 255]);
    let off = solid_px(16, 16, [64, 80, 120, 255]);
    write_png(&sprites.join("tile_on.png"), 16, 16, &on)?;
    write_png(&sprites.join("tile_off.png"), 16, 16, &off)?;
    let mut bg = vec![0u8; 64 * 64 * 4];
    for (i, px) in bg.chunks_mut(4).enumerate() {
        let t = (i % 64) as u8;
        px.copy_from_slice(&[20 + t / 4, 24 + t / 3, 40 + t / 2, 255]);
    }
    write_png(&sprites.join("background.png"), 64, 64, &bg)?;
    Ok(())
}

fn solid_px(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    (0..w * h).flat_map(|_| rgba).collect()
}

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(rgba).map_err(|e| std::io::Error::other(e))
}

/// Four-second loop of soft notes for the scene's music (16 notes, quarter seconds each).
fn music_loop() -> seal_audio::Sound {
    let rate = 22050;
    let notes = [
        523.25_f32, 659.25, 783.99, 659.25, 587.33, 698.46, 880.0, 698.46, 523.25, 659.25, 783.99,
        1046.5, 987.77, 783.99, 659.25, 587.33,
    ];
    let mut samples = Vec::new();
    for hz in notes {
        let tone = seal_audio::Sound::tone(rate, hz, 0.25, 0.18);
        let n = tone.samples.len() as f32;
        // Fade each note in and out so the joins do not click.
        samples.extend(
            tone.samples
                .iter()
                .enumerate()
                .map(|(i, s)| s * (i as f32 / 400.0).min(1.0) * ((n - i as f32) / 2000.0).min(1.0)),
        );
    }
    seal_audio::Sound { sample_rate: rate, channels: 1, samples }
}

/// Tiny PCM WAV so `play_sound` references a real file.
fn write_minimal_wav(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let rate = 8000u32;
    let samples: Vec<i16> = (0..80).map(|i| ((i as f32 * 0.4).sin() * 8000.0) as i16).collect();
    let mut wav = Vec::new();
    let data_len = (samples.len() * 2) as u32;
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, wav)
}

/// Solvable 3×3 start boards, row-major, `1` lit. All lit is the win.
pub const LEVEL_STARTS: [&str; 3] = ["101000101", "000111000", "000010101"];

const SCREENS: [&str; 6] =
    ["main_menu", "level_select", "level_play", "win_panel", "options", "pause_menu"];

/// `None` when the generated play graph loads and checks clean.
pub fn game_graph_error() -> Option<String> {
    let text = build_game_graph();
    let graph = match seal_graph::Graph::from_text(&text) {
        Ok(graph) => graph,
        Err(err) => return Some(err.to_string()),
    };
    match seal_graph::Program::build(&graph) {
        Ok(_) => None,
        Err(errors) => {
            Some(errors.iter().map(|err| err.to_string()).collect::<Vec<_>>().join("\n"))
        }
    }
}

pub fn build_game_graph() -> String {
    let mut g = GraphOut::default();
    g.boot();
    g.toggle_cell();
    g.paint_cell();
    g.apply_press();
    g.refresh_board();
    g.check_win();
    g.on_press();
    g.begin_level();
    g.cell_buttons();
    g.undo_restart();
    g.menus();
    g.finish()
}

#[derive(Default)]
struct GraphOut {
    nodes: Vec<String>,
    links: Vec<String>,
    next: u32,
}

impl GraphOut {
    fn id(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    fn raw(&mut self, body: String) -> u32 {
        let id = self.id();
        self.nodes.push(format!("  {{ id = {id}, {body} }}"));
        id
    }

    fn flow(&mut self, from: u32, pin: &str, to: u32) {
        self.links.push(format!("\"{from}.{pin} -> {to}.in\""));
    }

    fn data(&mut self, from: u32, from_pin: &str, to: u32, to_pin: &str) {
        self.links.push(format!("\"{from}.{from_pin} -> {to}.{to_pin}\""));
    }

    fn then(&mut self, from: u32, to: u32) -> u32 {
        self.flow(from, "then", to);
        to
    }

    fn event(&mut self, name: &str) -> u32 {
        self.raw(format!(r#"kind = "event", name = "{name}""#))
    }

    fn event_index(&mut self, name: &str) -> u32 {
        self.raw(format!(
            r#"kind = "event", name = "{name}", params = [{{ name = "index", type = "int" }}]"#
        ))
    }

    fn get(&mut self, name: &str) -> u32 {
        self.raw(format!(r#"kind = "get", name = "{name}""#))
    }

    fn call(&mut self, name: &str) -> u32 {
        self.raw(format!(r#"kind = "call", name = "{name}""#))
    }

    fn ui_show(&mut self, widget: &str, visible: bool) -> u32 {
        let flag = if visible { "true" } else { "false" };
        self.raw(format!(
            r#"kind = "ui_show", defaults = {{ widget = "{widget}", visible = {flag} }}"#
        ))
    }

    /// Hide every screen except `show`, then show that one. Returns the last node.
    fn show_only(&mut self, from: u32, pin: &str, show: &str) -> u32 {
        let mut prev = from;
        let mut prev_pin = pin;
        for screen in SCREENS {
            if screen == show {
                continue;
            }
            let node = self.ui_show(screen, false);
            self.flow(prev, prev_pin, node);
            prev = node;
            prev_pin = "then";
        }
        let node = self.ui_show(show, true);
        self.flow(prev, prev_pin, node);
        node
    }

    fn list_from_bits(&mut self, bits: &str) -> u32 {
        let mut defaults = String::new();
        for (i, ch) in bits.chars().enumerate() {
            if i > 0 {
                defaults.push_str(", ");
            }
            let lit = i32::from(ch == '1');
            defaults.push_str(&format!("item{i} = {lit}"));
        }
        self.raw(format!(
            r#"kind = "list_make", type = "int", count = 9, defaults = {{ {defaults} }}"#
        ))
    }

    /// 0.55 until the options slider stores a SoundVol, then that value / 10.
    fn sound_volume(&mut self) -> u32 {
        self.saved_volume("SoundVol", 0.55)
    }

    /// `fallback` until the options slider stores `key`, then that value / 10.
    fn saved_volume(&mut self, key: &str, fallback: f32) -> u32 {
        let saved = self.raw(format!(r#"kind = "saved", defaults = {{ key = "{key}" }}"#));
        let empty =
            self.raw(r#"kind = "compare", type = "str", op = "eq", defaults = { b = "" }"#.into());
        self.data(saved, "out", empty, "a");
        let parsed = self.raw(r#"kind = "parse_float""#.into());
        self.data(saved, "out", parsed, "a");
        let scaled = self.raw(r#"kind = "div", type = "float", defaults = { b = 10.0 }"#.into());
        self.data(parsed, "out", scaled, "a");
        let pick = self
            .raw(format!(r#"kind = "select", type = "float", defaults = {{ a = {fallback:?} }}"#));
        self.data(empty, "out", pick, "cond");
        self.data(scaled, "out", pick, "b");
        pick
    }

    fn play_click(&mut self, volume: u32, pitch: &str) -> u32 {
        let node = self.raw(format!(
            r#"kind = "play_sound", defaults = {{ sound = "audio/click.wav", pitch = {pitch} }}"#
        ));
        self.data(volume, "out", node, "volume");
        node
    }

    /// Music bus volume from the MusicVol slider (0.8 before the first change).
    fn apply_music_volume(&mut self) -> u32 {
        let volume = self.saved_volume("MusicVol", 0.8);
        let node = self.raw(r#"kind = "set_music_volume""#.into());
        self.data(volume, "out", node, "volume");
        node
    }

    fn boot(&mut self) {
        let start = self.event("start");
        let music = self.apply_music_volume();
        self.then(start, music);
        self.show_only(music, "then", "main_menu");

        let tick = self.event("tick");
        let booted = self.get("booted");
        let fresh =
            self.raw(r#"kind = "compare", type = "int", op = "eq", defaults = { b = 0 }"#.into());
        self.data(booted, "out", fresh, "a");
        let gate = self.raw(r#"kind = "branch""#.into());
        self.data(fresh, "out", gate, "cond");
        self.flow(tick, "then", gate);
        let mark = self.raw(r#"kind = "set", name = "booted", defaults = { value = 1 }"#.into());
        self.flow(gate, "true", mark);
        let has = self.raw(r#"kind = "has_save", defaults = { slot = "progress" }"#.into());
        let which = self.raw(r#"kind = "branch""#.into());
        self.data(has, "out", which, "cond");
        self.then(mark, which);
        let load = self.raw(r#"kind = "load_game", defaults = { slot = "progress" }"#.into());
        self.flow(which, "true", load);
        let init =
            self.raw(r#"kind = "set_saved", defaults = { key = "unlocked", value = "1" }"#.into());
        self.flow(which, "false", init);

        let loaded = self.event("game_loaded");
        let saved = self.raw(r#"kind = "saved", defaults = { key = "unlocked" }"#.into());
        let blank =
            self.raw(r#"kind = "compare", type = "str", op = "eq", defaults = { b = "" }"#.into());
        self.data(saved, "out", blank, "a");
        let fix = self.raw(r#"kind = "branch""#.into());
        self.data(blank, "out", fix, "cond");
        let music = self.apply_music_volume();
        self.then(loaded, music);
        self.flow(music, "then", fix);
        let fill =
            self.raw(r#"kind = "set_saved", defaults = { key = "unlocked", value = "1" }"#.into());
        self.flow(fix, "true", fill);
    }

    /// Flip `board[index]` between 0 and 1.
    fn toggle_cell(&mut self) {
        let ev = self.event_index("toggle_cell");
        let board = self.get("board");
        let cell = self.raw(r#"kind = "list_get", type = "int""#.into());
        self.data(board, "out", cell, "list");
        self.data(ev, "index", cell, "index");
        let off =
            self.raw(r#"kind = "compare", type = "int", op = "eq", defaults = { b = 0 }"#.into());
        self.data(cell, "out", off, "a");
        let flipped =
            self.raw(r#"kind = "select", type = "int", defaults = { a = 1, b = 0 }"#.into());
        self.data(off, "out", flipped, "cond");
        let written = self.raw(r#"kind = "list_set", type = "int""#.into());
        self.data(board, "out", written, "list");
        self.data(ev, "index", written, "index");
        self.data(flipped, "out", written, "item");
        let store = self.raw(r#"kind = "set", name = "board""#.into());
        self.data(written, "out", store, "value");
        self.then(ev, store);
    }

    fn paint_cell(&mut self) {
        let ev = self.event_index("paint_cell");
        let board = self.get("board");
        let cell = self.raw(r#"kind = "list_get", type = "int""#.into());
        self.data(board, "out", cell, "list");
        self.data(ev, "index", cell, "index");
        let on =
            self.raw(r#"kind = "compare", type = "int", op = "eq", defaults = { b = 1 }"#.into());
        self.data(cell, "out", on, "a");
        let word =
            self.raw(r#"kind = "select", type = "str", defaults = { a = "On", b = "Off" }"#.into());
        self.data(on, "out", word, "cond");
        let plus = self.raw(r#"kind = "add", type = "int", defaults = { b = 1 }"#.into());
        self.data(ev, "index", plus, "a");
        let digits = self.raw(r#"kind = "to_str", type = "int""#.into());
        self.data(plus, "out", digits, "a");
        let name = self.raw(r#"kind = "concat", defaults = { a = "Cell" }"#.into());
        self.data(digits, "out", name, "b");
        let label = self.raw(r#"kind = "ui_text""#.into());
        self.data(name, "out", label, "element");
        self.data(word, "out", label, "text");
        self.then(ev, label);
    }

    /// Toggle `press` and each orthogonal neighbour that stays on the 3×3 board.
    fn apply_press(&mut self) {
        let ev = self.event("apply_toggles");
        let press = self.get("press");
        let own = self.call("toggle_cell");
        self.data(press, "out", own, "index");
        self.then(ev, own);

        let col = self.raw(r#"kind = "mod", type = "int", defaults = { b = 3 }"#.into());
        self.data(press, "out", col, "a");
        let mut prev = own;
        prev = self.toggle_when(prev, press, col, "ne", 0, -1);
        prev = self.toggle_when(prev, press, col, "ne", 2, 1);
        prev = self.toggle_when(prev, press, press, "ge", 3, -3);
        self.toggle_when(prev, press, press, "lt", 6, 3);
    }

    /// After `prev`, toggle `press + delta` when `probe <op> rhs`. Both branch arms join.
    fn toggle_when(
        &mut self,
        prev: u32,
        press: u32,
        probe: u32,
        op: &str,
        rhs: i64,
        delta: i64,
    ) -> u32 {
        let cmp = self.raw(format!(
            r#"kind = "compare", type = "int", op = "{op}", defaults = {{ b = {rhs} }}"#
        ));
        self.data(probe, "out", cmp, "a");
        let gate = self.raw(r#"kind = "branch""#.into());
        self.data(cmp, "out", gate, "cond");
        self.then(prev, gate);
        let shifted = if delta < 0 {
            self.raw(format!(r#"kind = "sub", type = "int", defaults = {{ b = {} }}"#, -delta))
        } else {
            self.raw(format!(r#"kind = "add", type = "int", defaults = {{ b = {delta} }}"#))
        };
        self.data(press, "out", shifted, "a");
        let flip = self.call("toggle_cell");
        self.data(shifted, "out", flip, "index");
        self.flow(gate, "true", flip);
        // A pass-through so both branch arms have one `then` to continue from.
        let join = self.raw(r#"kind = "set", name = "press""#.into());
        self.data(press, "out", join, "value");
        self.then(flip, join);
        self.flow(gate, "false", join);
        join
    }

    fn refresh_board(&mut self) {
        let ev = self.event("refresh_board");
        let looped = self.raw(r#"kind = "for", defaults = { first = 0, last = 8 }"#.into());
        self.then(ev, looped);
        let paint = self.call("paint_cell");
        self.data(looped, "index", paint, "index");
        self.flow(looped, "body", paint);

        let moves = self.get("moves");
        let moves_text = self.raw(r#"kind = "to_str", type = "int""#.into());
        self.data(moves, "out", moves_text, "a");
        let moves_line = self.raw(r#"kind = "concat", defaults = { a = "Moves: " }"#.into());
        self.data(moves_text, "out", moves_line, "b");
        let moves_ui =
            self.raw(r#"kind = "ui_text", defaults = { element = "MovesLabel" }"#.into());
        self.data(moves_line, "out", moves_ui, "text");
        self.flow(looped, "done", moves_ui);
        let stats = self.raw(r#"kind = "ui_text", defaults = { element = "Stats" }"#.into());
        self.data(moves_line, "out", stats, "text");
        self.then(moves_ui, stats);

        let level = self.get("level");
        let level_text = self.raw(r#"kind = "to_str", type = "int""#.into());
        self.data(level, "out", level_text, "a");
        let level_line = self.raw(r#"kind = "concat", defaults = { a = "Level " }"#.into());
        self.data(level_text, "out", level_line, "b");
        let level_ui =
            self.raw(r#"kind = "ui_text", defaults = { element = "LevelLabel" }"#.into());
        self.data(level_line, "out", level_ui, "text");
        self.then(stats, level_ui);
    }

    fn check_win(&mut self) {
        let ev = self.event("check_win");
        let reset =
            self.raw(r#"kind = "set", name = "all_lit", defaults = { value = true }"#.into());
        self.then(ev, reset);
        let looped = self.raw(r#"kind = "for", defaults = { first = 0, last = 8 }"#.into());
        self.then(reset, looped);
        let board = self.get("board");
        let cell = self.raw(r#"kind = "list_get", type = "int""#.into());
        self.data(board, "out", cell, "list");
        self.data(looped, "index", cell, "index");
        let lit =
            self.raw(r#"kind = "compare", type = "int", op = "eq", defaults = { b = 1 }"#.into());
        self.data(cell, "out", lit, "a");
        let flag = self.get("all_lit");
        let both = self.raw(r#"kind = "and""#.into());
        self.data(flag, "out", both, "a");
        self.data(lit, "out", both, "b");
        let store = self.raw(r#"kind = "set", name = "all_lit""#.into());
        self.data(both, "out", store, "value");
        self.flow(looped, "body", store);

        let now = self.get("all_lit");
        let gate = self.raw(r#"kind = "branch""#.into());
        self.data(now, "out", gate, "cond");
        self.flow(looped, "done", gate);

        let volume = self.sound_volume();
        let sound = self.play_click(volume, "1.3");
        self.flow(gate, "true", sound);

        let level = self.get("level");
        let next = self.raw(r#"kind = "add", type = "int", defaults = { b = 1 }"#.into());
        self.data(level, "out", next, "a");
        let capped = self.raw(r#"kind = "min", type = "int", defaults = { b = 3 }"#.into());
        self.data(next, "out", capped, "a");
        let saved = self.raw(r#"kind = "saved", defaults = { key = "unlocked" }"#.into());
        let have = self.raw(r#"kind = "parse_int""#.into());
        self.data(saved, "out", have, "a");
        let at_least = self.raw(r#"kind = "max", type = "int", defaults = { b = 1 }"#.into());
        self.data(have, "out", at_least, "a");
        let best = self.raw(r#"kind = "max", type = "int""#.into());
        self.data(at_least, "out", best, "a");
        self.data(capped, "out", best, "b");
        let best_text = self.raw(r#"kind = "to_str", type = "int""#.into());
        self.data(best, "out", best_text, "a");
        let keep = self.raw(r#"kind = "set_saved", defaults = { key = "unlocked" }"#.into());
        self.data(best_text, "out", keep, "value");
        self.then(sound, keep);
        let save = self.raw(r#"kind = "save_game", defaults = { slot = "progress" }"#.into());
        self.then(keep, save);

        let last =
            self.raw(r#"kind = "compare", type = "int", op = "ge", defaults = { b = 3 }"#.into());
        self.data(level, "out", last, "a");
        let next_label = self.raw(
            r#"kind = "select", type = "str", defaults = { a = "Back to Menu", b = "Next Level" }"#
                .into(),
        );
        self.data(last, "out", next_label, "cond");
        let next_ui = self.raw(r#"kind = "ui_text", defaults = { element = "NextLevel" }"#.into());
        self.data(next_label, "out", next_ui, "text");
        self.then(save, next_ui);
        let win = self.ui_show("win_panel", true);
        self.then(next_ui, win);
    }

    fn on_press(&mut self) {
        let ev = self.event("on_press");
        let history = self.get("history");
        let press = self.get("press");
        let pushed = self.raw(r#"kind = "list_push", type = "int""#.into());
        self.data(history, "out", pushed, "list");
        self.data(press, "out", pushed, "item");
        let store = self.raw(r#"kind = "set", name = "history""#.into());
        self.data(pushed, "out", store, "value");
        self.then(ev, store);
        let apply = self.call("apply_toggles");
        self.then(store, apply);
        let moves = self.get("moves");
        let bumped = self.raw(r#"kind = "add", type = "int", defaults = { b = 1 }"#.into());
        self.data(moves, "out", bumped, "a");
        let save_moves = self.raw(r#"kind = "set", name = "moves""#.into());
        self.data(bumped, "out", save_moves, "value");
        self.then(apply, save_moves);
        let volume = self.sound_volume();
        let sound = self.play_click(volume, "1.0");
        self.then(save_moves, sound);
        let refresh = self.call("refresh_board");
        self.then(sound, refresh);
        let win = self.call("check_win");
        self.then(refresh, win);
    }

    fn begin_level(&mut self) {
        let ev = self.event("begin_level");
        let zero = self.raw(r#"kind = "set", name = "moves", defaults = { value = 0 }"#.into());
        self.then(ev, zero);
        let empty = self.raw(r#"kind = "list_make", type = "int", count = 0"#.into());
        let clear = self.raw(r#"kind = "set", name = "history""#.into());
        self.data(empty, "out", clear, "value");
        self.then(zero, clear);

        let mut boards = Vec::with_capacity(LEVEL_STARTS.len());
        for bits in LEVEL_STARTS {
            boards.push(self.list_from_bits(bits));
        }
        let level = self.get("level");
        let is_2 =
            self.raw(r#"kind = "compare", type = "int", op = "eq", defaults = { b = 2 }"#.into());
        self.data(level, "out", is_2, "a");
        let pick_2 = self.raw(r#"kind = "select", type = "list<int>""#.into());
        self.data(is_2, "out", pick_2, "cond");
        self.data(boards[1], "out", pick_2, "a");
        self.data(boards[2], "out", pick_2, "b");
        let is_1 =
            self.raw(r#"kind = "compare", type = "int", op = "eq", defaults = { b = 1 }"#.into());
        self.data(level, "out", is_1, "a");
        let pick = self.raw(r#"kind = "select", type = "list<int>""#.into());
        self.data(is_1, "out", pick, "cond");
        self.data(boards[0], "out", pick, "a");
        self.data(pick_2, "out", pick, "b");
        let store = self.raw(r#"kind = "set", name = "board""#.into());
        self.data(pick, "out", store, "value");
        self.then(clear, store);
        let hide = self.ui_show("win_panel", false);
        self.then(store, hide);
        let refresh = self.call("refresh_board");
        self.then(hide, refresh);
    }

    fn cell_buttons(&mut self) {
        for cell in 0..9 {
            let ev = self.event(&format!("ui_Cell{}_clicked", cell + 1));
            let set = self
                .raw(format!(r#"kind = "set", name = "press", defaults = {{ value = {cell} }}"#));
            self.then(ev, set);
            let go = self.call("on_press");
            self.then(set, go);
        }
    }

    fn undo_restart(&mut self) {
        let undo = self.event("ui_UndoBtn_clicked");
        let history = self.get("history");
        let len = self.raw(r#"kind = "list_len", type = "int""#.into());
        self.data(history, "out", len, "list");
        let some =
            self.raw(r#"kind = "compare", type = "int", op = "gt", defaults = { b = 0 }"#.into());
        self.data(len, "out", some, "a");
        let gate = self.raw(r#"kind = "branch""#.into());
        self.data(some, "out", gate, "cond");
        self.then(undo, gate);
        let last = self.raw(r#"kind = "sub", type = "int", defaults = { b = 1 }"#.into());
        self.data(len, "out", last, "a");
        let cell = self.raw(r#"kind = "list_get", type = "int""#.into());
        self.data(history, "out", cell, "list");
        self.data(last, "out", cell, "index");
        let remember = self.raw(r#"kind = "set", name = "press""#.into());
        self.data(cell, "out", remember, "value");
        self.flow(gate, "true", remember);
        let trimmed = self.raw(r#"kind = "list_remove_at", type = "int""#.into());
        self.data(history, "out", trimmed, "list");
        self.data(last, "out", trimmed, "index");
        let store = self.raw(r#"kind = "set", name = "history""#.into());
        self.data(trimmed, "out", store, "value");
        self.then(remember, store);
        let moves = self.get("moves");
        let down = self.raw(r#"kind = "sub", type = "int", defaults = { b = 1 }"#.into());
        self.data(moves, "out", down, "a");
        let floored = self.raw(r#"kind = "max", type = "int", defaults = { b = 0 }"#.into());
        self.data(down, "out", floored, "a");
        let save_moves = self.raw(r#"kind = "set", name = "moves""#.into());
        self.data(floored, "out", save_moves, "value");
        self.then(store, save_moves);
        let apply = self.call("apply_toggles");
        self.then(save_moves, apply);
        let refresh = self.call("refresh_board");
        self.then(apply, refresh);
        let hide = self.ui_show("win_panel", false);
        self.then(refresh, hide);

        let restart = self.event("ui_RestartBtn_clicked");
        let again = self.call("begin_level");
        self.then(restart, again);
    }

    fn menus(&mut self) {
        let play = self.event("ui_Play_clicked");
        let level1 = self.raw(r#"kind = "set", name = "level", defaults = { value = 1 }"#.into());
        self.then(play, level1);
        let begin = self.call("begin_level");
        self.then(level1, begin);
        self.show_only(begin, "then", "level_play");

        let open_select = self.event("ui_LevelSelect_clicked");
        let shown = self.show_only(open_select, "then", "level_select");
        let locks = self.call("refresh_locks");
        self.then(shown, locks);
        self.refresh_locks();

        for (event, level) in
            [("ui_Level1_clicked", 1_i64), ("ui_Level2_clicked", 2), ("ui_Level3_clicked", 3)]
        {
            self.open_level(event, level);
        }

        let back = self.event("ui_Back_clicked");
        let unpaused =
            self.raw(r#"kind = "set", name = "paused", defaults = { value = false }"#.into());
        self.then(back, unpaused);
        self.show_only(unpaused, "then", "main_menu");

        let next = self.event("ui_NextLevel_clicked");
        let level = self.get("level");
        let done =
            self.raw(r#"kind = "compare", type = "int", op = "ge", defaults = { b = 3 }"#.into());
        self.data(level, "out", done, "a");
        let gate = self.raw(r#"kind = "branch""#.into());
        self.data(done, "out", gate, "cond");
        self.then(next, gate);
        let clear =
            self.raw(r#"kind = "set", name = "paused", defaults = { value = false }"#.into());
        self.flow(gate, "true", clear);
        self.show_only(clear, "then", "main_menu");
        let raised = self.raw(r#"kind = "add", type = "int", defaults = { b = 1 }"#.into());
        self.data(level, "out", raised, "a");
        let save_level = self.raw(r#"kind = "set", name = "level""#.into());
        self.data(raised, "out", save_level, "value");
        self.flow(gate, "false", save_level);
        let begin_next = self.call("begin_level");
        self.then(save_level, begin_next);
        self.show_only(begin_next, "then", "level_play");

        let from_win = self.event("ui_LevelSelectWin_clicked");
        let select = self.show_only(from_win, "then", "level_select");
        let locks_again = self.call("refresh_locks");
        self.then(select, locks_again);

        let pause_quit = self.event("ui_PauseQuit_clicked");
        let clear_pause =
            self.raw(r#"kind = "set", name = "paused", defaults = { value = false }"#.into());
        self.then(pause_quit, clear_pause);
        self.show_only(clear_pause, "then", "main_menu");

        let options = self.event("ui_Options_clicked");
        let not_paused =
            self.raw(r#"kind = "set", name = "paused", defaults = { value = false }"#.into());
        self.then(options, not_paused);
        let hide_menu = self.ui_show("main_menu", false);
        self.then(not_paused, hide_menu);
        let show_options = self.ui_show("options", true);
        self.then(hide_menu, show_options);

        let pause = self.event("ui_PauseBtn_clicked");
        let mark = self.raw(r#"kind = "set", name = "paused", defaults = { value = true }"#.into());
        self.then(pause, mark);
        let show_pause = self.ui_show("pause_menu", true);
        self.then(mark, show_pause);

        let resume = self.event("ui_Resume_clicked");
        let drop =
            self.raw(r#"kind = "set", name = "paused", defaults = { value = false }"#.into());
        self.then(resume, drop);
        let hide_pause = self.ui_show("pause_menu", false);
        self.then(drop, hide_pause);

        let pause_options = self.event("ui_PauseOptions_clicked");
        let stay = self.raw(r#"kind = "set", name = "paused", defaults = { value = true }"#.into());
        self.then(pause_options, stay);
        let hide = self.ui_show("pause_menu", false);
        self.then(stay, hide);
        let show = self.ui_show("options", true);
        self.then(hide, show);

        let options_back = self.event("ui_OptionsBack_clicked");
        let hide_options = self.ui_show("options", false);
        self.then(options_back, hide_options);
        let paused = self.get("paused");
        let which = self.raw(r#"kind = "branch""#.into());
        self.data(paused, "out", which, "cond");
        self.then(hide_options, which);
        let back_pause = self.ui_show("pause_menu", true);
        self.flow(which, "true", back_pause);
        let back_menu = self.ui_show("main_menu", true);
        self.flow(which, "false", back_menu);

        let rebind = self.event("ui_RebindBtn_clicked");
        let action = self.raw(
            r#"kind = "rebind_action", defaults = { action = "ui_confirm", slot = 0 }"#.into(),
        );
        self.then(rebind, action);

        let music = self.slider("ui_MusicVol_changed", "MusicVol");
        let volume = self.apply_music_volume();
        self.then(music, volume);
        let quit = self.event("ui_Quit_clicked");
        let leave = self.raw(r#"kind = "set_game_state", defaults = { state = "quit" }"#.into());
        self.then(quit, leave);
        self.slider("ui_SoundVol_changed", "SoundVol");
    }

    fn refresh_locks(&mut self) {
        let ev = self.event("refresh_locks");
        let saved = self.raw(r#"kind = "saved", defaults = { key = "unlocked" }"#.into());
        let parsed = self.raw(r#"kind = "parse_int""#.into());
        self.data(saved, "out", parsed, "a");
        let rank = self.raw(r#"kind = "max", type = "int", defaults = { b = 1 }"#.into());
        self.data(parsed, "out", rank, "a");
        let first = self
            .raw(r#"kind = "ui_text", defaults = { element = "Level1", text = "Level 1" }"#.into());
        self.then(ev, first);
        let second = self.lock_label(rank, 2, "Level2", "Level 2", "Level 2 Locked");
        self.then(first, second);
        let third = self.lock_label(rank, 3, "Level3", "Level 3", "Level 3 Locked");
        self.then(second, third);
    }

    fn lock_label(&mut self, rank: u32, need: i64, element: &str, open: &str, locked: &str) -> u32 {
        let cmp = self.raw(format!(
            r#"kind = "compare", type = "int", op = "ge", defaults = {{ b = {need} }}"#
        ));
        self.data(rank, "out", cmp, "a");
        let word = self.raw(format!(
            r#"kind = "select", type = "str", defaults = {{ a = "{open}", b = "{locked}" }}"#
        ));
        self.data(cmp, "out", word, "cond");
        let ui = self.raw(format!(r#"kind = "ui_text", defaults = {{ element = "{element}" }}"#));
        self.data(word, "out", ui, "text");
        ui
    }

    fn open_level(&mut self, event: &str, level: i64) {
        let ev = self.event(event);
        let tail = if level > 1 {
            let saved = self.raw(r#"kind = "saved", defaults = { key = "unlocked" }"#.into());
            let parsed = self.raw(r#"kind = "parse_int""#.into());
            self.data(saved, "out", parsed, "a");
            let rank = self.raw(r#"kind = "max", type = "int", defaults = { b = 1 }"#.into());
            self.data(parsed, "out", rank, "a");
            let open = self.raw(format!(
                r#"kind = "compare", type = "int", op = "ge", defaults = {{ b = {level} }}"#
            ));
            self.data(rank, "out", open, "a");
            let gate = self.raw(r#"kind = "branch""#.into());
            self.data(open, "out", gate, "cond");
            self.then(ev, gate);
            let set = self
                .raw(format!(r#"kind = "set", name = "level", defaults = {{ value = {level} }}"#));
            self.flow(gate, "true", set);
            set
        } else {
            let set = self.raw(r#"kind = "set", name = "level", defaults = { value = 1 }"#.into());
            self.then(ev, set);
            set
        };
        let begin = self.call("begin_level");
        self.then(tail, begin);
        self.show_only(begin, "then", "level_play");
    }

    fn slider(&mut self, event: &str, key: &str) -> u32 {
        let ev = self.event(event);
        let value = self.raw(format!(r#"kind = "ui_value", defaults = {{ element = "{key}" }}"#));
        let text = self.raw(r#"kind = "to_str", type = "float""#.into());
        self.data(value, "out", text, "a");
        let save = self.raw(format!(r#"kind = "set_saved", defaults = {{ key = "{key}" }}"#));
        self.data(text, "out", save, "value");
        self.then(ev, save)
    }

    fn finish(self) -> String {
        let vars = r#"vars = [
  { name = "board", type = "list<int>", default = [0, 0, 0, 0, 0, 0, 0, 0, 0] },
  { name = "history", type = "list<int>", default = [] },
  { name = "moves", type = "int", default = 0 },
  { name = "level", type = "int", default = 1 },
  { name = "press", type = "int", default = 0 },
  { name = "all_lit", type = "bool", default = false },
  { name = "booted", type = "int", default = 0 },
  { name = "paused", type = "bool", default = false },
]"#;
        format!(
            "schema_version = 1\nseal_graph = \"puzzle\"\n{vars}\nnodes = [\n{}\n]\nlinks = [\n  {}\n]\n",
            self.nodes.join(",\n"),
            self.links.join(", ")
        )
    }
}
