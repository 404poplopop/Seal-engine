//! Pixel animation import: parse, damaged files, sprite animation output, rebuild.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use seal_editor_core::aseprite::{self, Direction};
use seal_editor_core::sprite_frames::SpriteFrames;
use seal_editor_core::{CommandArgs, EditorSession, Layout};
use seal_testkit::garbage::survives;

struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i16(&mut self, v: i16) {
        self.u16(v as u16);
    }
    fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }
    fn bytes(&mut self, b: &[u8]) {
        self.0.extend_from_slice(b);
    }
    fn str(&mut self, s: &str) {
        self.u16(s.len() as u16);
        self.bytes(s.as_bytes());
    }
}

fn chunk(kind: u16, data: &[u8]) -> Vec<u8> {
    let mut w = W(Vec::new());
    w.u32((data.len() + 6) as u32);
    w.u16(kind);
    w.bytes(data);
    w.0
}

fn layer(name: &str, opacity: u8, visible: bool, blend: u16, kind: u16) -> Vec<u8> {
    let mut d = W(Vec::new());
    d.u16(if visible { 1 } else { 0 });
    d.u16(kind);
    d.u16(0);
    d.u16(0);
    d.u16(0);
    d.u16(blend);
    d.u8(opacity);
    d.bytes(&[0; 3]);
    d.str(name);
    chunk(0x2004, &d.0)
}

fn cel_raw(layer: u16, x: i16, y: i16, w: u16, h: u16, pixels: &[u8]) -> Vec<u8> {
    let mut d = W(Vec::new());
    d.u16(layer);
    d.i16(x);
    d.i16(y);
    d.u8(255);
    d.u16(0);
    d.i16(0);
    d.bytes(&[0; 5]);
    d.u16(w);
    d.u16(h);
    d.bytes(pixels);
    chunk(0x2005, &d.0)
}

fn cel_compressed(layer: u16, x: i16, y: i16, w: u16, h: u16, pixels: &[u8]) -> Vec<u8> {
    let z = miniz_oxide::deflate::compress_to_vec_zlib(pixels, 6);
    let mut d = W(Vec::new());
    d.u16(layer);
    d.i16(x);
    d.i16(y);
    d.u8(255);
    d.u16(2);
    d.i16(0);
    d.bytes(&[0; 5]);
    d.u16(w);
    d.u16(h);
    d.bytes(&z);
    chunk(0x2005, &d.0)
}

fn cel_link(layer: u16, x: i16, y: i16, frame: u16) -> Vec<u8> {
    let mut d = W(Vec::new());
    d.u16(layer);
    d.i16(x);
    d.i16(y);
    d.u8(255);
    d.u16(1);
    d.i16(0);
    d.bytes(&[0; 5]);
    d.u16(frame);
    chunk(0x2005, &d.0)
}

fn palette(entries: &[(u8, [u8; 4])]) -> Vec<u8> {
    let first = entries.iter().map(|e| e.0).min().unwrap_or(0);
    let last = entries.iter().map(|e| e.0).max().unwrap_or(0);
    let mut d = W(Vec::new());
    d.u32(entries.len() as u32);
    d.u32(u32::from(first));
    d.u32(u32::from(last));
    d.bytes(&[0; 8]);
    for i in first..=last {
        let color = entries.iter().find(|e| e.0 == i).map(|e| e.1).unwrap_or([0, 0, 0, 255]);
        d.u16(0);
        d.bytes(&color);
    }
    chunk(0x2019, &d.0)
}

fn tags(items: &[(&str, u16, u16, u8, u16)]) -> Vec<u8> {
    let mut d = W(Vec::new());
    d.u16(items.len() as u16);
    d.bytes(&[0; 8]);
    for (name, from, to, dir, repeat) in items {
        d.u16(*from);
        d.u16(*to);
        d.u8(*dir);
        d.u16(*repeat);
        d.bytes(&[0; 6]);
        d.bytes(&[0; 4]);
        d.str(name);
    }
    chunk(0x2018, &d.0)
}

