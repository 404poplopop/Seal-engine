//! Export Game: the editor program, renamed after the game, plus the project's
//! files, laid out as a folder players can run. A `game.txt` beside the program
//! makes it open straight into play instead of the editor. Small pictures are
//! also packed onto shared atlas pages (see [`crate::atlas`]).

use crate::error::CoreError;
use std::path::{Path, PathBuf};

/// Marks a packaged game; it sits beside the program and names the project folder.
pub const GAME_MARKER: &str = "game.txt";
pub const MANIFEST: &str = "manifest.txt";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageReport {
    pub folder: PathBuf,
    pub exe: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

/// The project a packaged program should play, when `exe` is one.
/// Start scene line in `game.txt` when the project lives in a packaged game's `data/` folder.
pub fn packaged_start_scene(project_root: &Path) -> Option<String> {
    let dir = project_root.parent()?;
    let text = std::fs::read_to_string(dir.join(GAME_MARKER)).ok()?;
    if !dir.join(GAME_MARKER).is_file() {
        return None;
    }
    text.lines()
        .find_map(|line| line.trim().strip_prefix("start ").map(str::trim).map(str::to_string))
        .filter(|s| !s.is_empty())
}

pub fn game_project_beside(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let text = std::fs::read_to_string(dir.join(GAME_MARKER)).ok()?;
    let folder = text.lines().find_map(|l| l.trim().strip_prefix("project ").map(str::trim))?;
    if let Some(pack) = folder.strip_prefix("pack ") {
        let pack_path = dir.join(pack.trim());
        return crate::game_pack::mount_pack(dir, &pack_path).ok();
    }
    let project = dir.join(folder);
    project.join("project.seal").is_file().then_some(project)
}

/// Default destination: a `<Name> Game` folder next to the project folder.
pub fn default_package_folder(project: &Path, name: &str) -> PathBuf {
    let parent = project.parent().unwrap_or(project);
    parent.join(format!("{} Game", safe_name(name)))
}

fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        "Game".into()
    } else {
        cleaned
    }
}

/// Skipped when copying: editor state, version control, earlier packages and
/// the open project's lock file, which Windows keeps locked while it is open.
/// Left out of a package: hidden and editor files, earlier packages, build output
/// and the editor's `.bak` safety copies.
fn skip(name: &str) -> bool {
    name.starts_with('.')
        || name.ends_with(" Game")
        || name.ends_with(".bak")
        || name == "target"
        || name == "project.lock"
}

fn copy_tree(
    project: &Path,
    from: &Path,
    to: &Path,
    rel: &str,
    allow: &dyn Fn(&str) -> bool,
    files: &mut usize,
    bytes: &mut u64,
) -> Result<(), CoreError> {
    std::fs::create_dir_all(to)?;
    let mut entries: Vec<_> = std::fs::read_dir(from)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    let mut file_jobs = Vec::new();
    let mut dirs = Vec::new();
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if skip(&name) {
            continue;
        }
        let path = entry.path();
        let child = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        if path.is_dir() {
            dirs.push((path, name, child));
        } else if allow(&child) {
            file_jobs.push((path, to.join(&name), child));
        }
    }
    let copied = crate::editor_tools::map_in_order(
        crate::editor_tools::active_workers(),
        file_jobs,
        |(src, dest, child)| {
            if crate::sound_export::is_sound_rel(&child) {
                let raw = std::fs::read(&src)?;
                let bytes = crate::sound_export::package_bytes(project, &child, &raw)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let len = bytes.len() as u64;
                std::fs::write(&dest, bytes)?;
                return Ok(len);
            }
            std::fs::copy(src, dest)
        },
    );
    for result in copied {
        *bytes += result?;
        *files += 1;
    }
    for (path, name, child) in dirs {
        copy_tree(project, &path, &to.join(&name), &child, allow, files, bytes)?;
        if dir_is_empty(&to.join(&name)) {
            let _ = std::fs::remove_dir(to.join(&name));
        }
    }
    Ok(())
}

fn dir_is_empty(path: &Path) -> bool {
    std::fs::read_dir(path).ok().and_then(|mut dir| dir.next()).is_none()
}

/// Hash a packaged file in small pieces so the program itself does not sit in memory.
fn fnv_file(path: &Path) -> std::io::Result<(u64, u64)> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut len = 0u64;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf)?;
        if n == 0 {
            break;
        }
        len += n as u64;
        hash = fnv_continue(hash, &buf[..n]);
    }
    Ok((hash, len))
}

fn fnv_continue(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |h, b| (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3))
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// Copy `exe` and the project at `project` into `out` (replacing an earlier package there).
pub fn package_project(
    project: &Path,
    name: &str,
    exe: &Path,
    out: &Path,
) -> Result<PackageReport, CoreError> {
    package_filtered(project, name, exe, out, &|_| true, &[], true, false, "", "", None, "")
}

/// A file copied into the package in addition to the project.
#[derive(Clone, Debug)]
pub struct ExtraCopy {
    pub from: PathBuf,
    pub to: String,
}

