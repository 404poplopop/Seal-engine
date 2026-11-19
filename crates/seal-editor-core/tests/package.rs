use std::fs;
use std::path::PathBuf;

use seal_editor_core::package::{self, GAME_MARKER, MANIFEST};
use seal_editor_core::{CommandArgs, EditorSession, Layout};

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-package-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project folder with a scene, a sprite and editor-only state, plus a stand-in program.
fn sample(root: &std::path::Path) -> (PathBuf, PathBuf) {
    let project = root.join("Hop");
    fs::create_dir_all(project.join("sprites")).unwrap();
    fs::create_dir_all(project.join(".seal-editor")).unwrap();
    let tag =
        root.to_string_lossy().bytes().fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64));
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), tag & 0xffff_ffff_ffff);
    fs::write(project.join("project.seal"), format!("schema_version = 1\nname = \"Hop\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n")).unwrap();
    fs::write(project.join("scene.seal"), "scene\n").unwrap();
    fs::write(project.join("sprites").join("seal.png"), [1u8, 2, 3]).unwrap();
    fs::write(project.join(".seal-editor").join("layout.seal"), "layout").unwrap();
    fs::write(project.join("scene.seal.bak"), "older scene\n").unwrap();
    let exe = root.join("seal-editor.exe");
    fs::write(&exe, b"program").unwrap();
    (project, exe)
}

#[test]
fn a_package_holds_the_program_the_project_and_a_manifest() {
    let root = scratch("layout");
    let (project, exe) = sample(&root);
    let out = package::default_package_folder(&project, "Hop");
    assert_eq!(out, root.join("Hop Game"));
    let report = package::package_project(&project, "Hop", &exe, &out).unwrap();
    assert_eq!(report.folder, out);
    assert_eq!(fs::read(out.join("Hop.exe")).unwrap(), b"program");
    assert!(out.join("data").join("scene.seal").is_file());
    assert!(out.join("data").join("sprites").join("seal.png").is_file());
    assert!(!out.join("data").join(".seal-editor").exists(), "editor state stays out");
    assert!(!out.join("data").join("scene.seal.bak").exists(), "safety copies stay out");
    assert!(out.join("README.txt").is_file());
    let manifest = fs::read_to_string(out.join(MANIFEST)).unwrap();
    assert!(manifest.contains("data/scene.seal"));
    assert!(manifest.contains("Hop.exe"));
    // The packaged program finds its project through game.txt.
    assert_eq!(package::game_project_beside(&out.join("Hop.exe")), Some(out.join("data")));
}

#[test]
fn packaging_again_replaces_the_old_package_but_never_a_foreign_folder() {
    let root = scratch("again");
    let (project, exe) = sample(&root);
    let out = root.join("Hop Game");
    package::package_project(&project, "Hop", &exe, &out).unwrap();
    fs::write(out.join("stale.txt"), "old").unwrap();
    package::package_project(&project, "Hop", &exe, &out).unwrap();
    assert!(!out.join("stale.txt").exists());
    let foreign = root.join("Photos");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(foreign.join("cat.jpg"), "meow").unwrap();
    assert!(package::package_project(&project, "Hop", &exe, &foreign).is_err());
    assert!(foreign.join("cat.jpg").is_file(), "an unrelated folder is never touched");
    assert!(package::package_project(&project, "Hop", &exe, &project.join("out")).is_err());
}

#[test]
fn a_program_without_game_txt_is_the_editor() {
    let root = scratch("plain");
    let (_, exe) = sample(&root);
    assert_eq!(package::game_project_beside(&exe), None);
    fs::write(root.join(GAME_MARKER), "seal game\nname X\nproject nowhere\n").unwrap();
    assert_eq!(package::game_project_beside(&exe), None, "no project.seal there");
}

#[test]
fn the_package_command_packages_the_open_project() {
    let root = scratch("command");
    let (project, exe) = sample(&root);
    let mut session = EditorSession::new(Layout::default());
    let path = project.display().to_string();
    session.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    let out = root.join("Shipped");
    let out_s = out.display().to_string();
    let exe_s = exe.display().to_string();
    session
        .dispatch(
            "project.package",
            &CommandArgs::new(&[("out", out_s.as_str()), ("exe", exe_s.as_str())]),
        )
        .unwrap();
    assert!(out.join(GAME_MARKER).is_file());
    assert!(project.join("project.lock").is_file(), "the open project holds a lock");
    assert!(!out.join("data").join("project.lock").exists(), "the lock is not shipped");
    assert!(session.notice.as_deref().unwrap_or_default().contains("Shipped"));
}

fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: [u8; 4]) {
    let file = std::io::BufWriter::new(fs::File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let pixels: Vec<u8> = (0..w * h).flat_map(|_| rgba).collect();
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}

#[test]
fn packaging_packs_small_pictures_onto_atlas_pages() {
    use seal_editor_core::atlas::Atlas;
    let root = scratch("atlas");
    let (project, exe) = sample(&root);
    write_png(&project.join("sprites").join("red.png"), 16, 8, [255, 0, 0, 255]);
    write_png(&project.join("sprites").join("blue.png"), 4, 12, [0, 0, 255, 255]);
    write_png(&project.join("huge.png"), 600, 2, [0, 255, 0, 255]);
    let out = root.join("Hop Game");
    package::package_project(&project, "Hop", &exe, &out).unwrap();
    let data = out.join("data");
    let atlas = Atlas::load(&data).expect("atlas.txt written");
    let mut packed: Vec<&str> = atlas.sprites.iter().map(|s| s.path.as_str()).collect();
    packed.sort();
    assert_eq!(packed, ["sprites/blue.png", "sprites/red.png"], "big and broken pictures stay out");
    assert!(data.join("sprites").join("red.png").is_file(), "originals are kept");
    // Each picture's pixels are on its page where the index says.
    let page = data.join(&atlas.pages[0].0);
    let decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(&page).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut buf).unwrap();
    let page_w = atlas.pages[0].1;
    for s in &atlas.sprites {
        let want = if s.path.ends_with("red.png") { [255, 0, 0, 255] } else { [0, 0, 255, 255] };
        for (x, y) in [(s.x, s.y), (s.x + s.w - 1, s.y + s.h - 1)] {
            let i = ((y * page_w + x) * 4) as usize;
            assert_eq!(buf[i..i + 4], want, "{} at {x},{y}", s.path);
        }
    }
    let manifest = fs::read_to_string(out.join(MANIFEST)).unwrap();
    assert!(manifest.contains("data/atlas.txt") && manifest.contains("data/atlas/page0.png"));
}

#[test]
fn a_packaged_game_opens_even_while_the_original_project_is_known() {
    let root = scratch("same-id");
    let (project, exe) = sample(&root);
    // The editor knows the original project, which registers its id.
    seal_editor_core::open_project(&project).unwrap();
    let out = root.join("Hop Game");
    package::package_project(&project, "Hop", &exe, &out).unwrap();
    let game = package::game_project_beside(&out.join("Hop.exe")).unwrap();
    let opened = seal_editor_core::open_project(&game).expect("the game opens its own copy");
    assert_eq!(opened.name, "Hop");
    // And the original still opens: the copy never took its id.
    seal_editor_core::open_project(&project).unwrap();
}
