use std::ffi::{c_char, CStr, CString};
use std::path::PathBuf;
use std::ptr;

use seal_capi::*;

fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

fn last_error() -> String {
    let mut buf = [0 as c_char; 512];
    unsafe { seal_last_error(buf.as_mut_ptr(), buf.len()) };
    unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
}

fn get(w: *const SealWorld, e: u64, name: &str) -> Result<String, SealStatus> {
    let name = c(name);
    let mut len = 0usize;
    let status = unsafe { seal_component_get(w, e, name.as_ptr(), ptr::null_mut(), 0, &mut len) };
    if status != SealStatus::BufferTooSmall {
        return Err(status);
    }
    let mut buf = vec![0 as c_char; len + 1];
    let status =
        unsafe { seal_component_get(w, e, name.as_ptr(), buf.as_mut_ptr(), buf.len(), &mut len) };
    assert_eq!(status, SealStatus::Ok);
    Ok(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_str().unwrap().to_string())
}

#[test]
fn world_entities_components_and_scenes() {
    unsafe {
        let w = seal_world_new();
        let a = seal_entity_spawn(w);
        let b = seal_entity_spawn(w);
        assert_eq!(seal_world_entity_count(w), 2);
        assert_eq!(
            seal_component_set(w, a, c("name").as_ptr(), c("\"Crate\"").as_ptr()),
            SealStatus::Ok
        );
        assert_eq!(get(w, a, "name").unwrap(), "\"Crate\"");

        // Bad input is a status with a message, never a crash.
        assert_eq!(
            seal_component_set(w, a, c("no_such").as_ptr(), c("1").as_ptr()),
            SealStatus::NotFound
        );
        assert!(last_error().contains("no_such"));
        assert_eq!(
            seal_component_set(w, a, c("name").as_ptr(), c("{ broken").as_ptr()),
            SealStatus::Error
        );
        assert_eq!(
            seal_component_set(w, a, ptr::null(), c("1").as_ptr()),
            SealStatus::InvalidArgument
        );
        assert_eq!(get(w, b, "name"), Err(SealStatus::NotFound));
        assert_eq!(get(w, SEAL_ENTITY_NONE, "name"), Err(SealStatus::NoSuchEntity));
        assert_eq!(seal_entity_spawn(ptr::null_mut()), SEAL_ENTITY_NONE);

        // A small buffer is refused untouched, and the length is reported.
        let mut small = [7 as c_char; 3];
        let mut len = 0;
        assert_eq!(
            seal_component_get(w, a, c("name").as_ptr(), small.as_mut_ptr(), 3, &mut len),
            SealStatus::BufferTooSmall
        );
        assert_eq!((len, small), (7, [7; 3]));

        assert_eq!(seal_entity_set_parent(w, b, a), SealStatus::Ok);
        assert_eq!(seal_entity_parent(w, b), a);
        assert_eq!(seal_entity_set_parent(w, a, b), SealStatus::Error);
        assert_eq!(seal_entity_set_parent(w, b, SEAL_ENTITY_NONE), SealStatus::Ok);
        assert_eq!(seal_entity_parent(w, b), SEAL_ENTITY_NONE);

        let mut len = 0;
        seal_scene_save(w, ptr::null_mut(), 0, &mut len);
        let mut scene = vec![0 as c_char; len + 1];
        assert_eq!(seal_scene_save(w, scene.as_mut_ptr(), scene.len(), &mut len), SealStatus::Ok);
        let copy = seal_world_new();
        assert_eq!(seal_scene_load(copy, scene.as_ptr()), SealStatus::Ok);
        assert_eq!(seal_world_entity_count(copy), 2);
        assert_eq!(seal_scene_load(copy, c("not a scene {").as_ptr()), SealStatus::Error);

        assert_eq!(seal_component_remove(w, a, c("name").as_ptr()), SealStatus::Ok);
        assert_eq!(seal_component_remove(w, a, c("name").as_ptr()), SealStatus::NotFound);
        assert_eq!(seal_entity_despawn(w, a), SealStatus::Ok);
        assert_eq!(seal_entity_alive(w, a), 0);
        assert_eq!(seal_entity_despawn(w, a), SealStatus::NoSuchEntity);
        seal_world_free(copy);
        seal_world_free(w);
        seal_world_free(ptr::null_mut());
    }
}

fn header() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/include/seal.h")).unwrap()
}