fn slice(name: &str, frame: u32, x: i32, y: i32, w: u32, h: u32) -> Vec<u8> {
    let mut d = W(Vec::new());
    d.u32(1);
    d.u32(0);
    d.u32(0);
    d.str(name);
    d.u32(frame);
    d.i32(x);
    d.i32(y);
    d.u32(w);
    d.u32(h);
    chunk(0x2022, &d.0)
}

struct Doc {
    w: u16,
    h: u16,
    depth: u16,
    transparent: u8,
    speed: u16,
    opacity_valid: bool,
    extra: Vec<Vec<u8>>,
    frames: Vec<(u16, Vec<Vec<u8>>)>,
}

fn build(doc: &Doc) -> Vec<u8> {
    let mut body = Vec::new();
    for (i, (duration, cels)) in doc.frames.iter().enumerate() {
        let mut chunks = Vec::new();
        if i == 0 {
            chunks.extend(doc.extra.iter().cloned());
        }
        chunks.extend(cels.iter().cloned());
        let payload: usize = chunks.iter().map(Vec::len).sum();
        let mut frame = W(Vec::new());
        frame.u32((16 + payload) as u32);
        frame.u16(0xF1FA);
        frame.u16(0xFFFF);
        frame.u16(*duration);
        frame.u16(0);
        frame.u32(chunks.len() as u32);
        for chunk in &chunks {
            frame.bytes(chunk);
        }
        body.extend(frame.0);
    }
    let mut header = W(Vec::new());
    header.u32((128 + body.len()) as u32);
    header.u16(0xA5E0);
    header.u16(doc.frames.len() as u16);
    header.u16(doc.w);
    header.u16(doc.h);
    header.u16(doc.depth);
    header.u32(if doc.opacity_valid { 1 } else { 0 });
    header.u16(doc.speed);
    header.bytes(&[0; 8]);
    header.u8(doc.transparent);
    header.bytes(&[0; 3]);
    header.u16(0);
    header.bytes(&[0; 2]);
    header.bytes(&[0; 8]);
    header.bytes(&[0; 84]);
    assert_eq!(header.0.len(), 128);
    header.bytes(&body);
    header.0
}

fn one_rgba(w: u16, h: u16, pixels: &[u8]) -> Vec<u8> {
    build(&Doc {
        w,
        h,
        depth: 32,
        transparent: 0,
        speed: 0,
        opacity_valid: true,
        extra: vec![layer("paint", 255, true, 0, 0)],
        frames: vec![(100, vec![cel_raw(0, 0, 0, w, h, pixels)])],
    })
}

fn decode_png(path: &Path) -> (Vec<u8>, u32, u32) {
    let file = std::io::BufReader::new(fs::File::open(path).unwrap());
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    (buf, info.width, info.height)
}

#[test]
fn a_raw_frame_keeps_its_pixels_and_cel_position() {
    let bytes = one_rgba(2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]);
    let src = aseprite::parse(&bytes).unwrap();
    assert_eq!(src.frames[0], vec![255, 0, 0, 255, 0, 0, 255, 255]);
    let shifted = build(&Doc {
        w: 2,
        h: 1,
        depth: 32,
        transparent: 0,
        speed: 0,
        opacity_valid: true,
        extra: vec![layer("paint", 255, true, 0, 0)],
        frames: vec![(100, vec![cel_raw(0, 1, 0, 1, 1, &[255, 0, 0, 255])])],
    });
    let src = aseprite::parse(&shifted).unwrap();
    assert_eq!(src.frames[0], vec![0, 0, 0, 0, 255, 0, 0, 255]);
}

#[test]
fn a_compressed_cel_matches_the_raw_pixels() {
    let pixels = [10, 20, 30, 255, 1, 2, 3, 128];
    let bytes = build(&Doc {
        w: 2,
        h: 1,
        depth: 32,
        transparent: 0,
        speed: 0,
        opacity_valid: true,
        extra: vec![layer("paint", 255, true, 0, 0)],
        frames: vec![(100, vec![cel_compressed(0, 0, 0, 2, 1, &pixels)])],
    });
    assert_eq!(aseprite::parse(&bytes).unwrap().frames[0], pixels);
}

