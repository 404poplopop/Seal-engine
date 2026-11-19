use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use seal_sealtext::{parse, SealValue};

use crate::error::CoreError;
use crate::messages::Messages;
use crate::persist::{atomic_write, backup_path, move_aside};
use crate::workspace::user_state_dir;

pub const PROJECT_SCHEMA: u64 = 1;

const INDEX_SCHEMA: u64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenProject {
    pub root: PathBuf,
    pub name: String,
    pub schema_version: u64,
    pub id: String,
    pub editor_version: String,
    pub read_only: bool,
    pub ignore: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreateProjectOptions {
    pub init_git: bool,
    pub git_lfs: bool,
    pub confirm_nonempty: bool,
}

pub fn create_project(folder: &Path, name: &str) -> Result<OpenProject, CoreError> {
    create_project_with_options(folder, name, &CreateProjectOptions::default())
}

pub fn create_project_with_options(
    folder: &Path,
    name: &str,
    options: &CreateProjectOptions,
) -> Result<OpenProject, CoreError> {
    let name = validate_project_name(name)?;
    if folder.exists() {
        if folder_has_files(folder) && !options.confirm_nonempty {
            return Err(CoreError::new("project folder is not empty"));
        }
    }
    fs::create_dir_all(folder)?;
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let id = new_project_id();
    let mut raw = SealValue::Map(Vec::new().into());
    raw.insert("schema_version", SealValue::Number(PROJECT_SCHEMA.to_string()));
    raw.insert("name", SealValue::String(name.to_string()));
    raw.insert("id", SealValue::String(id));
    raw.insert("created", SealValue::Number(created.to_string()));
    raw.insert("editor_version", SealValue::String(crate::version::ENGINE_VERSION_FULL.into()));
    raw.insert("modules", SealValue::Array(Vec::new()));
    raw.insert("settings", SealValue::Map(Vec::new().into()));
    raw.insert("ignore", SealValue::Array(vec![SealValue::String(".seal-editor/".into())]));
    write_manifest(folder, &raw)?;
    if options.init_git {
        write_repo_hints(folder, options.git_lfs)?;
        init_git_repo(folder)?;
    }
    write_readme_stub(folder, name)?;
    let _ = crate::naming_rules::write_template_defaults(folder);
    let project = read_project(folder)?;
    register_project_id(&project)?;
    Ok(project)
}

pub fn open_project(folder: &Path) -> Result<OpenProject, CoreError> {
    // The manifest itself names its project too: open the folder it sits in.
    let folder = match folder.file_name().and_then(|name| name.to_str()) {
        Some("project.seal") if folder.is_file() => folder.parent().unwrap_or(folder),
        _ => folder,
    };
    let path = folder.join("project.seal");
    let text = fs::read_to_string(&path)
        .map_err(|_| CoreError::new(format!("no project manifest in {}", folder.display())))?;
    let raw = parse(&text).map_err(|err| {
        CoreError::new(format!("project manifest is unreadable ({err}); repair is available"))
    })?;
    let read_only = folder_is_read_only(folder);
    let project = project_from_raw(folder, raw, read_only)?;
    if !is_packaged_game_data(folder) {
        register_project_id(&project)?;
    }
    Ok(project)
}

/// A packaged game's own copy of a project (`data` beside `game.txt`). It keeps
/// the original's id, so it is never checked against or added to the index.
fn is_packaged_game_data(folder: &Path) -> bool {
    folder.parent().is_some_and(|dir| dir.join(crate::package::GAME_MARKER).is_file())
}

pub fn repair_project(folder: &Path) -> Result<OpenProject, CoreError> {
    let path = folder.join("project.seal");
    if path.exists() {
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(raw) = parse(&text) {
                let read_only = folder_is_read_only(folder);
                // A copied folder shares its source's id: give the copy its own.
                if let Some(SealValue::String(id)) = raw.get("id") {
                    if !read_only && check_duplicate_id(id, folder).is_err() {
                        let mut next = raw.clone();
                        next.insert("id", SealValue::String(new_project_id()));
                        write_manifest(folder, &next)?;
                        return open_project(folder);
                    }
                }
                if project_from_raw(folder, raw.clone(), read_only).is_ok() {
                    return open_project(folder);
                }
                if !read_only {
                    let healed = heal_manifest(folder, &raw)?;
                    write_manifest(folder, &healed)?;
                    return open_project(folder);
                }
            }
        }
    }
    if path.exists() {
        move_aside(&path)?;
    }
    if let Ok(backup) = backup_path(&path) {
        if backup.exists() {
            if let Ok(text) = fs::read_to_string(&backup) {
                if let Ok(raw) = parse(&text) {
                    let healed = heal_manifest(folder, &raw)?;
                    write_manifest(folder, &healed)?;
                    return read_project(folder);
                }
            }
        }
    }
    let name = folder.file_name().and_then(|name| name.to_str()).unwrap_or("project");
    create_project_with_options(
        folder,
        name,
        &CreateProjectOptions { confirm_nonempty: true, ..CreateProjectOptions::default() },
    )
}

