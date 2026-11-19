//! Top-down adventure starter: village and cave scenes, scripts, art and UI.

mod graphs;

use std::fs;
use std::path::Path;

use seal_world::phys2d;
use seal_world::World;

use crate::error::CoreError;

pub fn create(folder: &Path, project_name: &str) -> Result<(), CoreError> {
    for sub in ["sprites", "anims", "logic", "scenes", "ui", "audio", "tiles", "prefabs"] {
        fs::create_dir_all(folder.join(sub))?;
    }
    write_art(folder)?;
    graphs::write_all(folder)?;
    write_input(folder)?;
    write_ui(folder)?;
    write_audio(folder)?;
    fs::write(folder.join("scene.seal"), build_village(project_name).to_text())?;
    fs::write(folder.join("scenes/cave.seal"), build_cave().to_text())?;
    Ok(())
}

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), CoreError> {
    let file = std::io::BufWriter::new(fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .map_err(|e| CoreError::new(e.to_string()))?
        .write_image_data(rgba)
        .map_err(|e| CoreError::new(e.to_string()))?;
    Ok(())
}

fn solid_rect(w: u32, h: u32, fill: [u8; 4], border: Option<[u8; 4]>) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let edge = border.is_some_and(|_| x == 0 || y == 0 || x + 1 == w || y + 1 == h);
            let c = if edge { border.unwrap_or(fill) } else { fill };
            let i = ((y * w + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&c);
        }
    }
    px
}

fn write_art(folder: &Path) -> Result<(), CoreError> {
    let s = folder.join("sprites");
    write_png(
        &s.join("grass.png"),
        16,
        16,
        &solid_rect(16, 16, [70, 150, 70, 255], Some([40, 90, 40, 255])),
    )?;
    write_png(
        &s.join("path.png"),
        16,
        16,
        &solid_rect(16, 16, [170, 140, 100, 255], Some([120, 95, 70, 255])),
    )?;
    write_png(
        &s.join("wall.png"),
        16,
        16,
        &solid_rect(16, 16, [90, 70, 55, 255], Some([50, 40, 32, 255])),
    )?;
    write_png(
        &s.join("gem.png"),
        12,
        12,
        &solid_rect(12, 12, [60, 200, 255, 255], Some([20, 80, 140, 255])),
    )?;
    write_png(&s.join("heart.png"), 12, 12, &solid_rect(12, 12, [230, 60, 80, 255], None))?;
    write_png(&s.join("heart_empty.png"), 12, 12, &solid_rect(12, 12, [50, 30, 35, 255], None))?;
    write_png(
        &s.join("door.png"),
        16,
        20,
        &solid_rect(16, 20, [120, 80, 40, 255], Some([70, 45, 25, 255])),
    )?;
    write_png(
        &s.join("save.png"),
        14,
        14,
        &solid_rect(14, 14, [200, 200, 240, 255], Some([100, 100, 150, 255])),
    )?;
    write_png(&s.join("hero_sheet.png"), 128, 16, &sheet_pixels(8, [50, 120, 220]))?;
    write_png(&s.join("enemy_sheet.png"), 128, 16, &sheet_pixels(8, [210, 50, 50]))?;
    write_png(&s.join("npc_sheet.png"), 64, 16, &sheet_pixels(4, [180, 120, 200]))?;
    fs::write(folder.join("anims/hero.frames"), directional_frames("../sprites/hero_sheet.png"))?;
    fs::write(folder.join("anims/enemy.frames"), directional_frames("../sprites/enemy_sheet.png"))?;
    fs::write(
        folder.join("anims/npc.frames"),
        "\
animation idle fps=4 loop=yes\n\
frame ../sprites/npc_sheet.png 0 0 0.25 1\n\
frame ../sprites/npc_sheet.png 0.25 0 0.5 1\n\
animation face_left fps=4 loop=no\n\
frame ../sprites/npc_sheet.png 0.5 0 0.75 1\n\
animation face_right fps=4 loop=no\n\
frame ../sprites/npc_sheet.png 0.75 0 1 1\n",
    )?;
    Ok(())
}