/// seal.h and the Rust exports must list the same functions and constants.
#[test]
fn header_matches_exports() {
    let header = header();
    let source =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")).unwrap();
    // Every `seal_name(` in the header is a declaration.
    let mut declared: Vec<String> = header
        .match_indices("seal_")
        .filter_map(|(at, _)| {
            let word: String =
                header[at..].chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').collect();
            header[at + word.len()..].starts_with('(').then_some(word)
        })
        .collect();
    declared.sort();
    declared.dedup();
    let mut exported: Vec<String> = source
        .lines()
        .filter_map(|l| l.split("extern \"C\" fn ").nth(1))
        .map(|rest| rest.split('(').next().unwrap().to_string())
        .collect();
    exported.sort();
    assert_eq!(declared, exported);
    assert!(header.contains(&format!("#define SEAL_ABI_VERSION {}u", SEAL_ABI_VERSION)));
    for (name, value) in [
        ("SEAL_OK", SealStatus::Ok),
        ("SEAL_ERROR", SealStatus::Error),
        ("SEAL_BUFFER_TOO_SMALL", SealStatus::BufferTooSmall),
        ("SEAL_INVALID_ARGUMENT", SealStatus::InvalidArgument),
        ("SEAL_NO_SUCH_ENTITY", SealStatus::NoSuchEntity),
        ("SEAL_NOT_FOUND", SealStatus::NotFound),
        ("SEAL_PANIC", SealStatus::Panic),
    ] {
        let at = header.find(&format!("{name} = ")).unwrap_or_else(|| panic!("{name} missing"));
        let number: String =
            header[at + name.len() + 3..].chars().take_while(char::is_ascii_digit).collect();
        assert_eq!(number, (value as i32).to_string(), "{name}");
    }
}

/// Compile tests/smoke.c against the static library and run it, when a C
/// compiler and the library are present (skipped otherwise).
#[test]
fn c_program_links_and_runs() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.join("../../target"));
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    let lib = target.join(profile).join("libseal_capi.a");
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".into());
    // Tests link the rlib; build the static library so it is current.
    if let Ok(cargo) = std::env::var("CARGO") {
        let mut build = std::process::Command::new(cargo);
        build.args(["build", "-q", "-p", "seal-capi", "--lib"]);
        if profile == "release" {
            build.arg("--release");
        }
        let _ = build.status();
    }
    if !lib.exists() || std::process::Command::new(&cc).arg("--version").output().is_err() {
        eprintln!("skipped: no C compiler or {}", lib.display());
        return;
    }
    let exe = std::env::temp_dir().join(format!("seal-capi-smoke-{}", std::process::id()));
    let build = std::process::Command::new(&cc)
        .arg("-std=c99")
        .arg("-Wall")
        .arg("-Werror")
        .arg("-I")
        .arg(dir.join("include"))
        .arg(dir.join("tests/smoke.c"))
        .arg(&lib)
        .args(["-lpthread", "-ldl", "-lm", "-o"])
        .arg(&exe)
        .output()
        .unwrap();
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    let run = std::process::Command::new(&exe).output().unwrap();
    let _ = std::fs::remove_file(&exe);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "c smoke ok");
}

#[test]
fn standard_scenes_load_and_save_with_their_settings() {
    let scene = "seal_scene = 1\n\
        resources = { scene_settings = { gravity = 2.5, time_scale = 0.5 } }\n\
        entities = [\n\
          { id = 0, components = { name = \"Lamp\", look = { shape = \"sphere\", color = [1, 0.5, 0], roughness = 0.5, metal = 0 }, light = { intensity = 250 }, editor = { key = \"lamp\", tag = \"light\" } } },\n\
        ]\n";
    let w = seal_world_new();
    let text = c(scene);
    assert_eq!(unsafe { seal_scene_load(w, text.as_ptr()) }, SealStatus::Ok, "{}", last_error());
    let mut len = 0usize;
    assert_eq!(
        unsafe { seal_scene_save(w, ptr::null_mut(), 0, &mut len) },
        SealStatus::BufferTooSmall
    );
    let mut buf = vec![0 as c_char; len + 1];
    assert_eq!(
        unsafe { seal_scene_save(w, buf.as_mut_ptr(), buf.len(), &mut len) },
        SealStatus::Ok
    );
    let saved = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_str().unwrap().to_string();
    for part in [
        "gravity = 2.5",
        "time_scale = 0.5",
        "intensity = 250",
        "shape = \"sphere\"",
        "key = \"lamp\"",
    ] {
        assert!(saved.contains(part), "{part} missing from {saved}");
    }
    unsafe { seal_world_free(w) };
}