#[test]
fn indexed_pixels_use_the_palette_and_the_transparent_index() {
    let bytes = build(&Doc {
        w: 2,
        h: 1,
        depth: 8,
        transparent: 0,
        speed: 0,
        opacity_valid: true,
        extra: vec![
            layer("paint", 255, true, 0, 0),
            palette(&[(0, [0, 0, 0, 255]), (1, [0, 255, 0, 255])]),
        ],
        frames: vec![(100, vec![cel_raw(0, 0, 0, 2, 1, &[0, 1])])],
    });
    let src = aseprite::parse(&bytes).unwrap();
    assert_eq!(src.frames[0], vec![0, 0, 0, 0, 0, 255, 0, 255]);
}

#[test]
fn grayscale_pixels_expand_to_colour() {
    let bytes = build(&Doc {
        w: 1,
        h: 1,
        depth: 16,
        transparent: 0,
        speed: 0,
        opacity_valid: true,
        extra: vec![layer("paint", 255, true, 0, 0)],
        frames: vec![(50, vec![cel_raw(0, 0, 0, 1, 1, &[40, 128])])],
    });
    assert_eq!(aseprite::parse(&bytes).unwrap().frames[0], vec![40, 40, 40, 128]);
    assert_eq!(aseprite::parse(&bytes).unwrap().durations_ms, vec![50]);
}

#[test]
fn two_layers_composite_opacity_and_a_hidden_layer_is_skipped() {
    let red = cel_raw(0, 0, 0, 1, 1, &[255, 0, 0, 255]);
    let blue = cel_raw(1, 0, 0, 1, 1, &[0, 0, 255, 255]);
    let bytes = build(&Doc {
        w: 1,
        h: 1,
        depth: 32,
        transparent: 0,
        speed: 0,
        opacity_valid: true,
        extra: vec![layer("back", 255, true, 0, 0), layer("front", 128, true, 1, 0)],
        frames: vec![(100, vec![red.clone(), blue])],
    });
    let src = aseprite::parse(&bytes).unwrap();
    assert_eq!(src.frames[0], vec![127, 0, 128, 255]);
    assert!(src.notes.iter().any(|n| n.contains("blend")));
    let hidden = build(&Doc {
        w: 1,
        h: 1,
        depth: 32,
        transparent: 0,
        speed: 0,
        opacity_valid: true,
        extra: vec![
            layer("back", 255, true, 0, 0),
            layer("front", 255, false, 0, 0),
            layer("tiles", 255, true, 0, 2),
        ],
        frames: vec![(100, vec![red, cel_raw(1, 0, 0, 1, 1, &[0, 0, 255, 255])])],
    });
    let src = aseprite::parse(&hidden).unwrap();
    assert_eq!(src.frames[0], vec![255, 0, 0, 255]);
    assert!(src.notes.iter().any(|n| n.contains("tile layer")));
}

#[test]
fn a_linked_cel_copies_the_other_frame() {
    let bytes = build(&Doc {
        w: 2,
        h: 1,
        depth: 32,
        transparent: 0,
        speed: 0,
        opacity_valid: true,
        extra: vec![layer("paint", 255, true, 0, 0)],
        frames: vec![
            (100, vec![cel_raw(0, 0, 0, 1, 1, &[255, 0, 0, 255])]),
            (100, vec![cel_link(0, 1, 0, 0)]),
        ],
    });
    let src = aseprite::parse(&bytes).unwrap();
    assert_eq!(src.frames[0], vec![255, 0, 0, 255, 0, 0, 0, 0]);
    assert_eq!(src.frames[1], vec![0, 0, 0, 0, 255, 0, 0, 255]);
}