/// Copy a project with include filtering, extra files, and an optional window file.
///
/// `allow` sees each project-relative path (`sprites/a.png`). `clean` deletes an
/// earlier package in `out` first. `staging` keeps a folder that does not yet
/// have `game.txt` (a pre-export step just filled it).
#[allow(clippy::too_many_arguments)]
pub fn package_filtered(
    project: &Path,
    name: &str,
    exe: &Path,
    out: &Path,
    allow: &dyn Fn(&str) -> bool,
    extra: &[ExtraCopy],
    clean: bool,
    staging: bool,
    profile: &str,
    window_text: &str,
    export_platform: Option<(crate::import_platform::ImportPlatform, u32)>,
    render_text: &str,
) -> Result<PackageReport, CoreError> {
    package_filtered_with(
        project,
        name,
        exe,
        out,
        allow,
        extra,
        clean,
        staging,
        profile,
        window_text,
        export_platform,
        render_text,
        &crate::atlas::AtlasOptions::default(),
        false,
    )
}

/// [`package_filtered`] with the sprite-page options from an export preset.
#[allow(clippy::too_many_arguments)]
pub fn package_filtered_with(
    project: &Path,
    name: &str,
    exe: &Path,
    out: &Path,
    allow: &dyn Fn(&str) -> bool,
    extra: &[ExtraCopy],
    clean: bool,
    staging: bool,
    profile: &str,
    window_text: &str,
    export_platform: Option<(crate::import_platform::ImportPlatform, u32)>,
    render_text: &str,
    atlas: &crate::atlas::AtlasOptions,
    single_archive: bool,
) -> Result<PackageReport, CoreError> {
    if out.starts_with(project) {
        return Err(CoreError::new("the package folder cannot be inside the project"));
    }
    if out.exists() {
        let marker = out.join(GAME_MARKER).is_file();
        if !marker && !staging {
            return Err(CoreError::new(format!(
                "{} already exists and is not a packaged game",
                out.display()
            )));
        }
        if clean {
            std::fs::remove_dir_all(out)?;
        }
    }
    std::fs::create_dir_all(out)?;
    let ext = exe.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let game_exe = out.join(format!("{}{ext}", safe_name(name)));
    let mut files = 1;
    let mut bytes = std::fs::copy(exe, &game_exe)?;
    let project_layout = if single_archive { "project pack data.pack" } else { "project data" };
    let data_notes = if single_archive {
        "The game's files are in data.pack.\n"
    } else {
        "The game's files are in data/.\n"
    };
    // Single archive: build the data folder in a staging place, then pack it into data.pack.
    let data = if single_archive {
        std::env::temp_dir().join(format!(
            "seal-pack-staging-{}-{}",
            std::process::id(),
            safe_name(name)
        ))
    } else {
        out.join("data")
    };
    if single_archive {
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data)?;
    }
    copy_tree(project, project, &data, "", allow, &mut files, &mut bytes)?;
    // Small pictures go onto shared atlas pages; the originals stay for anything reading pixels.
    // The target platform's page size (import presets) caps the preset's Max page size.
    let mut atlas_options = atlas.clone();
    if let Some((platform, page_size)) = export_platform {
        let _ = crate::import_platform::apply_pictures_for_platform(project, &data, platform)?;
        if page_size > 0 {
            atlas_options.page_size = atlas_options.page_size.min(page_size);
        }
    }
    if crate::atlas::build_with(&data, &atlas_options)? > 0 {
        if let Some(atlas) = crate::atlas::Atlas::load(&data) {
            files += atlas.pages.len() + 1;
        }
    }
    crate::picture_pack::apply_package(&data)?;
    let platform = export_platform
        .map(|(platform, _)| platform)
        .unwrap_or(crate::import_platform::ImportPlatform::Windows);
    crate::texture_ktx_export::apply_package(project, &data, platform)?;
    if single_archive {
        let mut packed = Vec::new();
        walk(&data, &data, &mut packed)?;
        packed.sort();
        let pack = crate::game_pack::write_pack(&data, &packed)?;
        bytes += pack.len() as u64;
        std::fs::write(out.join("data.pack"), &pack)?;
        files += 1;
        let _ = std::fs::remove_dir_all(&data);
    }
    for item in extra {
        let dest = out.join(&item.to);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        bytes += std::fs::copy(&item.from, &dest)?;
        files += 1;
    }
    let start_scene = crate::build_settings::read_build_scenes(project)
        .ok()
        .and_then(|s| s.into_iter().next())
        .unwrap_or_else(|| "scene.seal".into());
    let mut marker = format!("seal game\nname {name}\n{project_layout}\nstart {start_scene}\n");
    if !profile.is_empty() {
        marker.push_str(&format!("profile {profile}\n"));
    }
    std::fs::write(out.join(GAME_MARKER), marker)?;
    let exe_name =
        game_exe.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    std::fs::write(
        out.join("README.txt"),
        format!("{name}\n\nRun {exe_name} to play. Press Esc to quit.\n{data_notes}Made with the Seal engine.\n"),
    )?;
    if !window_text.is_empty() {
        std::fs::write(out.join(crate::window_game::WINDOW_FILE), window_text)?;
        files += 1;
    }
    if !render_text.is_empty() {
        std::fs::write(out.join(crate::render_quality::RENDER_FILE), render_text)?;
        files += 1;
    }
    let mut list = Vec::new();
    walk(out, out, &mut list)?;
    list.retain(|f| f != MANIFEST);
    list.sort();
    let mut manifest = format!("seal package {name}\n");
    for file in &list {
        let (hash, len) = fnv_file(&out.join(file))?;
        manifest += &format!("{hash:016x} {len} {file}\n");
    }
    std::fs::write(out.join(MANIFEST), manifest)?;
    Ok(PackageReport { folder: out.to_path_buf(), exe: game_exe, files: files + 3, bytes })
}
