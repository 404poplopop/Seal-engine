//! Write Seal Hop as a Seal editor project: the level as scene objects that
//! draw the game's own pixel art, with the art saved as PNG files.
//!
//! cargo run -p seal-sample-2d --example editor_project -- OUT_DIR [LEVEL.txt]

use seal_editor_core::{create_project, World};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first().map(Path::new) else {
        eprintln!("usage: editor_project OUT_DIR [LEVEL.txt]");
        std::process::exit(2);
    };
    let level = match args.get(1) {
        Some(path) => std::fs::read_to_string(path).expect("read level"),
        None => seal_sample_2d::DEFAULT_LEVEL.to_string(),
    };
    // An existing project keeps its manifest; only the scene and art are rewritten.
    if !out.join("project.seal").is_file() {
        create_project(out, "SealHop").unwrap_or_else(|e| panic!("create project: {e}"));
    }
    let sprites = out.join("sprites");
    std::fs::create_dir_all(&sprites).expect("sprites folder");
    for (name, w, h, px) in seal_sample_2d::art_images() {
        write_png(&sprites.join(format!("{name}.png")), &px, w, h);
    }
    let mut world = scene(&level);
    save_actor_prefabs(&mut world, out);
    std::fs::write(out.join("scene.seal"), world.to_text()).expect("write scene");
    println!("wrote {} objects to {}", world.entities.len(), out.join("scene.seal").display());
}

/// The Seal, the coins and the flag are prefab instances: each kind is saved once as a prefab in
/// `prefabs/`, and the level's objects become copies that update from that file.
fn save_actor_prefabs(w: &mut World, out: &Path) {
    let dir = out.join("prefabs");
    std::fs::create_dir_all(&dir).expect("prefabs folder");
    for (tag, prefab) in [("player", "Seal"), ("coin", "Coin"), ("goal", "Flag")] {
        let ids: Vec<String> =
            w.entities.iter().filter(|e| e.tag == tag).map(|e| e.id.clone()).collect();
        let Some(first) = ids.first() else { continue };
        let mut text = w.prefab_text(first).expect("prefab text");
        // The prefab's own name is the kind, not "Coin 1".
        if let Some(e) = w.entity(first) {
            text = text.replacen(
                &format!("name = \"{}\"", e.name),
                &format!("name = \"{prefab}\""),
                1,
            );
        }
        std::fs::write(dir.join(format!("{prefab}.prefab")), text).expect("write prefab");
        for id in ids {
            let e = w.entity_mut(&id).expect("listed");
            e.source_kind = "prefab".into();
            e.source_asset = prefab.into();
        }
    }
}

fn write_png(path: &Path, rgba: &[u8], w: u32, h: u32) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).expect("create png"));
    let mut enc = png::Encoder::new(file, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().expect("png header").write_image_data(rgba).expect("png data");
}

struct Put<'a> {
    kind: &'a str,
    parent: &'a str,
    name: String,
    at: [f32; 3],
    scale: [f32; 2],
    sprite: &'a str,
    tag: &'a str,
}

fn put(w: &mut World, p: Put) -> String {
    let id = w.add(p.kind);
    let e = w.entity_mut(&id).expect("just added");
    e.parent = Some(p.parent.to_string());
    e.name = p.name;
    e.translation = p.at;
    e.scale = [p.scale[0], p.scale[1], 1.0];
    e.dynamic = false;
    e.tag = p.tag.into();
    if !p.sprite.is_empty() {
        e.sprite = format!("sprites/{}.png", p.sprite);
    }
    id
}

fn scene(level: &str) -> World {
    let rows: Vec<&str> = level.lines().skip_while(|l| l.trim().is_empty()).collect();
    let height = rows.len();
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0) as f32;
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
    w.entity_mut("root").expect("root").name = "Seal Hop".into();
    let folder = |w: &mut World, name: &str| {
        let id = w.add("empty");
        let e = w.entity_mut(&id).expect("just added");
        e.name = name.into();
        e.tag = "folder".into();
        id
    };
    let ground = folder(&mut w, "Ground");
    let coins = folder(&mut w, "Coins");
    let goal = folder(&mut w, "Goal");
    let player = folder(&mut w, "Player");
    let lighting = folder(&mut w, "Lighting");
    let x0 = width * 0.5;
    let (mut grass, mut dirt, mut coin) = (0, 0, 0);
    for (row, line) in rows.iter().enumerate() {
        let y = (height - 1 - row) as f32 + 0.5;
        for (x, c) in line.bytes().enumerate() {
            let cx = x as f32 + 0.5 - x0;
            match c {
                b'#' => {
                    let top = row == 0 || rows[row - 1].as_bytes().get(x) != Some(&b'#');
                    let (name, sprite) = if top {
                        grass += 1;
                        (format!("Grass {grass}"), "grass")
                    } else {
                        dirt += 1;
                        (format!("Dirt {dirt}"), "dirt")
                    };
                    let p = Put {
                        kind: "empty",
                        parent: &ground,
                        name,
                        at: [cx, y, 0.0],
                        scale: [1.0, 1.0],
                        sprite,
                        tag: "ground",
                    };
                    put(&mut w, p);
                }
                b'o' => {
                    coin += 1;
                    let p = Put {
                        kind: "empty",
                        parent: &coins,
                        name: format!("Coin {coin}"),
                        at: [cx, y, -0.1],
                        scale: [0.6, 0.6],
                        sprite: "coin",
                        tag: "coin",
                    };
                    put(&mut w, p);
                }
                b'F' => {
                    let p = Put {
                        kind: "empty",
                        parent: &goal,
                        name: "Flag".into(),
                        at: [cx, y + 0.5, -0.1],
                        scale: [1.0, 2.0],
                        sprite: "flag",
                        tag: "goal",
                    };
                    put(&mut w, p);
                }
                b'P' => {
                    let p = Put {
                        kind: "empty",
                        parent: &player,
                        name: "Seal".into(),
                        at: [cx, y, -0.2],
                        scale: [1.0, 1.0],
                        sprite: "seal",
                        tag: "player",
                    };
                    let id = put(&mut w, p);
                    w.entity_mut(&id).expect("just added").dynamic = true;
                }
                _ => {}
            }
        }
    }
    let sun = w.add("light");
    let e = w.entity_mut(&sun).expect("just added");
    e.parent = Some(lighting);
    e.name = "Sun".into();
    e.translation = [0.0, height as f32 + 6.0, 6.0];
    e.intensity = 6.0;
    let cam = w.add("camera");
    let e = w.entity_mut(&cam).expect("just added");
    e.parent = Some(player);
    e.name = "Side Camera".into();
    e.translation = [0.0, height as f32 * 0.5, 22.0];
    w
}
