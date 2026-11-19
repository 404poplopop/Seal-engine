//! Runtime sprite loading: placeholders, background decode, damaged files.

#[test]
fn viewport_texture_is_placeholder_before_decode_finishes() {
    let dir = std::env::temp_dir().join(format!("seal-sprite-rt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("dot.png");
    let mut f = std::fs::File::create(&path).unwrap();
    let mut enc = png::Encoder::new(&mut f, 8, 8);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&[128u8; 256]).unwrap();
    drop(f);
    let ctx = egui::Context::default();
    let path = path.to_string_lossy().to_string();
    let first = seal_ui::sprites::texture(&ctx, &path);
    assert!(first.is_some(), "placeholder texture id is returned immediately");
    let mut ready = false;
    for _ in 0..300 {
        let _ = seal_ui::sprites::texture(&ctx, &path);
        if let Some(sz) = seal_ui::sprites::texture_size(&ctx, &path) {
            if sz[0] >= 8.0 && sz[1] >= 8.0 {
                ready = true;
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ready, "full picture replaces the placeholder");
}

#[test]
fn damaged_picture_keeps_placeholder() {
    let dir = std::env::temp_dir().join(format!("seal-sprite-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bad.png");
    std::fs::write(&path, b"not a png file").unwrap();
    let ctx = egui::Context::default();
    let path = path.to_string_lossy().to_string();
    for _ in 0..100 {
        let _ = seal_ui::sprites::texture(&ctx, &path);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let id = seal_ui::sprites::texture(&ctx, &path);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(id.is_some(), "failed decode still yields a placeholder texture");
}