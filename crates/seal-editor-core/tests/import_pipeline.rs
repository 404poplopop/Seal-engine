//! Parallel imports, post-import scripts, custom importers, and cancelable jobs.

use seal_editor_core::import_pipeline::{
    self, commit_bytes, load_book, save_book, tags_of, CustomImporter, KindScript, PipelineBook,
    Scratch,
};
use seal_editor_core::{CommandArgs, EditorSession, Layout};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn scratch(label: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir =
        std::env::temp_dir().join(format!("seal-imp-{label}-{}-{n}-{nanos}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_sources(dir: &Path, count: usize) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for index in 0..count {
        let path = dir.join(format!("f{index:02}.png"));
        fs::write(&path, format!("png-bytes-{index:02}-{}", index * 17)).unwrap();
        paths.push(path);
    }
    paths
}

fn open_project(dir: &Path) -> (EditorSession, PathBuf) {
    let folder = dir.join("Game");
    let mut session = EditorSession::new(Layout::default());
    session.recent_path = Some(dir.join("recent.seal"));
    session
        .dispatch(
            "file.new-project",
            &CommandArgs::new(&[("path", folder.to_str().unwrap()), ("name", "Game")]),
        )
        .unwrap();
    (session, folder)
}

#[test]
fn parallel_import_matches_serial_for_fifty_files() {
    let src = scratch("src50");
    let paths = write_sources(&src, 50);
    let serial_root = scratch("serial");
    let parallel_root = scratch("parallel");
    let (serial, serial_threads) =
        import_pipeline::import_blocking(&serial_root, paths.clone(), 1).unwrap();
    let (parallel, parallel_threads) =
        import_pipeline::import_blocking(&parallel_root, paths, 4).unwrap();
    assert_eq!(serial.len(), 50);
    assert_eq!(parallel.len(), 50);
    assert_eq!(serial_threads, 1);
    assert!(parallel_threads > 1, "worker pool used {parallel_threads} threads");
    let hashes = |plans: &[seal_editor_core::import_pipeline::PlannedFile]| -> Vec<String> {
        plans.iter().map(|plan| plan.hash.clone()).collect()
    };
    assert_eq!(hashes(&serial), hashes(&parallel));
    for plan in &serial {
        let bytes = fs::read(serial_root.join(&plan.dest_rel)).unwrap();
        let other = fs::read(parallel_root.join(&plan.dest_rel)).unwrap();
        assert_eq!(bytes, other, "{}", plan.dest_rel);
        assert_eq!(bytes, plan.bytes);
        assert!(!plan.hash.is_empty());
    }
}

#[test]
fn post_import_script_changes_pivot_name_and_tag() {
    let dir = scratch("script");
    let (mut session, folder) = open_project(&dir);
    fs::create_dir_all(folder.join("logic")).unwrap();
    fs::write(
        folder.join("logic/after.graph"),
        r#"schema_version = 1
seal_graph = "after"
vars = []
nodes = [
  { id = 1, kind = "event", name = "import" },
  { id = 2, kind = "import_set_pivot", x = 0.0, y = 1.0 },
  { id = 3, kind = "import_rename", name = "hero" },
  { id = 4, kind = "import_add_tag", tag = "character" },
]
links = [
  "1.then -> 2.in",
  "2.then -> 3.in",
  "3.then -> 4.in",
]
"#,
    )
    .unwrap();
    let mut book = PipelineBook::default();
    book.scripts.push(KindScript { kind: "picture".into(), graph: "logic/after.graph".into() });
    save_book(&folder, &book).unwrap();
    let art = dir.join("coin.png");
    fs::write(&art, b"picture-bytes").unwrap();
    session
        .dispatch("asset.import", &CommandArgs::new(&[("path", art.to_str().unwrap())]))
        .unwrap();
    import_pipeline::pump_all(&mut session).unwrap();
    assert!(folder.join("sprites/hero.png").is_file());
    assert!(!folder.join("sprites/coin.png").exists());
    assert_eq!(fs::read(folder.join("sprites/hero.png")).unwrap(), b"picture-bytes");
    let sidecar = fs::read_to_string(folder.join("sprites/hero.png.import.seal")).unwrap();
    assert!(sidecar.contains("custom"), "{sidecar}");
    assert!(sidecar.contains("pivot_x"), "{sidecar}");
    assert_eq!(tags_of(&folder, "sprites/hero.png"), vec!["character".to_string()]);
    let row = session.models.import_activity.iter().find(|row| row.file == "sprites/hero.png");
    let row = row.expect("activity");
    assert_eq!(row.result, "ok");
    assert!(!row.output_hash.is_empty());
    assert_eq!(row.output_hash.len(), 16);
}

#[test]
fn custom_importer_is_picked_by_extension() {
    let dir = scratch("custom");
    let (mut session, folder) = open_project(&dir);
    fs::create_dir_all(folder.join("logic")).unwrap();
    fs::write(
        folder.join("logic/xyz.graph"),
        r#"schema_version = 1
seal_graph = "xyz"
vars = []
nodes = [
  { id = 1, kind = "event", name = "import" },
  { id = 2, kind = "import_set_text", text = "IMPORTED" },
  { id = 3, kind = "import_add_tag", tag = "custom" },
]
links = [
  "1.then -> 2.in",
  "2.then -> 3.in",
]
"#,
    )
    .unwrap();
    let mut book = PipelineBook::default();
    book.importers
        .push(CustomImporter { extension: "xyz".into(), graph: "logic/xyz.graph".into() });
    save_book(&folder, &book).unwrap();
    let data = dir.join("pack.xyz");
    fs::write(&data, b"raw").unwrap();
    let png = dir.join("plain.png");
    fs::write(&png, b"png").unwrap();
    let both = format!("{}\n{}", data.display(), png.display());
    session.dispatch("asset.import", &CommandArgs::new(&[("path", both.as_str())])).unwrap();
    import_pipeline::pump_all(&mut session).unwrap();
    assert_eq!(fs::read(folder.join("pack.xyz")).unwrap(), b"IMPORTED");
    assert_eq!(tags_of(&folder, "pack.xyz"), vec!["custom".to_string()]);
    let custom = session.models.import_activity.iter().find(|row| row.file == "pack.xyz").unwrap();
    assert_eq!(custom.importer, "custom:xyz");
    let picture =
        session.models.import_activity.iter().find(|row| row.file.ends_with("plain.png")).unwrap();
    assert_eq!(picture.importer, "import");
}

#[test]
fn project_importer_wins_over_the_compiled_hook() {
    fn hook(scratch: &mut Scratch) -> Result<(), String> {
        scratch.bytes = b"HOOK".to_vec();
        scratch.text_set = true;
        scratch.tags.push("hooked".into());
        Ok(())
    }
    import_pipeline::register_importer_hook("zzg10", "Zed", hook);
    let dir = scratch("hook");
    let root = dir.join("proj");
    fs::create_dir_all(&root).unwrap();
    let source = dir.join("a.zzg10");
    fs::write(&source, b"raw").unwrap();
    let (plans, _) = import_pipeline::import_blocking(&root, vec![source.clone()], 1).unwrap();
    assert_eq!(plans[0].importer, "custom:zzg10");
    assert_eq!(fs::read(root.join(&plans[0].dest_rel)).unwrap(), b"HOOK");
    assert_eq!(tags_of(&root, &plans[0].dest_rel), vec!["hooked".to_string()]);

    fs::create_dir_all(root.join("logic")).unwrap();
    fs::write(
        root.join("logic/zz.graph"),
        r#"schema_version = 1
seal_graph = "zz"
vars = []
nodes = [
  { id = 1, kind = "event", name = "import" },
  { id = 2, kind = "import_set_text", text = "GRAPH" },
]
links = [ "1.then -> 2.in" ]
"#,
    )
    .unwrap();
    let mut book = PipelineBook::default();
    book.importers
        .push(CustomImporter { extension: "zzg10".into(), graph: "logic/zz.graph".into() });
    save_book(&root, &book).unwrap();
    let source2 = dir.join("b.zzg10");
    fs::write(&source2, b"raw").unwrap();
    let _ = fs::remove_file(root.join("a.zzg10"));
    let (plans, _) = import_pipeline::import_blocking(&root, vec![source2], 1).unwrap();
    assert_eq!(fs::read(root.join(&plans[0].dest_rel)).unwrap(), b"GRAPH");
    assert!(!plans[0].tags.iter().any(|tag| tag == "hooked"));
    import_pipeline::remove_importer_hook("zzg10");
}

#[test]
fn cancel_stops_before_the_next_file_and_drops_partials() {
    let dir = scratch("cancel");
    let (mut session, folder) = open_project(&dir);
    let src = dir.join("batch");
    fs::create_dir_all(&src).unwrap();
    let paths = write_sources(&src, 20);
    let _ = paths;
    session
        .dispatch("asset.import", &CommandArgs::new(&[("path", src.to_str().unwrap())]))
        .unwrap();
    import_pipeline::pump_steps(&mut session, 5).unwrap();
    let fraction = import_pipeline::job_fraction(&session).unwrap();
    assert!((fraction - 0.25).abs() < 0.001, "{fraction}");
    let task = session.foundation.tasks.iter().find(|task| task.id == "import").unwrap();
    assert!((task.progress - 0.25).abs() < 0.001);
    let written = fs::read_dir(folder.join("sprites"))
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".png"))
        .count();
    assert_eq!(written, 5);
    session.cancel_task("import");
    import_pipeline::pump_all(&mut session).unwrap();
    assert!(session.models.long_job.is_none());
    let left: Vec<_> = fs::read_dir(folder.join("sprites"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".png"))
        .collect();
    assert_eq!(left.len(), 5);
    assert!(no_partials(&folder), "partial files remained");
    let stray = folder.join("sprites/leftover.png.partial");
    fs::write(&stray, b"half").unwrap();
    assert!(!commit_bytes(&folder.join("sprites/leftover.png"), b"full", true).unwrap());
    assert!(!folder.join("sprites/leftover.png").is_file());
    assert!(!stray.is_file());
}