/// Four directions, two frames each, on an eight-column sheet.
fn directional_frames(sheet: &str) -> String {
    let mut out = String::new();
    let clips = [
        ("walk_right", 0.0, 0.125, 0.125, 0.25, true),
        ("walk_left", 0.25, 0.375, 0.375, 0.5, true),
        ("walk_up", 0.5, 0.625, 0.625, 0.75, true),
        ("walk_down", 0.75, 0.875, 0.875, 1.0, true),
        ("idle_right", 0.0, 0.125, 0.0, 0.0, false),
        ("idle_left", 0.25, 0.375, 0.0, 0.0, false),
        ("idle_up", 0.5, 0.625, 0.0, 0.0, false),
        ("idle_down", 0.75, 0.875, 0.0, 0.0, false),
    ];
    for (name, a0, a1, b0, b1, two) in clips {
        let looped = if name.starts_with("walk") { "yes" } else { "no" };
        out.push_str(&format!("animation {name} fps=8 loop={looped}\n"));
        out.push_str(&format!("frame {sheet} {a0} 0 {a1} 1\n"));
        if two {
            out.push_str(&format!("frame {sheet} {b0} 0 {b1} 1\n"));
        }
    }
    out
}

fn sheet_pixels(columns: u32, rgb: [u8; 3]) -> Vec<u8> {
    let (w, h) = (columns * 16, 16u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    for col in 0..columns {
        let shade = if col % 2 == 0 { 0 } else { 30u8 };
        let fill = [
            rgb[0].saturating_sub(shade),
            rgb[1].saturating_sub(shade),
            rgb[2].saturating_sub(shade),
            255,
        ];
        let x0 = col * 16;
        for y in 2..14u32 {
            for x in (x0 + 4)..(x0 + 12) {
                let i = ((y * w + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&fill);
            }
        }
    }
    px
}

fn write_input(folder: &Path) -> Result<(), CoreError> {
    let text = "\
# Top-down adventure controls
action move_left = key.a, key.arrowleft, pad.dpad_left
action move_right = key.d, key.arrowright, pad.dpad_right
action move_up = key.w, key.arrowup, pad.dpad_up
action move_down = key.s, key.arrowdown, pad.dpad_down
action interact = key.e, pad.west
action pause = key.escape, pad.start
axis move_x = key.a key.d, key.arrowleft key.arrowright, pad.dpad_left pad.dpad_right, pad.left_x deadzone 0.25
axis move_y = key.s key.w, key.arrowdown key.arrowup, pad.dpad_down pad.dpad_up, pad.left_y deadzone 0.25
";
    fs::write(folder.join("input.sealinput"), text)?;
    Ok(())
}

fn write_ui(folder: &Path) -> Result<(), CoreError> {
    fs::write(
        folder.join("ui/Hud.sealui"),
        r#"seal ui 1
panel HudRoot
  anchor 0 1 0 1
  offset 12 -56 220 -12
  hbox Hearts
    anchor 0 0 1 0.5
    spacing 4
    image H1
      picture sprites/heart.png
      size 20 20
    image H2
      picture sprites/heart.png
      size 20 20
    image H3
      picture sprites/heart.png
      size 20 20
  label Gems
    anchor 0 0.5 1 1
    text Gems: 0
"#,
    )?;
    fs::write(
        folder.join("ui/Pause.sealui"),
        r#"seal ui 1
panel PauseMenu
  anchor 0.5 0.5 0.5 0.5
  offset -140 -120 140 120
  vbox Box
    anchor 0 0 1 1
    spacing 8
    label Title
      text Paused
      align center
    button Resume
      text Resume
    button Restart
      text Restart
    button Quit
      text Quit
"#,
    )?;
    fs::write(
        folder.join("ui/Dialog.sealui"),
        r#"seal ui 1
panel DialogBox
  anchor 0.5 0 0.5 0
  offset -180 40 180 120
  label Talk
    anchor 0 0 1 1
    text Welcome, traveler.
    align center
"#,
    )?;
    Ok(())
}

fn write_audio(folder: &Path) -> Result<(), CoreError> {
    // Minimal silent WAV stubs so play_sound nodes find a file.
    for name in ["pickup", "hurt", "save"] {
        write_wav_stub(&folder.join(format!("audio/{name}.wav")))?;
    }
    Ok(())
}

fn write_wav_stub(path: &Path) -> Result<(), CoreError> {
    let mut data = Vec::new();
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&(36u32 + 2).to_le_bytes());
    data.extend_from_slice(b"WAVEfmt ");
    data.extend_from_slice(&16u32.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&22_050u32.to_le_bytes());
    data.extend_from_slice(&44_100u32.to_le_bytes());
    data.extend_from_slice(&2u16.to_le_bytes());
    data.extend_from_slice(&16u16.to_le_bytes());
    data.extend_from_slice(b"data");
    data.extend_from_slice(&2u32.to_le_bytes());
    data.extend_from_slice(&[0u8, 0u8]);
    fs::write(path, data)?;
    Ok(())
}

fn base_world(name: &str) -> World {
    let mut w = World::from_text(&World::empty_scene_text(name)).expect("empty scene");
    w.gravity = 0.0;
    w
}

fn sprite_object(
    w: &mut World,
    parent: &str,
    name: &str,
    at: [f32; 3],
    sprite: &str,
    tag: &str,
    dynamic: bool,
) -> String {
    let id = w.add("empty");
    let e = w.entity_mut(&id).expect("added");
    e.parent = Some(parent.into());
    e.name = name.into();
    e.translation = at;
    e.tag = tag.into();
    e.dynamic = dynamic;
    e.sprite = sprite.into();
    id
}

fn solid_wall(w: &mut World, parent: &str, name: &str, at: [f32; 3]) -> String {
    let id = sprite_object(w, parent, name, at, "sprites/wall.png", "wall", false);
    let e = w.entity_mut(&id).expect("wall");
    assert!(phys2d::apply(&mut e.phys2d, "phys_body", "fixed"));
    assert!(phys2d::apply(&mut e.phys2d, "phys_shape", "box"));
    assert!(phys2d::apply(&mut e.phys2d, "phys_size", "1, 1"));
    id
}

fn floor_tile(w: &mut World, parent: &str, x: f32, y: f32, path: bool) -> String {
    let sprite = if path { "sprites/path.png" } else { "sprites/grass.png" };
    sprite_object(w, parent, &format!("Tile {x:.0},{y:.0}"), [x, y, 0.0], sprite, "floor", false)
}

fn tiles_folder(w: &mut World) -> String {
    let id = w.add("empty");
    let e = w.entity_mut(&id).expect("just added");
    e.name = "Tiles".into();
    e.tag = "folder".into();
    id
}

fn build_village(project_name: &str) -> World {
    let mut w = base_world(project_name);
    w.entity_mut("root").expect("root").name = "Village".into();
    let root = "root".to_string();
    let tiles = tiles_folder(&mut w);
    for y in -3i32..=3 {
        for x in -6i32..=6 {
            floor_tile(&mut w, &tiles, x as f32, y as f32, y == 0 && x.abs() <= 2);
        }
    }
    for &(x, y) in &[(-6, -3), (6, -3), (-6, 3), (6, 3)] {
        solid_wall(&mut w, &root, "Wall", [x as f32, y as f32, 0.0]);
    }
    solid_wall(&mut w, &root, "North", [0.0, 3.0, 0.0]);
    solid_wall(&mut w, &root, "South", [0.0, -3.0, 0.0]);
    let player =
        sprite_object(&mut w, &root, "Hero", [-4.5, 0.0, 0.0], "anims/hero.frames", "player", true);
    {
        let e = w.entity_mut(&player).expect("hero");
        assert!(phys2d::apply(&mut e.phys2d, "phys_body", "dynamic"));
        assert!(phys2d::apply(&mut e.phys2d, "phys_shape", "box"));
        assert!(phys2d::apply(&mut e.phys2d, "phys_size", "0.7, 0.7"));
        e.script = "logic/player.graph".into();
    }
    let cam = w.add("camera");
    {
        let e = w.entity_mut(&cam).expect("cam");
        e.parent = Some(player.clone());
        e.name = "Follow Camera".into();
        e.camera2d.on = true;
        e.camera2d.follow = "Hero".into();
        e.camera2d.zoom = 3.0;
        e.camera2d.smoothing = 6.0;
    }
    for (i, (x, y)) in [(-1.5, 0.5), (0.5, 1.5), (2.0, -0.5)].into_iter().enumerate() {
        let id = sprite_object(
            &mut w,
            &root,
            &format!("Gem {}", i + 1),
            [x, y, -0.05],
            "sprites/gem.png",
            "pickup",
            false,
        );
        let e = w.entity_mut(&id).expect("gem");
        assert!(phys2d::apply(&mut e.phys2d, "phys_body", "fixed"));
        assert!(phys2d::apply(&mut e.phys2d, "phys_shape", "box"));
        assert!(phys2d::apply(&mut e.phys2d, "phys_size", "0.5, 0.5"));
        e.script = "logic/pickup.graph".into();
    }
    place_enemy(&mut w, &root, "Enemy 1", [5.0, -2.5, 0.0]);
    let npc =
        sprite_object(&mut w, &root, "Elder", [1.0, 2.2, 0.0], "anims/npc.frames", "npc", false);
    {
        let e = w.entity_mut(&npc).expect("npc");
        assert!(phys2d::apply(&mut e.phys2d, "phys_body", "fixed"));
        assert!(phys2d::apply(&mut e.phys2d, "phys_shape", "box"));
        assert!(phys2d::apply(&mut e.phys2d, "phys_size", "0.6, 0.8"));
        e.script = "logic/npc.graph".into();
    }
    let door = sprite_object(
        &mut w,
        &root,
        "Cave Door",
        [5.5, 0.0, 0.0],
        "sprites/door.png",
        "door",
        false,
    );
    {
        let e = w.entity_mut(&door).expect("door");
        assert!(phys2d::apply(&mut e.phys2d, "phys_body", "fixed"));
        assert!(phys2d::apply(&mut e.phys2d, "phys_shape", "box"));
        assert!(phys2d::apply(&mut e.phys2d, "phys_size", "0.8, 1.0"));
        e.script = "logic/door_village.graph".into();
    }
    let save = sprite_object(
        &mut w,
        &root,
        "Shrine",
        [-5.5, -2.0, 0.0],
        "sprites/save.png",
        "save",
        false,
    );
    {
        let e = w.entity_mut(&save).expect("save");
        assert!(phys2d::apply(&mut e.phys2d, "phys_body", "fixed"));
        assert!(phys2d::apply(&mut e.phys2d, "phys_shape", "box"));
        assert!(phys2d::apply(&mut e.phys2d, "phys_size", "0.6, 0.6"));
        e.script = "logic/save.graph".into();
    }
    let mgr = sprite_object(&mut w, &root, "Game", [0.0, 0.0, 10.0], "", "game", false);
    {
        let e = w.entity_mut(&mgr).expect("mgr");
        e.hidden = true;
        e.script = "logic/game.graph".into();
    }
    w
}

fn build_cave() -> World {
    let mut w = base_world("Cave");
    w.entity_mut("root").expect("root").name = "Cave".into();
    let root = "root".to_string();
    let tiles = tiles_folder(&mut w);
    for y in -2..=2 {
        for x in -4..=4 {
            floor_tile(&mut w, &tiles, x as f32, y as f32, false);
        }
    }
    for &x in &[-4, 4] {
        solid_wall(&mut w, &root, "Side", [x as f32, 0.0, 0.0]);
    }
    let door =
        sprite_object(&mut w, &root, "Exit", [-3.5, 0.0, 0.0], "sprites/door.png", "door", false);
    {
        let e = w.entity_mut(&door).expect("door");
        assert!(phys2d::apply(&mut e.phys2d, "phys_body", "fixed"));
        assert!(phys2d::apply(&mut e.phys2d, "phys_shape", "box"));
        assert!(phys2d::apply(&mut e.phys2d, "phys_size", "0.8, 1.0"));
        e.script = "logic/door_cave.graph".into();
    }
    place_enemy(&mut w, &root, "Enemy 2", [2.0, 1.0, 0.0]);
    w
}

fn place_enemy(w: &mut World, parent: &str, name: &str, at: [f32; 3]) {
    let id = sprite_object(w, parent, name, at, "anims/enemy.frames", "enemy", false);
    let e = w.entity_mut(&id).expect("enemy");
    e.nav2d.on = true;
    e.nav2d.target = "Hero".into();
    // Slower than the hero, from the far corner: a new player has time to look around.
    e.nav2d.speed = 1.5;
    e.nav2d.radius = 0.35;
    e.nav2d.avoid = true;
    assert!(phys2d::apply(&mut e.phys2d, "phys_body", "kinematic"));
    assert!(phys2d::apply(&mut e.phys2d, "phys_shape", "box"));
    assert!(phys2d::apply(&mut e.phys2d, "phys_size", "0.7, 0.7"));
    e.script = "logic/enemy.graph".into();
}

#[cfg(test)]
mod tests {

    #[test]
    fn corrupt_frames_line_is_rejected() {
        assert!(seal_world::frames::SpriteFrames::parse("frame oops\n").is_err());
    }
}
