use seal_world::frames::{auto_frames, grid_frames, Clip, Frame, SpriteFrames};
use seal_world::World;

const HERO: &str = "\
# hero animations
animation idle fps=4 loop=yes
frame hero.png 0 0 0.5 1
frame hero.png 0.5 0 1 1
animation jump fps=10 loop=no
frame jump.png
";

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("seal-frames-{name}-{}", std::process::id()));
    std::fs::create_dir_all(d.join("logic")).unwrap();
    d
}

#[test]
fn animation_files_read_and_write_back_the_same() {
    let lib = SpriteFrames::parse(HERO).unwrap();
    assert_eq!(lib.names(), vec!["idle", "jump"]);
    let idle = lib.clip("idle").unwrap();
    assert_eq!((idle.fps, idle.looping, idle.frames.len()), (4.0, true, 2));
    assert_eq!(idle.frames[1].uv, [0.5, 0.0, 1.0, 1.0]);
    assert_eq!(lib.clip("jump").unwrap().frames[0], Frame::whole("jump.png"));
    let again = SpriteFrames::parse(&lib.to_text()).unwrap();
    assert_eq!(again, lib);
    assert_eq!(lib.clip_or_first("missing").unwrap().name, "idle");
    assert_eq!(lib.unused_name("idle"), "idle 2");
}

#[test]
fn looping_clips_wrap_and_one_shot_clips_hold_the_last_frame() {
    let mut clip = Clip::new("run");
    clip.fps = 10.0;
    clip.frames = grid_frames("run.png", 4, 1, 0, 4);
    assert_eq!(clip.frame_at(0.0), 0);
    assert_eq!(clip.frame_at(0.1), 1);
    assert_eq!(clip.frame_at(0.45), 0);
    clip.looping = false;
    assert_eq!(clip.frame_at(5.0), 3);
    assert!(clip.finished(0.4) && !clip.finished(0.3));
}

#[test]
fn grid_slicing_reads_rows_left_to_right() {
    let f = grid_frames("s.png", 2, 2, 1, 3);
    assert_eq!(f.len(), 3);
    assert_eq!(f[0].uv, [0.5, 0.0, 1.0, 0.5]);
    assert_eq!(f[1].uv, [0.0, 0.5, 0.5, 1.0]);
    assert_eq!(grid_frames("s.png", 2, 2, 3, 10).len(), 1, "never past the last cell");
}

#[test]
fn auto_slicing_cuts_along_clear_gaps_into_equal_sized_frames() {
    // 8 x 4 sheet: a 2 x 2 block at x 0..2, y 1..3 and a 3 x 4 block at x 5..8.
    let (w, h) = (8usize, 4usize);
    let mut px = vec![0u8; w * h * 4];
    let mut fill = |x: usize, y: usize| px[(y * w + x) * 4 + 3] = 255;
    for y in 1..3 {
        for x in 0..2 {
            fill(x, y);
        }
    }
    for y in 0..4 {
        for x in 5..8 {
            fill(x, y);
        }
    }
    let f = auto_frames("s.png", &px, 8, 4);
    assert_eq!(f.len(), 2);
    // Both frames take the largest picture's 3 x 4 size so neither is stretched:
    // the small one is centred across and stands on its bottom edge.
    assert_eq!(f[0].uv, [0.0, 0.0, 3.0 / 8.0, 1.0]);
    assert_eq!(f[1].uv, [5.0 / 8.0, 0.0, 1.0, 1.0]);
    assert!(auto_frames("s.png", &[], 0, 0).is_empty());
}

