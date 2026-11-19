//! The sprite animation editor: every edit is written to the .frames file at once.

use seal_editor_core::asset_editor::{ASSET_CLIP_DRAFT, ASSET_DRAFT};
use seal_editor_core::sprite_frames::SpriteFrames;
use seal_editor_core::{CommandArgs, EditorSession, Layout};
use std::fs;
use std::path::PathBuf;

fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) {
    let file = std::io::BufWriter::new(fs::File::create(path).unwrap());
    let mut enc = png::Encoder::new(file, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn project() -> (PathBuf, EditorSession) {
    project_in("seal-anim-editor", 77)
}

fn project_in(name: &str, n: u64) -> (PathBuf, EditorSession) {
    let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(project.join("sprites")).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), n);
    fs::write(project.join("project.seal"), format!("schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n")).unwrap();
    // A 4x1 sheet of 8x8 cells; cells 0 and 2 are opaque, 1 and 3 empty.
    let (w, h) = (32u32, 8u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            if (x / 8) % 2 == 0 && x % 8 > 0 && x % 8 < 7 {
                let i = ((y * w + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
    }
    write_png(&project.join("sprites/hero.png"), w, h, &px);
    let path = project.to_string_lossy().to_string();
    let mut s = EditorSession::new(Layout::default());
    s.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    (project, s)
}

fn run(s: &mut EditorSession, cmd: &str, args: &[(&str, &str)]) -> Result<(), String> {
    s.dispatch(cmd, &CommandArgs::new(args)).map_err(|e| e.to_string())
}

fn lib(project: &std::path::Path) -> SpriteFrames {
    SpriteFrames::parse(&fs::read_to_string(project.join("anims/hero.frames")).unwrap()).unwrap()
}

#[test]
fn a_sprite_animation_is_built_and_saved_edit_by_edit() {
    let (project, mut s) = project();
    run(
        &mut s,
        "asset.frames_new",
        &[("path", "anims/hero"), ("image", "sprites/hero.png"), ("cols", "4")],
    )
    .unwrap();
    assert_eq!(s.draft(ASSET_DRAFT).as_deref(), Some("anims/hero.frames"));
    let l = lib(&project);
    assert_eq!(l.clips[0].name, "idle");
    assert_eq!(l.clips[0].frames.len(), 4);
    assert_eq!(l.clips[0].frames[0].image, "../sprites/hero.png");
    // Opening it again from the Asset Browser lands in the Asset Editor.
    run(&mut s, "content.open_file", &[("path", "anims/hero.frames")]).unwrap();

    // Automatic cutting finds the two opaque cells.
    run(
        &mut s,
        "asset.frames_slice",
        &[("clip", "idle"), ("image", "sprites/hero.png"), ("mode", "auto")],
    )
    .unwrap();
    assert_eq!(lib(&project).clips[0].frames.len(), 2);

    // A second clip, renamed, slowed down and set to play once.
    run(&mut s, "asset.frames_clip_add", &[("name", "clip")]).unwrap();
    let added = s.draft(ASSET_CLIP_DRAFT).unwrap();
    run(
        &mut s,
        "asset.frames_clip_set",
        &[("clip", &added), ("name", "jump"), ("fps", "4"), ("loop", "no")],
    )
    .unwrap();
    assert_eq!(s.draft(ASSET_CLIP_DRAFT).as_deref(), Some("jump"));
    assert!(run(&mut s, "asset.frames_clip_set", &[("clip", "jump"), ("name", "idle")]).is_err());
    assert!(
        run(&mut s, "asset.frames_clip_set", &[("clip", "jump"), ("name", "two words")]).is_err()
    );
    run(
        &mut s,
        "asset.frames_slice",
        &[
            ("clip", "jump"),
            ("image", "sprites/hero.png"),
            ("mode", "grid"),
            ("cols", "4"),
            ("first", "1"),
            ("count", "2"),
        ],
    )
    .unwrap();
    run(
        &mut s,
        "asset.frames_slice",
        &[("clip", "jump"), ("image", "sprites/hero.png"), ("mode", "whole"), ("append", "yes")],
    )
    .unwrap();
    let l = lib(&project);
    let jump = l.clip("jump").unwrap();
    assert_eq!((jump.fps, jump.looping, jump.frames.len()), (4.0, false, 3));
    assert_eq!(jump.frames[2].uv, [0.0, 0.0, 1.0, 1.0]);

    // Frames move, copy and go away.
    let first = jump.frames[0].clone();
    run(&mut s, "asset.frames_frame", &[("clip", "jump"), ("index", "0"), ("action", "right")])
        .unwrap();
    assert_eq!(lib(&project).clip("jump").unwrap().frames[1], first);
    run(&mut s, "asset.frames_frame", &[("clip", "jump"), ("index", "1"), ("action", "copy")])
        .unwrap();
    assert_eq!(lib(&project).clip("jump").unwrap().frames.len(), 4);
    run(&mut s, "asset.frames_frame", &[("clip", "jump"), ("index", "3"), ("action", "remove")])
        .unwrap();
    assert_eq!(lib(&project).clip("jump").unwrap().frames.len(), 3);
    assert!(run(
        &mut s,
        "asset.frames_frame",
        &[("clip", "jump"), ("index", "9"), ("action", "remove")]
    )
    .is_err());

    // Bad input is refused without touching the file.
    let before = fs::read_to_string(project.join("anims/hero.frames")).unwrap();
    assert!(run(&mut s, "asset.frames_slice", &[("clip", "jump"), ("image", "../outside.png")])
        .is_err());
    assert!(run(&mut s, "asset.frames_slice", &[("clip", "jump"), ("image", "sprites/none.png")])
        .is_err());
    assert!(run(&mut s, "asset.frames_clip_set", &[("clip", "jump"), ("fps", "fast")]).is_err());
    assert_eq!(fs::read_to_string(project.join("anims/hero.frames")).unwrap(), before);

    // Deleting a clip picks the one left.
    run(&mut s, "asset.frames_clip_remove", &[("clip", "jump")]).unwrap();
    assert_eq!(lib(&project).names(), vec!["idle".to_string()]);
    assert_eq!(s.draft(ASSET_CLIP_DRAFT).as_deref(), Some("idle"));
    assert!(run(&mut s, "asset.frames_new", &[("path", "anims/hero.frames")]).is_err());

    // "Make Animation" on a picture: next to it, cut automatically, never overwriting.
    for expect in ["sprites/hero.frames", "sprites/hero_2.frames"] {
        run(
            &mut s,
            "asset.frames_new",
            &[
                ("path", "sprites/hero"),
                ("image", "sprites/hero.png"),
                ("mode", "auto"),
                ("unique", "yes"),
            ],
        )
        .unwrap();
        assert_eq!(s.draft(ASSET_DRAFT).as_deref(), Some(expect));
        let l = SpriteFrames::parse(&fs::read_to_string(project.join(expect)).unwrap()).unwrap();
        assert_eq!(l.clips[0].frames.len(), 2);
        assert_eq!(l.clips[0].frames[0].image, "hero.png");
    }
}

#[test]
fn a_frame_event_is_saved_with_the_animation_and_an_empty_name_clears_it() {
    let (project, mut s) = project_in("seal-anim-editor-event", 78);
    run(
        &mut s,
        "asset.frames_new",
        &[("path", "anims/hero"), ("image", "sprites/hero.png"), ("cols", "2")],
    )
    .unwrap();
    run(
        &mut s,
        "asset.frames_frame",
        &[("clip", "idle"), ("index", "1"), ("action", "set"), ("event", " foot step ")],
    )
    .unwrap();
    let text = fs::read_to_string(project.join("anims/hero.frames")).unwrap();
    assert!(text.contains("event foot step"), "{text}");
    let saved = lib(&project);
    assert_eq!(saved.clip("idle").unwrap().frames[1].event, "foot step");
    assert!(saved.clip("idle").unwrap().frames[0].event.is_empty());
    run(
        &mut s,
        "asset.frames_frame",
        &[("clip", "idle"), ("index", "1"), ("action", "set"), ("event", "  ")],
    )
    .unwrap();
    assert!(lib(&project).clip("idle").unwrap().frames[1].event.is_empty());
}
