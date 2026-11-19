use seal_editor_core::{CommandArgs, EditorSession, Layout};
use std::fs;

#[test]
fn make_prefab_saves_a_file_links_it_and_add_prefab_places_a_copy() {
    let dir = std::env::temp_dir().join(format!("seal-prefab-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let project = dir.join("P");
    fs::create_dir_all(&project).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 7);
    fs::write(project.join("project.seal"), format!("schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n")).unwrap();
    fs::write(project.join("scene.seal"), "scene\n").unwrap();
    let path = project.to_string_lossy().to_string();
    let mut s = EditorSession::new(Layout::default());
    s.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    s.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
    let id = s.models.selection.clone().unwrap();
    let before = s.world.entities.len();
    s.dispatch("scene.make_prefab", &CommandArgs::new(&[("name", "Crate")])).unwrap();
    assert!(project.join("prefabs").join("Crate.prefab").is_file());
    assert_eq!(s.world.entity(&id).unwrap().source_asset, "Crate");
    s.dispatch("scene.make_prefab", &CommandArgs::new(&[("name", "Crate")])).unwrap();
    assert!(project.join("prefabs").join("Crate 2.prefab").is_file(), "never overwrites");
    s.dispatch("scene.add_prefab", &CommandArgs::new(&[("path", "prefabs/Crate.prefab")])).unwrap();
    assert!(s.world.entities.len() > before);
    assert!(s.dispatch("scene.add_prefab", &CommandArgs::new(&[("path", "../x.prefab")])).is_err());
    s.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert_eq!(s.world.entities.len(), before);
}

#[test]
fn prefab_copies_rebuild_from_the_file_and_keep_their_own_changes() {
    let dir = std::env::temp_dir().join(format!("seal-prefab-sync-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(&project).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 8);
    fs::write(project.join("project.seal"), format!("schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n")).unwrap();
    fs::write(project.join("scene.seal"), "scene\n").unwrap();
    let path = project.to_string_lossy().to_string();
    let mut s = EditorSession::new(Layout::default());
    s.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    s.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
    let source = s.models.selection.clone().unwrap();
    s.dispatch("scene.make_prefab", &CommandArgs::new(&[("name", "Box")])).unwrap();

    s.models.selection = None;
    s.dispatch("scene.add_prefab", &CommandArgs::new(&[("path", "prefabs/Box.prefab")])).unwrap();
    let a = s.models.selection.clone().unwrap();
    s.models.selection = None;
    s.dispatch("scene.add_prefab", &CommandArgs::new(&[("path", "prefabs/Box.prefab")])).unwrap();
    let b = s.models.selection.clone().unwrap();
    let set = |s: &mut EditorSession, id: &str, field: &str, value: &str| {
        s.dispatch(
            "inspector.set",
            &CommandArgs::new(&[("id", id), ("field", field), ("value", value)]),
        )
        .unwrap();
    };
    // Copy A changes its role and moves; that must survive a rebuild.
    set(&mut s, &a, "role", "coin");
    set(&mut s, &a, "x", "5");
    assert_eq!(s.world.entity(&a).unwrap().overrides, "role", "moving is not an override");
    // The prefab itself changes: new role and metal.
    let mut edited = seal_world::World::from_text(&s.world.to_text()).unwrap();
    let e = edited.entity_mut(&source).unwrap();
    e.tag = "prop".into();
    e.metal = 0.9;
    fs::write(project.join("prefabs/Box.prefab"), edited.prefab_text(&source).unwrap()).unwrap();
    s.dispatch("prefab.refresh", &CommandArgs::default()).unwrap();
    let ea = s.world.entity(&a).unwrap();
    assert_eq!(ea.tag, "coin");
    assert_eq!(ea.translation[0], 5.0);
    assert!((ea.metal - 0.9).abs() < 1e-4, "non-overridden fields follow the file");
    let eb = s.world.entity(&b).unwrap();
    assert_eq!(eb.tag, "prop");
    assert!((eb.metal - 0.9).abs() < 1e-4);
    // Opening the project again rebuilds too.
    s.dispatch("file.save", &CommandArgs::default()).unwrap_or(());
    s.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert!(s.world.entity(&b).unwrap().metal < 0.5, "refresh is one undo step");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dropping_a_prefab_by_file_name_places_it_and_its_children_at_the_drop_point() {
    let dir = std::env::temp_dir().join(format!("seal-prefab-drop-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(&project).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 9);
    fs::write(project.join("project.seal"), format!("schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n")).unwrap();
    fs::write(project.join("scene.seal"), "scene\n").unwrap();
    let path = project.to_string_lossy().to_string();
    let mut s = EditorSession::new(Layout::default());
    s.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    s.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube"), ("a", "1"), ("b", "1")]))
        .unwrap();
    let parent = s.models.selection.clone().unwrap();
    s.dispatch(
        "scene.add",
        &CommandArgs::new(&[("kind", "cube"), ("a", "2"), ("b", "1"), ("parent", parent.as_str())]),
    )
    .unwrap();
    s.models.selection = Some(parent.clone());
    s.dispatch("scene.make_prefab", &CommandArgs::new(&[("name", "Pair")])).unwrap();
    s.models.selection = None;
    s.dispatch(
        "scene.add_prefab",
        &CommandArgs::new(&[("path", "Pair.prefab"), ("a", "10"), ("b", "4")]),
    )
    .unwrap();
    let copy = s.models.selection.clone().unwrap();
    let root = s.world.entity(&copy).unwrap();
    assert_eq!((root.translation[0], root.translation[1]), (10.0, 4.0));
    let child = s
        .world
        .entities
        .iter()
        .find(|e| e.parent.as_deref() == Some(copy.as_str()))
        .unwrap()
        .id
        .clone();
    let c = s.world.entity(&child).unwrap();
    assert_eq!((c.translation[0], c.translation[1]), (11.0, 4.0), "child keeps its offset");
    s.dispatch("prefab.refresh", &CommandArgs::default()).unwrap();
    let c = s.world.entities.iter().find(|e| e.parent.as_deref() == Some(copy.as_str())).unwrap();
    assert_eq!(
        (c.translation[0], c.translation[1]),
        (11.0, 4.0),
        "rebuilt children follow the copy"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn editing_a_prefab_on_its_own_saves_the_file_updates_copies_and_keeps_the_level() {
    let dir = std::env::temp_dir().join(format!("seal-prefab-edit-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(&project).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 10);
    fs::write(project.join("project.seal"), format!("schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n")).unwrap();
    fs::write(project.join("scene.seal"), "scene\n").unwrap();
    let path = project.to_string_lossy().to_string();
    let mut s = EditorSession::new(Layout::default());
    s.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    s.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
    s.dispatch("scene.make_prefab", &CommandArgs::new(&[("name", "Crate")])).unwrap();
    s.models.selection = None;
    s.dispatch(
        "scene.add_prefab",
        &CommandArgs::new(&[("path", "Crate.prefab"), ("a", "7"), ("b", "0")]),
    )
    .unwrap();
    let copy = s.models.selection.clone().unwrap();
    let level_count = s.world.entities.len();
    let undo_depth = s.past.len();

    // Inspector > Edit Prefab on the copy.
    s.dispatch("prefab.edit", &CommandArgs::default()).unwrap();
    assert!(s.prefab_edit.is_some());
    assert!(s.world.entities.len() < level_count, "only the prefab is shown");
    let root = s.models.selection.clone().unwrap();
    s.dispatch(
        "inspector.set",
        &CommandArgs::new(&[("id", root.as_str()), ("field", "role"), ("value", "coin")]),
    )
    .unwrap();
    let saved = fs::read_to_string(project.join("prefabs/Crate.prefab")).unwrap();
    assert!(saved.contains("coin"), "every edit lands in the prefab file at once: {saved}");
    // Saving the level while the prefab is open keeps the level, not the prefab.
    s.save_scene().unwrap();
    let level = fs::read_to_string(project.join("scene.seal")).unwrap();
    assert!(seal_world::World::from_text(&level).unwrap().entities.len() == level_count);

    s.dispatch("prefab.done", &CommandArgs::default()).unwrap();
    assert!(s.prefab_edit.is_none());
    assert_eq!(s.world.entities.len(), level_count);
    let e = s.world.entity(&copy).unwrap();
    assert_eq!(e.tag, "coin", "the copy follows the edited prefab");
    assert_eq!(e.translation[0], 7.0, "and stays where it was placed");
    assert_eq!(s.past.len(), undo_depth + 1, "the update is one undo step");
    s.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert_ne!(s.world.entity(&copy).unwrap().tag, "coin");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_copy_can_apply_its_changes_to_the_prefab_or_revert_them() {
    let dir = std::env::temp_dir().join(format!("seal-prefab-apply-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(&project).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 11);
    fs::write(project.join("project.seal"), format!("schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n")).unwrap();
    fs::write(project.join("scene.seal"), "scene\n").unwrap();
    let path = project.to_string_lossy().to_string();
    let mut s = EditorSession::new(Layout::default());
    s.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    s.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
    s.dispatch("scene.make_prefab", &CommandArgs::new(&[("name", "Gem")])).unwrap();
    s.models.selection = None;
    s.dispatch("scene.add_prefab", &CommandArgs::new(&[("path", "Gem.prefab")])).unwrap();
    let a = s.models.selection.clone().unwrap();
    s.models.selection = None;
    s.dispatch("scene.add_prefab", &CommandArgs::new(&[("path", "Gem.prefab")])).unwrap();
    let b = s.models.selection.clone().unwrap();
    let set = |s: &mut EditorSession, id: &str, field: &str, value: &str| {
        s.dispatch(
            "inspector.set",
            &CommandArgs::new(&[("id", id), ("field", field), ("value", value)]),
        )
        .unwrap();
    };
    set(&mut s, &a, "role", "coin");
    set(&mut s, &a, "metal", "0.8");
    // Revert just one field.
    s.dispatch("prefab.revert", &CommandArgs::new(&[("id", a.as_str()), ("field", "metal")]))
        .unwrap();
    assert!(s.world.entity(&a).unwrap().metal < 0.5);
    assert_eq!(s.world.entity(&a).unwrap().tag, "coin");
    // Apply the rest: B follows, A has no own changes left.
    s.dispatch("prefab.apply", &CommandArgs::new(&[("id", a.as_str())])).unwrap();
    assert_eq!(s.world.entity(&b).unwrap().tag, "coin");
    assert!(s.world.entity(&a).unwrap().overrides.is_empty());
    let file = fs::read_to_string(project.join("prefabs/Gem.prefab")).unwrap();
    assert!(!file.contains("source_asset = \"Gem\""), "the file does not point at itself");
    // Revert everything on B after a change.
    set(&mut s, &b, "role", "goal");
    s.dispatch("prefab.revert", &CommandArgs::new(&[("id", b.as_str())])).unwrap();
    assert_eq!(s.world.entity(&b).unwrap().tag, "coin");
    s.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert_eq!(s.world.entity(&b).unwrap().tag, "goal");
    let _ = fs::remove_dir_all(&dir);
}