#[test]
fn an_object_draws_its_clip_frame_with_flips() {
    let d = dir("draw");
    std::fs::write(d.join("hero.frames"), HERO).unwrap();
    let mut world = World::demo();
    let e = &mut world.entities[2];
    e.sprite = "hero.frames".into();
    let (img, uv) = e.sprite_draw(Some(&d), 0.0).unwrap();
    assert_eq!(img, d.join("hero.png"));
    assert_eq!(uv, [0.0, 0.0, 0.5, 1.0]);
    let (_, uv) = e.sprite_draw(Some(&d), 0.3).unwrap();
    assert_eq!(uv, [0.5, 0.0, 1.0, 1.0], "a quarter second in at 4 fps is frame 2");
    e.flip = [true, false];
    let (_, uv) = e.sprite_draw(Some(&d), 0.0).unwrap();
    assert_eq!(uv, [0.5, 0.0, 0.0, 1.0], "flip X swaps left and right");
    e.flip = [false, true];
    let (_, uv) = e.sprite_draw(Some(&d), 0.0).unwrap();
    assert_eq!(uv, [0.0, 1.0, 0.5, 0.0], "flip Y swaps top and bottom");
    e.flip = [false, false];
    e.play_animation("jump", 0.0);
    let (img, _) = e.sprite_draw(Some(&d), 0.0).unwrap();
    assert_eq!(img, d.join("jump.png"));
}

#[test]
fn clip_and_flips_survive_save_and_reopen() {
    let mut world = World::demo();
    let e = &mut world.entities[2];
    e.sprite = "hero.frames".into();
    e.animation = "jump".into();
    e.flip = [true, true];
    let text = world.to_text();
    let back = World::from_text(&text).unwrap();
    let e = &back.entities[2];
    assert_eq!((e.animation.as_str(), e.flip), ("jump", [true, true]));
    assert_eq!(back.to_text(), text);
}

const CLIP_GRAPH: &str = r#"
schema_version = 1
seal_graph = "clip"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "self" },
  { id = 3, kind = "play_clip", defaults = { clip = "jump" } },
]
links = ["1.then -> 3.in", "2.out -> 3.entity"]
"#;

#[test]
fn a_script_switches_its_sprite_to_another_clip() {
    let d = dir("script");
    std::fs::write(d.join("hero.frames"), HERO).unwrap();
    std::fs::write(d.join("logic/clip.graph"), CLIP_GRAPH).unwrap();
    let mut world = World::starter();
    for e in &mut world.entities {
        e.tag.clear();
        e.dynamic = false;
    }
    world.entities[0].sprite = "hero.frames".into();
    world.entities[0].script = "logic/clip.graph".into();
    world.project_dir = Some(d.clone());
    world.play();
    for _ in 0..5 {
        world.step(1.0 / 60.0);
    }
    assert_eq!(world.entities[0].animation, "jump", "log: {:?}", world.play_log);
    let (img, _) = world.entities[0].sprite_draw(Some(&d), world.play_time).unwrap();
    assert_eq!(img, d.join("jump.png"));
    world.stop();
    assert_eq!(world.entities[0].animation, "", "stop restores the clip set in the editor");
}

#[test]
fn old_animation_files_omit_the_new_frame_fields() {
    let lib = SpriteFrames::parse(HERO).unwrap();
    let text = lib.to_text();
    assert!(!text.contains("pivot="));
    assert!(!text.contains("border="));
    assert!(!text.contains("outline="));
    assert!(!text.contains("phys="));
    let frame = &lib.clip("idle").unwrap().frames[0];
    assert_eq!(frame.pivot, [0.5, 0.5]);
    assert_eq!(frame.border, [0.0; 4]);
    assert!(frame.outline.is_empty() && frame.physics.is_empty());
    let mut edited = lib.clone();
    let frame = &mut edited.clip_mut("idle").unwrap().frames[0];
    frame.pivot = [0.2, 0.8];
    frame.border = [1.0, 2.0, 3.0, 4.0];
    frame.outline = vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]];
    frame.physics = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
    let back = SpriteFrames::parse(&edited.to_text()).unwrap();
    assert_eq!(back, edited);
    assert!(
        SpriteFrames::parse("animation idle fps=8 loop=yes\nframe hero.png pivot=no\n").is_err()
    );
    assert!(SpriteFrames::parse("animation idle fps=8 loop=yes\nframe hero.png 0 0 1 1 foo=1\n")
        .is_err());
}
