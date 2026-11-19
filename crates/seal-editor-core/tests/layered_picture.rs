//! Layered picture import: 8-bit RGB, raw and run-length channels, sprites, reimport.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use seal_editor_core::layered_picture::{self, LayerLine};
use seal_editor_core::{CommandArgs, EditorSession, Layout, World};
use seal_testkit::garbage::survives;

struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
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
}

struct Chan {
    id: i16,
    data: Vec<u8>,
}

struct Lyr {
    name: String,
    top: i32,
    left: i32,
    bottom: i32,
    right: i32,
    flags: u8,
    blend: [u8; 4],
    section: u32,
    id: u32,
    channels: Vec<Chan>,
}

fn raw_plane(id: i16, samples: &[u8]) -> Chan {
    let mut data = Vec::new();
    data.extend_from_slice(&0u16.to_be_bytes());
    data.extend_from_slice(samples);
    Chan { id, data }
}

fn pack_row(row: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < row.len() {
        let byte = row[i];
        let mut j = i + 1;
        while j < row.len() && row[j] == byte && j - i < 128 {
            j += 1;
        }
        let run = j - i;
        if run >= 2 {
            out.push((257 - run) as u8);
            out.push(byte);
            i = j;
        } else {
            let start = i;
            i += 1;
            while i < row.len() && i - start < 128 {
                if i + 1 < row.len() && row[i] == row[i + 1] {
                    break;
                }
                i += 1;
            }
            let lit = &row[start..i];
            out.push((lit.len() - 1) as u8);
            out.extend_from_slice(lit);
        }
    }
    out
}

fn rle_plane(id: i16, rows: &[&[u8]]) -> Chan {
    let mut counts = Vec::new();
    let mut packed = Vec::new();
    for row in rows {
        let block = pack_row(row);
        counts.extend_from_slice(&(block.len() as u16).to_be_bytes());
        packed.extend(block);
    }
    let mut data = Vec::new();
    data.extend_from_slice(&1u16.to_be_bytes());
    data.extend(counts);
    data.extend(packed);
    Chan { id, data }
}

fn pascal4(out: &mut W, name: &str) {
    let bytes = name.as_bytes();
    out.u8(bytes.len() as u8);
    out.bytes(bytes);
    let pad = (4 - ((1 + bytes.len()) % 4)) % 4;
    for _ in 0..pad {
        out.u8(0);
    }
}

fn extra(name: &str, id: u32, section: u32) -> Vec<u8> {
    let mut out = W(Vec::new());
    out.u32(0);
    out.u32(0);
    pascal4(&mut out, name);
    out.bytes(b"8BIM");
    out.bytes(b"lyid");
    out.u32(4);
    out.u32(id);
    out.bytes(b"8BIM");
    out.bytes(b"lsct");
    out.u32(4);
    out.u32(section);
    out.0
}

fn push_record(out: &mut W, layer: &Lyr) {
    out.i32(layer.top);
    out.i32(layer.left);
    out.i32(layer.bottom);
    out.i32(layer.right);
    out.u16(layer.channels.len() as u16);
    for channel in &layer.channels {
        out.i16(channel.id);
        out.u32(channel.data.len() as u32);
    }
    out.bytes(b"8BIM");
    out.bytes(&layer.blend);
    out.u8(255);
    out.u8(0);
    out.u8(layer.flags);
    out.u8(0);
    let extra = extra(&layer.name, layer.id, layer.section);
    out.u32(extra.len() as u32);
    out.bytes(&extra);
}

fn marker(name: &str, id: u32, section: u32) -> Lyr {
    Lyr {
        name: name.into(),
        top: 0,
        left: 0,
        bottom: 0,
        right: 0,
        flags: 2,
        blend: *b"norm",
        section,
        id,
        channels: Vec::new(),
    }
}