pub const RECENT_LIMIT: usize = 15;

pub fn touch_recent(path: &Path, project: &Path) -> Result<(), CoreError> {
    let mut recent = read_recent(path)?;
    let text = project.display().to_string();
    recent.retain(|entry| entry != &text);
    recent.insert(0, text);
    recent.truncate(RECENT_LIMIT);
    write_recent(path, &recent)
}

pub fn remove_recent(path: &Path, project: &Path) -> Result<(), CoreError> {
    let mut recent = read_recent(path)?;
    let text = project.display().to_string();
    recent.retain(|entry| entry != &text);
    write_recent(path, &recent)
}

fn write_recent(path: &Path, recent: &[String]) -> Result<(), CoreError> {
    let mut raw =
        SealValue::Map(vec![("schema_version".into(), SealValue::Number("1".into()))].into());
    raw.insert(
        "projects",
        SealValue::Array(recent.iter().cloned().map(SealValue::String).collect()),
    );
    atomic_write(path, raw.to_text().as_bytes())
}

pub fn read_recent(path: &Path) -> Result<Vec<String>, CoreError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let value = parse(&fs::read_to_string(path)?)?;
    let Some(SealValue::Array(items)) = value.get("projects") else {
        return Ok(Vec::new());
    };
    Ok(items
        .iter()
        .filter_map(|item| match item {
            SealValue::String(text) => Some(text.clone()),
            _ => None,
        })
        .collect())
}

fn validate_project_name(name: &str) -> Result<&str, CoreError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(CoreError::new("project name must not be empty"));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(CoreError::new("project name must not be . or .."));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(CoreError::new("project name must not contain a path separator"));
    }
    Ok(trimmed)
}

/// Returns whether a project content path matches any manifest ignore pattern.
#[cfg_attr(not(test), allow(dead_code))]
pub fn is_ignored(name: &str, patterns: &[impl AsRef<str>]) -> bool {
    let name = name.replace('\\', "/");
    patterns.iter().any(|pattern| matches_ignore_pattern(&name, pattern.as_ref()))
}

#[cfg_attr(not(test), allow(dead_code))]
fn matches_ignore_pattern(name: &str, pattern: &str) -> bool {
    let pattern = pattern.replace('\\', "/");
    if let Some(prefix) = pattern.strip_suffix('/') {
        if prefix.is_empty() {
            return false;
        }
        return name == prefix || name.starts_with(&format!("{prefix}/"));
    }
    if let Some(rest) = pattern.strip_prefix('.') {
        if rest.is_empty() {
            return name.starts_with('.') || name.split('/').any(|part| part.starts_with('.'));
        }
        return name == pattern
            || name.starts_with(&format!("{pattern}/"))
            || name.ends_with(&format!("/{pattern}"))
            || name.contains(&format!("/{pattern}/"));
    }
    name == pattern
}

fn read_project(folder: &Path) -> Result<OpenProject, CoreError> {
    let raw = parse(&fs::read_to_string(folder.join("project.seal"))?)?;
    let read_only = folder_is_read_only(folder);
    project_from_raw(folder, raw, read_only)
}

