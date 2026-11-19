//! Sprite atlas packing: tight bounds, quarter turns, page limits, and scale.

use std::fs;
use std::path::{Path, PathBuf};

use seal_editor_core::atlas::{
    build, build_with, pack, pack_turning, page_texel, page_uv, picture_corner_uv, Atlas,
    AtlasOptions, Placed,
};
use seal_editor_core::export_preset::{parse_book, to_text, ExportBook, ExportPreset};

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "seal-atlas-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_png(path: &Path, w: u32, h: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            rgba.extend_from_slice(&pixel(x, y));
        }
    }
    let file = std::io::BufWriter::new(fs::File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
}

fn read_png(path: &Path) -> (Vec<u8>, u32, u32) {
    let decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    (buf, info.width, info.height)
}

fn page_area(atlas: &Atlas) -> u32 {
    atlas.pages.iter().map(|(_, w, h, _)| w * h).sum()
}

fn options(tight: bool, rotate: bool, page: u32, scale: u32) -> AtlasOptions {
    AtlasOptions { tight, rotate, page_size: page, scale_percent: scale }
}

#[test]
fn default_options_match_the_existing_packer() {
    let root = scratch("default");
    let plain = root.join("plain");
    let same = root.join("same");
    for dir in [&plain, &same] {
        write_png(&dir.join("sprites/red.png"), 16, 8, |_, _| [255, 0, 0, 255]);
        write_png(&dir.join("sprites/blue.png"), 4, 12, |_, _| [0, 0, 255, 255]);
    }
    assert_eq!(build(&plain).unwrap(), 2);
    assert_eq!(build_with(&same, &AtlasOptions::default()).unwrap(), 2);
    let left = fs::read(plain.join("atlas.txt")).unwrap();
    let right = fs::read(same.join("atlas.txt")).unwrap();
    assert_eq!(left, right);
    assert!(std::str::from_utf8(&left).unwrap().starts_with("seal atlas 1\n"));
    let atlas = Atlas::load(&plain).unwrap();
    let (spots, pages) = pack(&[(4, 12), (16, 8)], seal_editor_core::atlas::PAGE_SIZE);
    // Directory order is blue then red: heights 12 then 8.
    let mut found = atlas.sprites.clone();
    found.sort_by(|a, b| a.path.cmp(&b.path));
    let order = ["sprites/blue.png", "sprites/red.png"];
    for (sprite, (spot, name)) in found.iter().zip(spots.iter().zip(order)) {
        assert_eq!(sprite.path, *name);
        assert_eq!((sprite.page, sprite.x, sprite.y), *spot);
        assert!(sprite.legacy_line());
    }
    assert_eq!(atlas.pages.len(), pages.len());
    for ((_, w, h, _), page) in atlas.pages.iter().zip(&pages) {
        assert_eq!((*w, *h), *page);
    }
    let page = plain.join(&atlas.pages[0].0);
    assert_eq!(fs::read(&page).unwrap(), fs::read(same.join(&atlas.pages[0].0)).unwrap());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tight_packing_is_smaller_and_draws_in_the_same_place() {
    let root = scratch("tight");
    let loose = root.join("loose");
    let tight = root.join("tight");
    let pixel = |x, y| {
        if (6..10).contains(&x) && (5..9).contains(&y) {
            [20 + x as u8, 40 + y as u8, 7, 255]
        } else {
            [0, 0, 0, 0]
        }
    };
    for dir in [&loose, &tight] {
        for name in ["a.png", "b.png", "c.png", "d.png"] {
            write_png(&dir.join("sprites").join(name), 32, 32, pixel);
        }
    }
    build_with(&loose, &options(false, false, 2048, 100)).unwrap();
    build_with(&tight, &options(true, false, 2048, 100)).unwrap();
    let loose_atlas = Atlas::load(&loose).unwrap();
    let tight_atlas = Atlas::load(&tight).unwrap();
    assert!(
        page_area(&tight_atlas) < page_area(&loose_atlas),
        "{tight_atlas:?} vs {loose_atlas:?}"
    );
    let sprite = tight_atlas.sprites.iter().find(|s| s.path.ends_with("a.png")).unwrap();
    assert_eq!(
        (sprite.origin_x, sprite.origin_y, sprite.content_w, sprite.content_h),
        (6, 5, 4, 4)
    );
    assert_eq!((sprite.source_w, sprite.source_h), (32, 32));
    let (page, pw, ph, _) = &tight_atlas.pages[sprite.page];
    let (pixels, _, _) = read_png(&tight.join(page));
    for y in 0..32 {
        for x in 0..32 {
            let source = pixel(x, y);
            match page_texel(sprite, x, y) {
                Some((px, py)) => {
                    assert!(px < *pw && py < *ph);
                    let i = ((py * pw + px) * 4) as usize;
                    assert_eq!(&pixels[i..i + 4], &source, "pixel {x},{y}");
                    let [u, v] = picture_corner_uv(
                        sprite,
                        *pw,
                        *ph,
                        (x as f32 + 0.5) / 32.0,
                        (y as f32 + 0.5) / 32.0,
                    );
                    assert_eq!((u * *pw as f32).floor() as u32, px);
                    assert_eq!((v * *ph as f32).floor() as u32, py);
                }
                None => assert_eq!(source[3], 0, "trimmed pixel {x},{y} still has colour"),
            }
        }
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn turned_sprites_sample_the_picture_and_use_less_page() {
    let sizes = [(30u32, 60u32), (80, 16)];
    let (_spots, plain_pages) = pack(&sizes, 100);
    let turned = pack_turning(&sizes, 100);
    let area = |pages: &[(u32, u32)]| pages.iter().map(|(w, h)| w * h).sum::<u32>();
    assert!(area(&turned.pages) < area(&plain_pages), "{:?} vs {:?}", turned.pages, plain_pages);
    assert!(!turned.spots[0].unwrap().turned);
    assert!(turned.spots[1].unwrap().turned);

    let root = scratch("turn");
    write_png(&root.join("tall.png"), 30, 60, |x, y| [x as u8, y as u8, 1, 255]);
    write_png(&root.join("wide.png"), 80, 16, |x, y| [x as u8, y as u8, 2, 255]);
    build_with(&root, &options(false, true, 100, 100)).unwrap();
    let atlas = Atlas::load(&root).unwrap();
    let wide = atlas.sprites.iter().find(|s| s.path == "wide.png").unwrap();
    assert!(wide.turned, "{wide:?}");
    assert!(atlas.pages.iter().all(|(_, w, h, _)| *w <= 100 && *h <= 100));
    let (page, pw, ph, _) = &atlas.pages[wide.page];
    let (pixels, _, _) = read_png(&root.join(page));
    for y in 0..16 {
        for x in 0..80 {
            let (px, py) = page_texel(wide, x, y).unwrap();
            let i = ((py * pw + px) * 4) as usize;
            assert_eq!(&pixels[i..i + 4], &[x as u8, y as u8, 2, 255]);
            let [u, v] =
                picture_corner_uv(wide, *pw, *ph, (x as f32 + 0.5) / 80.0, (y as f32 + 0.5) / 16.0);
            assert_eq!((u * *pw as f32).floor() as u32, px);
            assert_eq!((v * *ph as f32).floor() as u32, py);
        }
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn page_size_spills_onto_more_pages_and_stays_inside_the_limit() {
    let small = scratch("page-small");
    let large = scratch("page-large");
    for dir in [&small, &large] {
        for name in ["a.png", "b.png", "c.png"] {
            write_png(&dir.join(name), 40, 40, |_, _| [9, 8, 7, 255]);
        }
    }
    build_with(&small, &options(false, false, 64, 100)).unwrap();
    build_with(&large, &options(false, false, 256, 100)).unwrap();
    let small_atlas = Atlas::load(&small).unwrap();
    let large_atlas = Atlas::load(&large).unwrap();
    assert_eq!(small_atlas.sprites.len(), 3);
    assert!(small_atlas.pages.len() > large_atlas.pages.len());
    assert_eq!(large_atlas.pages.len(), 1);
    for (_, w, h, _) in &small_atlas.pages {
        assert!(*w <= 64 && *h <= 64, "{w}x{h}");
    }
    for sprite in &small_atlas.sprites {
        assert!(sprite.x + sprite.w <= 64 && sprite.y + sprite.h <= 64, "{sprite:?}");
    }
    let _ = fs::remove_dir_all(&small);
    let _ = fs::remove_dir_all(&large);
}

#[test]
fn half_scale_writes_half_the_texels_and_keeps_the_picture_size() {
    let root = scratch("half");
    write_png(&root.join("block.png"), 8, 4, |_, _| [10, 20, 30, 255]);
    build_with(&root, &options(false, false, 2048, 50)).unwrap();
    let atlas = Atlas::load(&root).unwrap();
    let sprite = &atlas.sprites[0];
    assert_eq!((sprite.w, sprite.h), (4, 2));
    assert_eq!((sprite.source_w, sprite.source_h), (8, 4));
    assert!(!sprite.turned);
    let (page, pw, ph, _) = &atlas.pages[sprite.page];
    let uv = page_uv(sprite, *pw, *ph);
    assert_eq!(((uv[2] - uv[0]) * *pw as f32).round() as u32, 4);
    assert_eq!(((uv[3] - uv[1]) * *ph as f32).round() as u32, 2);
    let (pixels, _, _) = read_png(&root.join(page));
    for y in 0..4 {
        for x in 0..8 {
            let (px, py) = page_texel(sprite, x, y).unwrap();
            let i = ((py * *pw + px) * 4) as usize;
            assert_eq!(&pixels[i..i + 4], &[10, 20, 30, 255]);
        }
    }
    let [u, v] = picture_corner_uv(sprite, *pw, *ph, 0.5, 0.5);
    let (px, py) = page_texel(sprite, 4, 2).unwrap();
    assert_eq!((u * *pw as f32).floor() as u32, px);
    assert_eq!((v * *ph as f32).floor() as u32, py);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_damaged_atlas_index_is_refused() {
    assert!(Atlas::parse("").is_none());
    assert!(Atlas::parse("seal atlas 9\n").is_none());
    assert!(Atlas::parse(
        "seal atlas 2\npage atlas/page0.png 8 8 nearest\nsprite 0 0 0 4 4 a.png\n"
    )
    .is_none());
    assert!(Atlas::parse(
        "seal atlas 2\npage atlas/page0.png 8 8 nearest\nsprite 0 0 0 4 4 8 8 0 0 8 8 2 a.png\n"
    )
    .is_none());
    assert!(Atlas::parse(
        "seal atlas 2\npage atlas/page0.png 8 8 nearest\nsprite 0 0 0 4 4 8 8 6 0 4 4 0 a.png\n"
    )
    .is_none());
    let placed = Placed {
        path: "my sprites/a b.png".into(),
        page: 0,
        x: 1,
        y: 2,
        w: 4,
        h: 3,
        source_w: 10,
        source_h: 8,
        origin_x: 2,
        origin_y: 1,
        content_w: 6,
        content_h: 5,
        turned: true,
    };
    let atlas = Atlas {
        pages: vec![("atlas/page0.png".into(), 16, 16, true)],
        sprites: vec![placed.clone()],
    };
    let text = atlas.to_text();
    assert!(text.starts_with("seal atlas 2\n"));
    assert_eq!(Atlas::parse(&text).unwrap().sprites, vec![placed]);
    let root = scratch("damaged");
    fs::write(root.join("atlas.txt"), "not an atlas\n").unwrap();
    assert!(Atlas::load(&root).is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn export_presets_store_atlas_options_and_reject_bad_ones() {
    let mut preset = ExportPreset::named("Phone", "out");
    preset.atlas_tight = true;
    preset.atlas_rotate = true;
    preset.atlas_page = 4096;
    preset.atlas_scale = 50;
    let book = ExportBook { selected: "Phone".into(), presets: vec![preset.clone()] };
    let again = parse_book(&to_text(&book)).unwrap();
    assert_eq!(again.presets[0], preset);
    assert!(again.presets[0].atlas_options().tight);
    assert_eq!(again.presets[0].atlas_options().page_size, 4096);
    assert_eq!(again.presets[0].atlas_options().scale_percent, 50);
    let old = "presets = [{ name = \"Desktop\" }]\n";
    let loaded = parse_book(old).unwrap();
    assert!(!loaded.presets[0].atlas_tight);
    assert_eq!(loaded.presets[0].atlas_page, 2048);
    assert_eq!(loaded.presets[0].atlas_scale, 100);
    assert!(parse_book("presets = [{ name = \"A\", atlas_tight = \"yes\" }]\n").is_err());
    assert!(parse_book("presets = [{ name = \"A\", atlas_page = 3000 }]\n").is_err());
    assert!(parse_book("presets = [{ name = \"A\", atlas_scale = 0 }]\n").is_err());
}