/// 6×4 picture. Background is raw red. Coin is run-length green (or blue).
/// Dot is a run-length row with a transparent pixel on each side. Hidden is raw
/// blue and must not appear. Coin, Dot and Hidden sit in the group Items.
fn sample(coin_g: u8, coin_b: u8) -> Vec<u8> {
    let background = Lyr {
        name: "Background".into(),
        top: 0,
        left: 0,
        bottom: 4,
        right: 6,
        flags: 0,
        blend: *b"norm",
        section: 0,
        id: 10,
        channels: vec![
            raw_plane(0, &vec![255; 24]),
            raw_plane(1, &vec![0; 24]),
            raw_plane(2, &vec![0; 24]),
            raw_plane(-1, &vec![255; 24]),
        ],
    };
    let row_g = [coin_g, coin_g];
    let row_b = [coin_b, coin_b];
    let row_z = [0u8, 0];
    let row_a = [255u8, 255];
    let coin = Lyr {
        name: "Coin".into(),
        top: 0,
        left: 4,
        bottom: 2,
        right: 6,
        flags: 0,
        blend: *b"norm",
        section: 0,
        id: 12,
        channels: vec![
            rle_plane(0, &[&row_z, &row_z]),
            rle_plane(1, &[&row_g, &row_g]),
            rle_plane(2, &[&row_b, &row_b]),
            rle_plane(-1, &[&row_a, &row_a]),
        ],
    };
    let dot = Lyr {
        name: "Dot".into(),
        top: 3,
        left: 1,
        bottom: 4,
        right: 4,
        flags: 0,
        blend: *b"norm",
        section: 0,
        id: 13,
        channels: vec![
            rle_plane(0, &[&[255, 255, 255][..]]),
            rle_plane(1, &[&[255, 255, 255][..]]),
            rle_plane(2, &[&[255, 255, 255][..]]),
            rle_plane(-1, &[&[0, 255, 0][..]]),
        ],
    };
    let hidden = Lyr {
        name: "Hidden".into(),
        top: 0,
        left: 0,
        bottom: 1,
        right: 1,
        flags: 2,
        blend: *b"norm",
        section: 0,
        id: 14,
        channels: vec![
            raw_plane(0, &[0]),
            raw_plane(1, &[0]),
            raw_plane(2, &[255]),
            raw_plane(-1, &[255]),
        ],
    };
    let mut folder = marker("Items", 15, 1);
    folder.flags = 0;
    folder.blend = *b"pass";
    let layers = [background, marker("bound", 11, 3), coin, dot, hidden, folder];
    let mut records = W(Vec::new());
    let mut channels = W(Vec::new());
    for layer in &layers {
        push_record(&mut records, layer);
        for channel in &layer.channels {
            channels.bytes(&channel.data);
        }
    }
    let mut info = W(Vec::new());
    info.u16(layers.len() as u16);
    info.bytes(&records.0);
    info.bytes(&channels.0);
    let mut section = W(Vec::new());
    section.u32(info.0.len() as u32);
    section.bytes(&info.0);
    let mut file = W(Vec::new());
    file.bytes(b"8BPS");
    file.u16(1);
    file.bytes(&[0; 6]);
    file.u16(4);
    file.u32(4);
    file.u32(6);
    file.u16(8);
    file.u16(3);
    file.u32(0);
    file.u32(0);
    file.u32(section.0.len() as u32);
    file.bytes(&section.0);
    file.u16(0);
    file.bytes(&vec![0u8; 4 * 6 * 4]);
    file.0
}

fn header(mode: u16, depth: u16) -> Vec<u8> {
    let mut file = W(Vec::new());
    file.bytes(b"8BPS");
    file.u16(1);
    file.bytes(&[0; 6]);
    file.u16(4);
    file.u32(1);
    file.u32(1);
    file.u16(depth);
    file.u16(mode);
    file.u32(0);
    file.u32(0);
    file.u32(0);
    file.u16(0);
    file.bytes(&[255, 0, 0, 255]);
    file.0
}