fn project_from_raw(
    folder: &Path,
    raw: SealValue,
    read_only: bool,
) -> Result<OpenProject, CoreError> {
    let name = match raw.get("name") {
        Some(SealValue::String(name)) if !name.is_empty() => name.clone(),
        _ => return Err(CoreError::new("project manifest has no name")),
    };
    let schema_version = match raw.get("schema_version") {
        Some(SealValue::Number(lexeme)) => lexeme.parse().unwrap_or(0),
        _ => 0,
    };
    if schema_version == 0 {
        return Err(CoreError::new("project manifest has no schema version"));
    }
    let id = match raw.get("id") {
        Some(SealValue::String(id)) if !id.is_empty() => id.clone(),
        _ => return Err(CoreError::new("project manifest has no id")),
    };
    if !is_packaged_game_data(folder) {
        check_duplicate_id(&id, folder)?;
    }
    let editor_version = match raw.get("editor_version") {
        Some(SealValue::String(version)) if !version.is_empty() => version.clone(),
        _ => return Err(CoreError::new("project manifest has no editor version")),
    };
    let _settings = raw.get("settings");
    let ignore = read_ignore_list(&raw);
    Ok(OpenProject {
        root: std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf()),
        name,
        schema_version,
        id,
        editor_version,
        read_only,
        ignore,
    })
}