fn no_partials(root: &Path) -> bool {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy();
            if name.ends_with(".partial") || name.contains(".tmp") {
                return false;
            }
        }
    }
    true
}

#[test]
fn damaged_book_and_damaged_script_do_not_write_a_partial_file() {
    let dir = scratch("damage");
    fs::write(dir.join("import-pipeline.seal"), "scripts = no\n").unwrap();
    let book = load_book(&dir);
    assert!(book.load_warning.is_some(), "{:?}", book.load_warning);
    assert!(book.scripts.is_empty());

    let (mut session, folder) = open_project(&dir);
    fs::write(
        folder.join("import-pipeline.seal"),
        "scripts = [ { kind = \"picture\", graph = \"logic/bad.graph\" } ]\n",
    )
    .unwrap();
    fs::create_dir_all(folder.join("logic")).unwrap();
    fs::write(folder.join("logic/bad.graph"), "this is not a graph {").unwrap();
    let art = dir.join("bad.png");
    fs::write(&art, b"png").unwrap();
    session
        .dispatch("asset.import", &CommandArgs::new(&[("path", art.to_str().unwrap())]))
        .unwrap();
    import_pipeline::pump_all(&mut session).unwrap();
    assert!(!folder.join("sprites/bad.png").exists());
    assert!(no_partials(&folder));
    let row = &session.models.import_activity[0];
    assert_eq!(row.result, "failed");
    assert!(row.output_hash.is_empty());
}

#[test]
fn reindex_cancel_leaves_the_index_file_unchanged() {
    let dir = scratch("reindex");
    let (mut session, folder) = open_project(&dir);
    fs::write(folder.join("note.txt"), b"hello").unwrap();
    let index = folder.join("asset-index.seal");
    let original = b"schema_version = 1\nentries = []\n".to_vec();
    fs::write(&index, &original).unwrap();
    session.dispatch("asset.reindex", &CommandArgs::default()).unwrap();
    import_pipeline::pump_steps(&mut session, 1).unwrap();
    let fraction = import_pipeline::job_fraction(&session).unwrap();
    assert!(fraction > 0.0 && fraction < 1.0, "{fraction}");
    session.cancel_task("reindex");
    import_pipeline::pump_all(&mut session).unwrap();
    assert_eq!(fs::read(&index).unwrap(), original);
    assert!(no_partials(&folder));
}