fn zip_channel() -> Vec<u8> {
    let mut file = W(Vec::new());
    file.bytes(b"8BPS");
    file.u16(1);
    file.bytes(&[0; 6]);
    file.u16(3);
    file.u32(1);
    file.u32(1);
    file.u16(8);
    file.u16(3);
    file.u32(0);
    file.u32(0);
    let layer = Lyr {
        name: "Only".into(),
        top: 0,
        left: 0,
        bottom: 1,
        right: 1,
        flags: 0,
        blend: *b"norm",
        section: 0,
        id: 1,
        channels: vec![
            Chan { id: 0, data: vec![0, 2, 1, 2, 3] },
            Chan { id: 1, data: vec![0, 2, 1, 2, 3] },
            Chan { id: 2, data: vec![0, 2, 1, 2, 3] },
        ],
    };
    let mut records = W(Vec::new());
    let mut channels = W(Vec::new());
    push_record(&mut records, &layer);
    for channel in &layer.channels {
        channels.bytes(&channel.data);
    }
    let mut info = W(Vec::new());
    info.u16(1);
    info.bytes(&records.0);
    info.bytes(&channels.0);
    let mut section = W(Vec::new());
    section.u32(info.0.len() as u32);
    section.bytes(&info.0);
    file.u32(section.0.len() as u32);
    file.bytes(&section.0);
    file.u16(0);
    file.bytes(&[0, 0, 0]);
    file.0
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

fn pixel(buf: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

fn near(got: f32, want: f32) {
    assert!((got - want).abs() < 0.02, "{got} != {want}");
}

fn open_project(tag: &str) -> (PathBuf, EditorSession) {
    let dir = std::env::temp_dir().join(format!("seal-layered-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(&project).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 12);
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

fn ids_of(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("id = ")?;
            Some(rest.trim_matches('"').to_string())
        })
        .collect()
}

fn sprite_lines(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("sprite = ")?;
            Some(rest.trim_matches('"').to_string())
        })
        .collect()
}

#[test]
fn a_small_file_flattens_visible_layers_and_lists_groups() {
    let bytes = sample(255, 0);
    assert!(bytes.len() < 20 * 1024, "{}", bytes.len());
    let src = layered_picture::parse(&bytes).unwrap();
    assert_eq!((src.width, src.height), (6, 4));
    assert_eq!(pixel(&src.rgba, 6, 0, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(&src.rgba, 6, 3, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(&src.rgba, 6, 4, 0), [0, 255, 0, 255]);
    assert_eq!(pixel(&src.rgba, 6, 5, 1), [0, 255, 0, 255]);
    assert_eq!(pixel(&src.rgba, 6, 2, 3), [255, 255, 255, 255]);
    assert_eq!(pixel(&src.rgba, 6, 1, 3), [255, 0, 0, 255]);
    let names: Vec<_> = src.layers.iter().map(|layer| layer.name.as_str()).collect();
    assert_eq!(names, ["Items", "Hidden", "Dot", "Coin", "Background"]);
    assert!(src.layers.iter().any(|layer| layer.name == "Hidden" && !layer.visible));
    assert!(src
        .layers
        .iter()
        .any(|layer| layer
            == &LayerLine { name: "Items".into(), visible: true, group: true, depth: 0 }));
    assert_eq!(src.layers.iter().find(|layer| layer.name == "Coin").unwrap().depth, 1);
}

#[test]
fn layers_as_sprites_place_trimmed_pictures_and_keep_ids_on_reimport() {
    let (project, mut session) = open_project("import");
    let psd = project.join("hero.psd");
    fs::write(&psd, sample(255, 0)).unwrap();
    session
        .dispatch(
            "asset.layered_picture_option",
            &CommandArgs::new(&[("path", "hero.psd"), ("value", "yes")]),
        )
        .unwrap();
    assert!(layered_picture::layers_as_sprites(&psd));
    session
        .dispatch("asset.layered_picture_import", &CommandArgs::new(&[("path", "hero.psd")]))
        .unwrap();
    let (flat, fw, fh) = decode_png(&project.join("hero.png"));
    assert_eq!((fw, fh), (6, 4));
    assert_eq!(pixel(&flat, 6, 4, 0), [0, 255, 0, 255]);
    assert_eq!(pixel(&flat, 6, 0, 0), [255, 0, 0, 255]);
    let (bg, bw, bh) = decode_png(&project.join("hero_layers/Background.png"));
    assert_eq!((bw, bh), (6, 4));
    assert!(bg.chunks(4).all(|p| p == [255, 0, 0, 255]));
    let (coin, cw, ch) = decode_png(&project.join("hero_layers/Items/Coin.png"));
    assert_eq!((cw, ch), (2, 2));
    assert!(coin.chunks(4).all(|p| p == [0, 255, 0, 255]));
    let (dot, dw, dh) = decode_png(&project.join("hero_layers/Items/Dot.png"));
    assert_eq!((dw, dh, &dot[..]), (1, 1, &[255, 255, 255, 255][..]));
    assert!(!project.join("hero_layers").join("Hidden.png").exists());
    let text = fs::read_to_string(project.join("hero.prefab")).unwrap();
    assert!(text.contains("generated_from = \"hero.psd\""));
    let before_ids = ids_of(&text);
    let before_sprites = sprite_lines(&text);
    assert!(before_ids.contains(&"lyrroot".to_string()));
    assert!(before_ids.contains(&"lyr12".to_string()));
    assert!(before_sprites.iter().any(|path| path.ends_with("Items/Coin.png")));
    let mut world = World::starter();
    world.add_prefab_text(&text, None).unwrap();
    let coin_e = world.entities.iter().find(|e| e.name == "Coin").unwrap();
    let dot_e = world.entities.iter().find(|e| e.name == "Dot").unwrap();
    let bg_e = world.entities.iter().find(|e| e.name == "Background").unwrap();
    let items = world.entities.iter().find(|e| e.name == "Items").unwrap();
    near(coin_e.translation[0], 5.0);
    near(coin_e.translation[1], 3.0);
    near(coin_e.scale[0], 2.0);
    near(coin_e.scale[1], 2.0);
    near(dot_e.translation[0], 2.5);
    near(dot_e.translation[1], 0.5);
    near(dot_e.scale[0], 1.0);
    near(bg_e.translation[0], 3.0);
    near(bg_e.translation[1], 2.0);
    near(bg_e.scale[0], 6.0);
    near(bg_e.scale[1], 4.0);
    assert!(coin_e.order > bg_e.order);
    assert!(dot_e.order > coin_e.order);
    assert_eq!(coin_e.parent.as_deref(), Some(items.id.as_str()));
    assert_eq!(dot_e.parent.as_deref(), Some(items.id.as_str()));
    assert!(world.entities.iter().all(|e| e.name != "Hidden"));
    assert!(coin_e.sprite.ends_with("hero_layers/Items/Coin.png"));

    fs::write(&psd, sample(0, 255)).unwrap();
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    for path in [
        project.join("hero.png"),
        project.join("hero.prefab"),
        project.join("hero_layers/Background.png"),
        project.join("hero_layers/Items/Coin.png"),
        project.join("hero_layers/Items/Dot.png"),
    ] {
        set_mtime(&path, old);
    }
    set_mtime(&psd, old + Duration::from_secs(30));
    session.dispatch("content.open_file", &CommandArgs::new(&[("path", "hero.psd")])).unwrap();
    session.tick(0.2);
    let text = fs::read_to_string(project.join("hero.prefab")).unwrap();
    assert_eq!(ids_of(&text), before_ids);
    assert_eq!(sprite_lines(&text), before_sprites);
    let (coin, _, _) = decode_png(&project.join("hero_layers/Items/Coin.png"));
    assert!(coin.chunks(4).all(|p| p == [0, 0, 255, 255]));
    let (flat, _, _) = decode_png(&project.join("hero.png"));
    assert_eq!(pixel(&flat, 6, 4, 0), [0, 0, 255, 255]);
    let (bg, _, _) = decode_png(&project.join("hero_layers/Background.png"));
    assert!(bg.chunks(4).all(|p| p == [255, 0, 0, 255]));
    let _ = fs::remove_dir_all(project.parent().unwrap());
}

#[test]
fn flatten_only_writes_the_picture_and_will_not_replace_a_foreign_file() {
    let (root, mut session) = open_project("flat");
    fs::write(root.join("hero.psd"), sample(255, 0)).unwrap();
    session
        .dispatch("asset.layered_picture_import", &CommandArgs::new(&[("path", "hero.psd")]))
        .unwrap();
    assert!(root.join("hero.png").is_file());
    assert!(!root.join("hero.prefab").exists());
    assert!(!layered_picture::layers_as_sprites(&root.join("hero.psd")));
    let (flat, _, _) = decode_png(&root.join("hero.png"));
    assert_eq!(pixel(&flat, 6, 5, 1), [0, 255, 0, 255]);

    let (other, mut session) = open_project("clash");
    fs::write(other.join("hero.psd"), sample(255, 0)).unwrap();
    fs::write(other.join("hero.png"), b"not-ours").unwrap();
    let err = session
        .dispatch("asset.layered_picture_import", &CommandArgs::new(&[("path", "hero.psd")]))
        .unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
    assert_eq!(fs::read(other.join("hero.png")).unwrap(), b"not-ours");
    let _ = fs::remove_dir_all(root.parent().unwrap());
    let _ = fs::remove_dir_all(other.parent().unwrap());
}

#[test]
fn damaged_and_unsupported_files_fail_clearly() {
    let good = sample(255, 0);
    assert!(layered_picture::parse(&[]).unwrap_err().contains("not a layered picture"));
    assert!(layered_picture::parse(b"nope").unwrap_err().contains("not a layered picture"));
    assert!(layered_picture::parse(&good[..20]).is_err());
    let cmyk = layered_picture::parse(&header(4, 8)).unwrap_err();
    assert!(cmyk.contains("8-bit RGB"), "{cmyk}");
    assert!(cmyk.contains("CMYK"), "{cmyk}");
    let deep = layered_picture::parse(&header(3, 16)).unwrap_err();
    assert!(deep.contains("16-bit"), "{deep}");
    let zip = layered_picture::parse(&zip_channel()).unwrap_err();
    // ZIP channels are read now; this one holds bytes that are not a zlib stream.
    assert!(zip.contains("ZIP"), "{zip}");
    for n in 0..good.len() {
        let _ = layered_picture::parse(&good[..n]);
    }
    survives("layered-picture", &good, |bytes| {
        let _ = layered_picture::parse(bytes);
    });
}

fn set_mtime(path: &Path, time: SystemTime) {
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(time).unwrap();
}