fn read_ignore_list(raw: &SealValue) -> Vec<String> {
    let Some(SealValue::Array(items)) = raw.get("ignore") else {
        return vec![".seal-editor/".into()];
    };
    items
        .iter()
        .filter_map(|item| match item {
            SealValue::String(text) if !text.is_empty() => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn heal_manifest(folder: &Path, raw: &SealValue) -> Result<SealValue, CoreError> {
    let mut next = match raw {
        SealValue::Map(entries) => SealValue::Map(entries.clone()),
        _ => SealValue::Map(Vec::new().into()),
    };
    if next.get("name").is_none() {
        let name =
            folder.file_name().and_then(|part| part.to_str()).unwrap_or("project").to_string();
        next.insert("name", SealValue::String(name));
    }
    if next.get("schema_version").is_none() {
        next.insert("schema_version", SealValue::Number(PROJECT_SCHEMA.to_string()));
    }
    if next.get("id").is_none() {
        next.insert("id", SealValue::String(new_project_id()));
    }
    if next.get("created").is_none() {
        let created = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        next.insert("created", SealValue::Number(created.to_string()));
    }
    if next.get("editor_version").is_none() {
        next.insert(
            "editor_version",
            SealValue::String(crate::version::ENGINE_VERSION_FULL.into()),
        );
    }
    if next.get("modules").is_none() {
        next.insert("modules", SealValue::Array(Vec::new()));
    }
    if next.get("settings").is_none() {
        next.insert("settings", SealValue::Map(Vec::new().into()));
    }
    if next.get("ignore").is_none() {
        next.insert("ignore", SealValue::Array(vec![SealValue::String(".seal-editor/".into())]));
    }
    Ok(next)
}

/// Set one entry of the `settings` table in `folder`'s `project.seal`,
/// keeping everything else in the file.
pub fn set_setting(folder: &Path, key: &str, value: SealValue) -> Result<(), CoreError> {
    let path = folder.join("project.seal");
    let text = fs::read_to_string(&path)?;
    let mut raw = parse(&text).map_err(|e| CoreError::new(format!("project.seal: {e}")))?;
    let mut settings = match raw.get("settings") {
        Some(map @ SealValue::Map(_)) => map.clone(),
        _ => SealValue::Map(Vec::new().into()),
    };
    settings.insert(key, value);
    raw.insert("settings", settings);
    write_manifest(folder, &raw)
}

fn write_manifest(folder: &Path, raw: &SealValue) -> Result<(), CoreError> {
    fs::create_dir_all(folder)?;
    atomic_write(&folder.join("project.seal"), raw.to_text().as_bytes())
}

fn write_repo_hints(folder: &Path, git_lfs: bool) -> Result<(), CoreError> {
    let ignore = folder.join(".gitignore");
    if !ignore.exists() {
        fs::write(&ignore, ".seal-editor/\n")?;
    }
    if git_lfs {
        let attributes = folder.join(".gitattributes");
        if !attributes.exists() {
            let patterns = [
                "*.bin filter=lfs diff=lfs merge=lfs -text",
                "*.png filter=lfs diff=lfs merge=lfs -text",
                "*.jpg filter=lfs diff=lfs merge=lfs -text",
                "*.jpeg filter=lfs diff=lfs merge=lfs -text",
                "*.bmp filter=lfs diff=lfs merge=lfs -text",
                "*.gif filter=lfs diff=lfs merge=lfs -text",
                "*.tga filter=lfs diff=lfs merge=lfs -text",
                "*.webp filter=lfs diff=lfs merge=lfs -text",
                "*.hdr filter=lfs diff=lfs merge=lfs -text",
                "*.exr filter=lfs diff=lfs merge=lfs -text",
                "*.svg filter=lfs diff=lfs merge=lfs -text",
                "*.ktx2 filter=lfs diff=lfs merge=lfs -text",
                "*.dds filter=lfs diff=lfs merge=lfs -text",
                "*.wav filter=lfs diff=lfs merge=lfs -text",
                "*.mp3 filter=lfs diff=lfs merge=lfs -text",
                "*.ogg filter=lfs diff=lfs merge=lfs -text",
            ];
            fs::write(attributes, format!("{}\n", patterns.join("\n")))?;
        }
    }
    Ok(())
}

fn write_readme_stub(folder: &Path, name: &str) -> Result<(), CoreError> {
    let path = folder.join("README.md");
    if path.exists() {
        return Ok(());
    }
    let body = format!("# {name}\n\nCreated with the Seal editor.\n");
    fs::write(path, body)?;
    Ok(())
}

fn init_git_repo(folder: &Path) -> Result<(), CoreError> {
    let output = std::process::Command::new("git")
        .arg("init")
        .current_dir(folder)
        .output()
        .map_err(|err| CoreError::new(format!("git init failed: {err}")))?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stderr);
        return Err(CoreError::new(format!("git init failed: {text}")));
    }
    Ok(())
}

fn new_project_id() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let mut bytes = [0u8; 16];
    bytes[0..6].copy_from_slice(&(millis << 16).to_be_bytes()[2..8]);
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[7] = sequence as u8;
    let noise = millis
        ^ (std::process::id() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (millis >> 17)
        ^ sequence.wrapping_mul(0x6C62_272E_07BB_0142);
    bytes[8..16].copy_from_slice(&noise.to_be_bytes());
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format_uuid(bytes)
}

fn format_uuid(bytes: [u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

pub fn folder_is_writable(folder: &Path) -> bool {
    !folder_is_read_only(folder)
}

/// Whether a folder holds anything besides the editor's own `.seal-editor` state.
pub fn folder_has_files(folder: &Path) -> bool {
    fs::read_dir(folder)
        .map(|dir| dir.filter_map(|e| e.ok()).any(|e| e.file_name() != ".seal-editor"))
        .unwrap_or(false)
}

pub(crate) fn folder_is_read_only(folder: &Path) -> bool {
    let probe_dir = folder.join(".seal-editor");
    let existed = probe_dir.exists();
    if fs::create_dir_all(&probe_dir).is_err() {
        return true;
    }
    // The probe leaves no trace in a folder the user has not made a project yet.
    let _cleanup = Cleanup(if existed { None } else { Some(probe_dir.clone()) });
    let probe = probe_dir.join(".write-check");
    match OpenOptions::new().create(true).write(true).truncate(true).open(&probe) {
        Ok(mut file) => {
            let ok = file.write_all(b"x").is_ok() && file.sync_all().is_ok();
            let _ = fs::remove_file(&probe);
            !ok
        }
        Err(_) => true,
    }
}

fn project_index_path() -> PathBuf {
    user_state_dir().join("project-index.seal")
}

fn project_index_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|err| err.into_inner())
}

fn check_duplicate_id(id: &str, folder: &Path) -> Result<(), CoreError> {
    match registered_elsewhere(id, folder)? {
        Some(other) => {
            let messages = Messages::english();
            Err(CoreError::new(
                messages.format("duplicate-project-id", &[("id", id), ("path", &other)]),
            ))
        }
        None => Ok(()),
    }
}

/// The folder of another live project with this id, when `folder` is a copy of it.
pub fn copy_source(folder: &Path) -> Option<String> {
    let raw = parse(&fs::read_to_string(folder.join("project.seal")).ok()?).ok()?;
    let Some(SealValue::String(id)) = raw.get("id") else { return None };
    registered_elsewhere(id, folder).ok().flatten()
}

fn registered_elsewhere(id: &str, folder: &Path) -> Result<Option<String>, CoreError> {
    let _guard = project_index_lock();
    let path = project_index_path();
    let index = read_project_index(&path)?;
    let Some(SealValue::String(other)) = index.get(id) else {
        return Ok(None);
    };
    let other_path = Path::new(other);
    // A moved project leaves its old folder empty or holding another project: not a copy.
    let other_id = fs::read_to_string(other_path.join("project.seal"))
        .ok()
        .and_then(|text| parse(&text).ok())
        .and_then(|value| match value.get("id") {
            Some(SealValue::String(found)) => Some(found.clone()),
            _ => None,
        });
    if other_path == folder || other_id.as_deref() != Some(id) {
        return Ok(None);
    }
    Ok(Some(other.clone()))
}

fn register_project_id(project: &OpenProject) -> Result<(), CoreError> {
    if project.read_only {
        return Ok(());
    }
    let _guard = project_index_lock();
    let path = project_index_path();
    let mut index = read_project_index(&path)?;
    index.insert(project.id.clone(), SealValue::String(project.root.display().to_string()));
    write_project_index(&path, &index)
}

fn read_project_index(path: &Path) -> Result<SealValue, CoreError> {
    if !path.exists() {
        return Ok(SealValue::Map(
            vec![("schema_version".into(), SealValue::Number(INDEX_SCHEMA.to_string()))].into(),
        ));
    }
    let value = parse(&fs::read_to_string(path)?)?;
    match value {
        SealValue::Map(_) => Ok(value),
        _ => Ok(SealValue::Map(
            vec![("schema_version".into(), SealValue::Number(INDEX_SCHEMA.to_string()))].into(),
        )),
    }
}

fn write_project_index(path: &Path, index: &SealValue) -> Result<(), CoreError> {
    let mut next = match index {
        SealValue::Map(entries) => SealValue::Map(entries.clone()),
        _ => SealValue::Map(Vec::new().into()),
    };
    next.insert("schema_version", SealValue::Number(INDEX_SCHEMA.to_string()));
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    atomic_write(path, next.to_text().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seal-project-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_open_and_recent_roundtrip() {
        let dir = scratch("round");
        let folder = dir.join("demo");
        let created = create_project(&folder, "Demo").unwrap();
        assert_eq!(created.name, "Demo");
        assert_eq!(created.schema_version, 1);
        assert!(!created.id.is_empty());
        assert!(created.editor_version.contains("0.2+"));
        assert!(!created.read_only);
        let opened = open_project(&folder).unwrap();
        assert_eq!(opened, created);
        assert!(!folder.join(".gitignore").exists());
        if std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
        {
            let with_git = create_project_with_options(
                &dir.join("git"),
                "Git",
                &CreateProjectOptions { init_git: true, git_lfs: true, confirm_nonempty: false },
            )
            .unwrap();
            assert!(with_git.root.join(".git").exists());
            let ignore = fs::read_to_string(with_git.root.join(".gitignore")).unwrap();
            assert!(ignore.contains(".seal-editor/"));
            let attributes = fs::read_to_string(with_git.root.join(".gitattributes")).unwrap();
            assert!(attributes.contains("*.png"));
        }
        let text = fs::read_to_string(folder.join("project.seal")).unwrap();
        assert!(text.contains("settings"));
        assert!(text.contains("ignore"));
        let recent_path = dir.join("recent.seal");
        touch_recent(&recent_path, &folder).unwrap();
        touch_recent(&recent_path, &dir.join("other")).unwrap();
        touch_recent(&recent_path, &folder).unwrap();
        let recent = read_recent(&recent_path).unwrap();
        assert_eq!(recent[0], folder.display().to_string());
        assert_eq!(recent.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn damaged_manifest_can_be_repaired() {
        let dir = scratch("repair");
        let folder = dir.join("broken");
        create_project(&folder, "Broken").unwrap();
        let original = fs::read(folder.join("project.seal")).unwrap();
        fs::write(folder.join("project.seal.bak"), &original).unwrap();
        fs::write(folder.join("project.seal"), b"!!!").unwrap();
        assert!(open_project(&folder).is_err());
        let repaired = repair_project(&folder).unwrap();
        assert_eq!(repaired.name, "Broken");
        fs::write(folder.join("project.seal"), b"!!!").unwrap();
        let _ = fs::remove_file(folder.join("project.seal.bak"));
        let named = repair_project(&folder).unwrap();
        assert_eq!(named.name, "broken");
        assert!(folder.read_dir().unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".bad-")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_ids_are_rejected_when_another_copy_exists() {
        let dir = scratch("dup");
        let first = dir.join("one");
        let second = dir.join("two");
        let a = create_project(&first, "One").unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let mut raw = parse(&fs::read_to_string(first.join("project.seal")).unwrap()).unwrap();
        raw.insert("id", SealValue::String(a.id.clone()));
        raw.insert("name", SealValue::String("Two".into()));
        write_manifest(&second, &raw).unwrap();
        let err = open_project(&second).unwrap_err();
        assert!(err.to_string().contains(&a.id));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignore_patterns_match_directories_hidden_names_and_exact_paths() {
        let patterns = [".seal-editor/", ".git", "secret"];
        assert!(is_ignored(".seal-editor", &patterns));
        assert!(is_ignored(".seal-editor/cache", &patterns));
        assert!(is_ignored(".git/config", &patterns));
        assert!(is_ignored("secret", &patterns));
        assert!(!is_ignored("secret/notes", &patterns));
        assert!(is_ignored("assets/.hidden", &["."]));
        assert!(!is_ignored("assets/readme", &["."]));
    }
}

/// Removes a probe folder on drop, only if it is still empty.
struct Cleanup(Option<std::path::PathBuf>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(dir) = &self.0 {
            let _ = fs::remove_dir(dir);
        }
    }
}

#[cfg(test)]
mod moved_project_tests {
    #[test]
    fn a_moved_project_opens_and_a_true_copy_is_refused() {
        let dir = std::env::temp_dir().join(format!("seal-moved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = dir.join("first");
        let made = super::create_project(&first, "Game").unwrap();
        // A copy beside the original keeps the same id: refused.
        let copy = dir.join("copy");
        std::fs::create_dir_all(&copy).unwrap();
        std::fs::copy(first.join("project.seal"), copy.join("project.seal")).unwrap();
        assert!(super::check_duplicate_id(&made.id, &copy).is_err());
        assert!(super::copy_source(&copy).is_some());
        // Repair gives the copy its own id; the original keeps its id.
        let repaired = super::repair_project(&copy).unwrap();
        assert_ne!(repaired.id, made.id);
        assert_eq!(super::open_project(&first).unwrap().id, made.id);
        std::fs::remove_dir_all(&copy).unwrap();
        // Moving the original away (its old folder now holds another project) is allowed.
        let moved = dir.join("moved");
        std::fs::rename(&first, &moved).unwrap();
        super::create_project(&first, "Other").unwrap();
        assert!(super::check_duplicate_id(&made.id, &moved).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod probe_tests {
    #[test]
    fn the_write_probe_leaves_an_empty_folder_empty() {
        let dir = std::env::temp_dir().join(format!("seal-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(super::folder_is_writable(&dir));
        assert!(super::folder_is_writable(&dir));
        assert!(!super::folder_has_files(&dir));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::create_dir_all(dir.join(".seal-editor")).unwrap();
        assert!(!super::folder_has_files(&dir), "editor state alone is not user files");
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        assert!(super::folder_has_files(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