#[test]
fn tags_become_clips_and_slices_stay_out_of_the_animation() {
    let px = [255, 0, 0, 255];
    let cel = || cel_raw(0, 0, 0, 1, 1, &px);
    let src = aseprite::parse(&build(&Doc {
        w: 1,
        h: 1,
        depth: 32,
        transparent: 0,
        speed: 0,
        opacity_valid: true,
        extra: vec![
            layer("paint", 255, true, 0, 0),
            tags(&[("run", 0, 2, 0, 0), ("back", 0, 1, 1, 1), ("wave", 0, 2, 2, 0)]),
            slice("head", 0, 0, 0, 1, 1),
        ],
        frames: vec![(100, vec![cel()]), (100, vec![cel()]), (200, vec![cel()])],
    }))
    .unwrap();
    assert_eq!(src.slices[0].name, "head");
    assert_eq!(src.tags[2].direction, Direction::PingPong);
    let dir = std::env::temp_dir().join(format!("seal-pixel-tags-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("hero.aseprite"),
        build(&Doc {
            w: 1,
            h: 1,
            depth: 32,
            transparent: 0,
            speed: 0,
            opacity_valid: true,
            extra: vec![
                layer("paint", 255, true, 0, 0),
                tags(&[("run", 0, 2, 0, 0), ("back", 0, 1, 1, 1), ("wave", 0, 2, 2, 0)]),
                slice("head", 0, 0, 0, 1, 1),
            ],
            frames: vec![(100, vec![cel()]), (100, vec![cel()]), (200, vec![cel()])],
        }),
    )
    .unwrap();
    let report = aseprite::import_path(&dir.join("hero.aseprite")).unwrap();
    assert!(report.notes.iter().any(|n| n.contains("named cells")));
    let text = fs::read_to_string(dir.join("hero.frames")).unwrap();
    assert!(text.contains("# generated-from hero.aseprite"));
    assert!(text.contains("# slices not written"));
    let lib = SpriteFrames::parse(&text).unwrap();
    let run = lib.clip("run").unwrap();
    assert!(run.looping);
    assert_eq!(run.fps, 10.0);
    assert_eq!(run.frames.len(), 4, "the 200 ms frame is held twice at 10 fps");
    let back = lib.clip("back").unwrap();
    assert!(!back.looping);
    assert_eq!(back.frames.len(), 2);
    let wave = lib.clip("wave").unwrap();
    assert_eq!(wave.frames.len(), 5, "forward, back, and the 200 ms frame held twice");
    assert!(!text.lines().any(|line| line.starts_with("frame") && line.contains("head")));
    let (png, w, h) = decode_png(&dir.join("hero.png"));
    assert_eq!((w, h), (3, 1));
    assert_eq!(&png[0..4], &[255, 0, 0, 255]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn uneven_durations_use_one_average_speed() {
    let cel = cel_raw(0, 0, 0, 1, 1, &[1, 2, 3, 255]);
    let frames = vec![(100, vec![cel.clone()]), (101, vec![cel.clone()]), (100, vec![cel])];
    let dir = std::env::temp_dir().join(format!("seal-pixel-avg-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("odd.ase"),
        build(&Doc {
            w: 1,
            h: 1,
            depth: 32,
            transparent: 0,
            speed: 0,
            opacity_valid: true,
            extra: vec![layer("paint", 255, true, 0, 0)],
            frames,
        }),
    )
    .unwrap();
    let report = aseprite::import_path(&dir.join("odd.ase")).unwrap();
    assert!(report.notes.iter().any(|n| n.contains("average")));
    let lib = SpriteFrames::parse(&fs::read_to_string(dir.join("odd.frames")).unwrap()).unwrap();
    assert_eq!(lib.clips[0].frames.len(), 3);
    assert!((lib.clips[0].fps - 9.967).abs() < 0.001);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn damaged_files_are_errors_and_never_panic() {
    let good = one_rgba(1, 1, &[9, 8, 7, 255]);
    assert!(aseprite::parse(&[]).is_err());
    assert!(aseprite::parse(&good[..20]).is_err());
    let mut magic = good.clone();
    magic[4] = 0;
    magic[5] = 0;
    assert!(aseprite::parse(&magic).unwrap_err().contains("not a pixel animation"));
    let mut huge = good.clone();
    huge[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(aseprite::parse(&huge).is_err());
    let absurd = build(&Doc {
        w: 65535,
        h: 65535,
        depth: 32,
        transparent: 0,
        speed: 0,
        opacity_valid: true,
        extra: Vec::new(),
        frames: vec![(100, Vec::new())],
    });
    assert!(aseprite::parse(&absurd).unwrap_err().contains("not usable"));
    let bomb = {
        // A 1x1 cel whose compressed stream expands to 64 KiB.
        let mut d = W(Vec::new());
        d.u16(0);
        d.i16(0);
        d.i16(0);
        d.u8(255);
        d.u16(2);
        d.i16(0);
        d.bytes(&[0; 5]);
        d.u16(1);
        d.u16(1);
        d.bytes(&miniz_oxide::deflate::compress_to_vec_zlib(&vec![0u8; 64 * 1024], 1));
        build(&Doc {
            w: 1,
            h: 1,
            depth: 32,
            transparent: 0,
            speed: 0,
            opacity_valid: true,
            extra: vec![layer("paint", 255, true, 0, 0)],
            frames: vec![(100, vec![chunk(0x2005, &d.0)])],
        })
    };
    assert!(aseprite::parse(&bomb).is_err());
    let mut bad_chunk = good.clone();
    if bad_chunk.len() >= 148 {
        bad_chunk[144..148].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(aseprite::parse(&bad_chunk).is_err());
    }
    survives("pixel-animation", &good, |bytes| {
        let _ = aseprite::parse(bytes);
    });
}

#[test]
fn the_editor_imports_and_rebuilds_when_the_source_changes() {
    let (project, mut session) = project();
    let bytes = one_rgba(1, 1, &[255, 0, 0, 255]);
    fs::write(project.join("hero.aseprite"), &bytes).unwrap();
    session.dispatch("content.open_file", &CommandArgs::new(&[("path", "hero.aseprite")])).unwrap();
    assert_eq!(
        session.draft(seal_editor_core::asset_editor::ASSET_DRAFT).as_deref(),
        Some("hero.aseprite")
    );
    session
        .dispatch("asset.sprite_source_import", &CommandArgs::new(&[("path", "hero.aseprite")]))
        .unwrap();
    assert!(project.join("hero.png").is_file());
    let lib =
        SpriteFrames::parse(&fs::read_to_string(project.join("hero.frames")).unwrap()).unwrap();
    assert_eq!(lib.clips[0].name, "all");
    assert_eq!(lib.clips[0].frames.len(), 1);
    session.dispatch("content.open_file", &CommandArgs::new(&[("path", "hero.frames")])).unwrap();
    session
        .dispatch("asset.frames_clip_set", &CommandArgs::new(&[("clip", "all"), ("fps", "3")]))
        .unwrap();
    let kept = fs::read_to_string(project.join("hero.frames")).unwrap();
    assert!(kept.contains("# generated-from hero.aseprite"));
    assert!(kept.contains("fps=3"));
    fs::write(project.join("hero.aseprite"), one_rgba(1, 1, &[0, 255, 0, 255])).unwrap();
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    set_mtime(&project.join("hero.frames"), old);
    set_mtime(&project.join("hero.aseprite"), old + Duration::from_secs(30));
    session.dispatch("content.open_file", &CommandArgs::new(&[("path", "hero.aseprite")])).unwrap();
    session.tick(0.3);
    let lib =
        SpriteFrames::parse(&fs::read_to_string(project.join("hero.frames")).unwrap()).unwrap();
    assert_eq!(lib.clips[0].fps, 10.0, "a newer source rebuilds the animation");
    let (png, _, _) = decode_png(&project.join("hero.png"));
    assert_eq!(&png[0..4], &[0, 255, 0, 255]);
    let _ = fs::remove_dir_all(project.parent().unwrap());
}

fn set_mtime(path: &Path, time: SystemTime) {
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(time).unwrap();
}

fn project() -> (PathBuf, EditorSession) {
    let dir = std::env::temp_dir().join(format!("seal-pixel-editor-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(&project).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 17);
    fs::write(
        project.join("project.seal"),
        format!(
            "schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
        ),
    )
    .unwrap();
    fs::write(project.join("scene.seal"), "scene\n").unwrap();
    let path = project.to_string_lossy().to_string();
    let mut session = EditorSession::new(Layout::default());
    session.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    (project, session)
}
