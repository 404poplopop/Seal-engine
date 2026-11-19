//! Named export presets stored in the project's settings.
//!
//! A preset names the output folder, the build profile, which project files
//! to keep, extra files to copy, whether to clear stale files, and the
//! built-in steps that run before and after the copy. The only steps are
//! copying a file and zipping a folder.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use seal_sealtext::{SealMap, SealValue};

use crate::error::CoreError;
use crate::package::{self, ExtraCopy, PackageReport};

const KEY: &str = "export.presets";

/// One preset the Export panel edits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportPreset {
    pub name: String,
    pub folder: String,
    pub profile: String,
    /// Target platform for per-platform import overrides during export.
    pub platform: String,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub extra: Vec<(String, String)>,
    pub clean: bool,
    pub pre: Vec<ExportStep>,
    pub post: Vec<ExportStep>,
    /// Pack pictures by their opaque bounds.
    pub atlas_tight: bool,
    /// Turn a picture a quarter turn when that fits the page.
    pub atlas_rotate: bool,
    /// Largest sprite-page side for this preset, in pixels.
    pub atlas_page: u32,
    /// Packed sprite size as a percent of the picture. 100 is full size.
    pub atlas_scale: u32,
    pub single_archive: bool,
    /// `folder` or `msix`.
    pub package_target: String,
    /// MSIX publisher (`CN=...`, from the store account) and four-part package version.
    pub msix_publisher: String,
    pub msix_version: String,
}

/// A built-in step. `copy` copies one file. `zip` writes a zip of a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportStep {
    Copy { from: String, to: String },
    Zip { folder: String },
}

/// The presets saved with a project, and which one Export uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportBook {
    pub selected: String,
    pub presets: Vec<ExportPreset>,
}

impl Default for ExportBook {
    fn default() -> Self {
        Self { selected: String::new(), presets: Vec::new() }
    }
}

impl ExportBook {
    pub fn selected_preset(&self) -> Option<&ExportPreset> {
        if self.presets.is_empty() {
            return None;
        }
        self.presets
            .iter()
            .find(|preset| preset.name == self.selected)
            .or_else(|| self.presets.first())
    }
}

impl ExportPreset {
    pub fn named(name: &str, folder: &str) -> Self {
        Self {
            name: name.to_string(),
            folder: folder.to_string(),
            profile: "release".into(),
            platform: "windows".into(),
            include: Vec::new(),
            exclude: Vec::new(),
            extra: Vec::new(),
            clean: true,
            pre: Vec::new(),
            post: Vec::new(),
            atlas_tight: false,
            atlas_rotate: false,
            atlas_page: crate::atlas::PAGE_SIZE,
            atlas_scale: 100,
            single_archive: false,
            package_target: "folder".into(),
            msix_publisher: MSIX_PUBLISHER.into(),
            msix_version: MSIX_VERSION.into(),
        }
    }

    pub fn atlas_options(&self) -> crate::atlas::AtlasOptions {
        crate::atlas::AtlasOptions {
            tight: self.atlas_tight,
            rotate: self.atlas_rotate,
            page_size: self.atlas_page,
            scale_percent: self.atlas_scale,
        }
    }
}

/// Project files that this preset ships, plus extra files, as paths inside the package.
pub fn planned_files(project_files: &[String], preset: &ExportPreset) -> Vec<String> {
    let mut out = Vec::new();
    for rel in project_files {
        let rel = rel.replace('\\', "/");
        if rel.is_empty() || !included(&rel, preset) {
            continue;
        }
        out.push(format!("data/{rel}"));
    }
    for (_, to) in &preset.extra {
        let to = to.replace('\\', "/").trim_matches('/').to_string();
        if !to.is_empty() {
            out.push(to);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Paths present in `existing` that this export will not write.
pub fn stale_files(existing: &[String], planned: &[String]) -> Vec<String> {
    let keep: BTreeSet<&str> = planned.iter().map(String::as_str).collect();
    let mut stale: Vec<String> =
        existing.iter().filter(|path| !keep.contains(path.as_str())).cloned().collect();
    stale.sort();
    stale
}

/// Delete files under `root` whose relative path is not in `keep`.
/// Returns the relative paths that were removed.
pub fn remove_stale(root: &Path, keep: &[String]) -> Result<Vec<String>, CoreError> {
    let keep: BTreeSet<&str> = keep.iter().map(String::as_str).collect();
    let mut found = Vec::new();
    if root.is_dir() {
        let mut list = Vec::new();
        walk_files(root, root, &mut list)?;
        for rel in list {
            if !keep.contains(rel.as_str()) {
                found.push(rel);
            }
        }
    }
    found.sort();
    for rel in &found {
        let path = root.join(rel);
        if path.is_file() {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(found)
}

fn walk_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), CoreError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk_files(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn included(rel: &str, preset: &ExportPreset) -> bool {
    let include: Vec<&str> =
        preset.include.iter().map(String::as_str).filter(|pat| !pat.is_empty()).collect();
    if !include.is_empty() && !include.iter().any(|pat| glob_match(pat, rel)) {
        return false;
    }
    !preset.exclude.iter().any(|pat| !pat.is_empty() && glob_match(pat, rel))
}

/// `*` stays inside one path segment. `**` crosses segments. A pattern with no
/// slash matches any segment, so `*.seal` matches `scenes/a.seal`.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let path = path.trim_matches('/');
    let pattern = pattern.trim().trim_matches('/');
    if pattern.is_empty() || path.is_empty() {
        return false;
    }
    if !pattern.contains('/') {
        return path.split('/').any(|segment| segment_match(pattern, segment));
    }
    let pat: Vec<&str> = pattern.split('/').collect();
    let text: Vec<&str> = path.split('/').collect();
    match_parts(&pat, &text)
}

fn match_parts(pat: &[&str], text: &[&str]) -> bool {
    if pat.is_empty() {
        return text.is_empty();
    }
    if pat[0] == "**" {
        if match_parts(&pat[1..], text) {
            return true;
        }
        return !text.is_empty() && match_parts(pat, &text[1..]);
    }
    !text.is_empty() && segment_match(pat[0], text[0]) && match_parts(&pat[1..], &text[1..])
}

fn segment_match(pattern: &str, segment: &str) -> bool {
    segment_rec(pattern.as_bytes(), segment.as_bytes())
}

fn segment_rec(pattern: &[u8], segment: &[u8]) -> bool {
    if pattern.is_empty() {
        return segment.is_empty();
    }
    if pattern[0] == b'*' {
        if segment_rec(&pattern[1..], segment) {
            return true;
        }
        return !segment.is_empty() && segment_rec(pattern, &segment[1..]);
    }
    if pattern[0] == b'?' {
        return !segment.is_empty() && segment_rec(&pattern[1..], &segment[1..]);
    }
    !segment.is_empty() && pattern[0] == segment[0] && segment_rec(&pattern[1..], &segment[1..])
}

pub fn to_text(book: &ExportBook) -> String {
    let mut presets = Vec::new();
    for preset in &book.presets {
        let mut map = Vec::new();
        map.push(("name".into(), SealValue::String(preset.name.clone())));
        map.push(("folder".into(), SealValue::String(preset.folder.clone())));
        map.push(("profile".into(), SealValue::String(preset.profile.clone())));
        map.push(("platform".into(), SealValue::String(preset.platform.clone())));
        map.push(("clean".into(), SealValue::Bool(preset.clean)));
        map.push(("atlas_tight".into(), SealValue::Bool(preset.atlas_tight)));
        map.push(("atlas_rotate".into(), SealValue::Bool(preset.atlas_rotate)));
        map.push(("atlas_page".into(), SealValue::Number(preset.atlas_page.to_string())));
        map.push(("atlas_scale".into(), SealValue::Number(preset.atlas_scale.to_string())));
        map.push(("include".into(), string_array(&preset.include)));
        map.push(("exclude".into(), string_array(&preset.exclude)));
        map.push((
            "extra".into(),
            SealValue::Array(
                preset
                    .extra
                    .iter()
                    .map(|(from, to)| {
                        SealValue::Map(SealMap::new(vec![
                            ("from".into(), SealValue::String(from.clone())),
                            ("to".into(), SealValue::String(to.clone())),
                        ]))
                    })
                    .collect(),
            ),
        ));
        map.push(("pre".into(), step_array(&preset.pre)));
        map.push(("post".into(), step_array(&preset.post)));
        map.push(("single_archive".into(), SealValue::Bool(preset.single_archive)));
        map.push(("package_target".into(), SealValue::String(preset.package_target.clone())));
        // Written only when changed, so presets saved before these fields stay identical.
        if preset.msix_publisher != MSIX_PUBLISHER {
            map.push(("msix_publisher".into(), SealValue::String(preset.msix_publisher.clone())));
        }
        if preset.msix_version != MSIX_VERSION {
            map.push(("msix_version".into(), SealValue::String(preset.msix_version.clone())));
        }
        presets.push(SealValue::Map(SealMap::new(map)));
    }
    let root = SealValue::Map(SealMap::new(vec![
        ("selected".into(), SealValue::String(book.selected.clone())),
        ("presets".into(), SealValue::Array(presets)),
    ]));
    root.to_text()
}

fn string_array(items: &[String]) -> SealValue {
    SealValue::Array(items.iter().cloned().map(SealValue::String).collect())
}

fn step_array(steps: &[ExportStep]) -> SealValue {
    SealValue::Array(
        steps
            .iter()
            .map(|step| match step {
                ExportStep::Copy { from, to } => SealValue::Map(SealMap::new(vec![
                    ("action".into(), SealValue::String("copy".into())),
                    ("from".into(), SealValue::String(from.clone())),
                    ("to".into(), SealValue::String(to.clone())),
                ])),
                ExportStep::Zip { folder } => SealValue::Map(SealMap::new(vec![
                    ("action".into(), SealValue::String("zip".into())),
                    ("folder".into(), SealValue::String(folder.clone())),
                ])),
            })
            .collect(),
    )
}

/// Read a saved book. A damaged file returns the reason.
pub fn parse_book(text: &str) -> Result<ExportBook, String> {
    if text.trim().is_empty() {
        return Ok(ExportBook::default());
    }
    let value = seal_sealtext::parse(text).map_err(|err| format!("export presets: {err}"))?;
    let map = value.as_map().ok_or("export presets must be a map")?;
    for (key, _) in map {
        if key != "selected" && key != "presets" {
            return Err(format!("unknown export setting {key}"));
        }
    }
    let selected = match value.get("selected") {
        Some(SealValue::String(text)) => text.clone(),
        Some(_) => return Err("selected must be the preset name".into()),
        None => String::new(),
    };
    let presets = match value.get("presets") {
        None => Vec::new(),
        Some(SealValue::Array(items)) => {
            let mut presets = Vec::new();
            for item in items {
                presets.push(parse_preset(item)?);
            }
            presets
        }
        Some(_) => return Err("presets must be a list".into()),
    };
    let mut names = BTreeSet::new();
    for preset in &presets {
        if preset.name.trim().is_empty() {
            return Err("an export preset needs a name".into());
        }
        if !names.insert(preset.name.clone()) {
            return Err(format!("two export presets are named {}", preset.name));
        }
    }
    if !selected.is_empty() && !presets.iter().any(|preset| preset.name == selected) {
        return Err(format!("selected preset {selected} does not exist"));
    }
    Ok(ExportBook { selected, presets })
}

fn parse_preset(value: &SealValue) -> Result<ExportPreset, String> {
    let map = value.as_map().ok_or("a preset must be a map")?;
    for (key, _) in map {
        if !matches!(
            key.as_str(),
            "name"
                | "folder"
                | "profile"
                | "platform"
                | "clean"
                | "include"
                | "exclude"
                | "extra"
                | "pre"
                | "post"
                | "atlas_tight"
                | "atlas_rotate"
                | "atlas_page"
                | "atlas_scale"
                | "single_archive"
                | "package_target"
                | "msix_publisher"
                | "msix_version"
        ) {
            return Err(format!("unknown preset field {key}"));
        }
    }
    let name = need_string(value, "name")?;
    let folder = optional_string(value, "folder")?;
    let profile = match value.get("profile") {
        None => "release".to_string(),
        Some(SealValue::String(text)) => match text.as_str() {
            "release" | "debug" => text.clone(),
            other => return Err(format!("build profile is release or debug, not {other}")),
        },
        Some(_) => return Err("build profile must be release or debug".into()),
    };
    let platform = match value.get("platform") {
        None => "windows".to_string(),
        Some(SealValue::String(text)) => {
            let text = text.trim().to_ascii_lowercase();
            if crate::import_platform::ImportPlatform::from_id(&text).is_some() {
                text
            } else {
                return Err(format!("target platform is not recognised: {text}"));
            }
        }
        Some(_) => return Err("target platform must be text".into()),
    };
    let clean = match value.get("clean") {
        None => true,
        Some(SealValue::Bool(value)) => *value,
        Some(_) => return Err("clean must be true or false".into()),
    };
    let single_archive = match value.get("single_archive") {
        None => false,
        Some(SealValue::Bool(v)) => *v,
        Some(_) => return Err("single_archive must be true or false".into()),
    };
    let package_target = match value.get("package_target") {
        None => "folder".to_string(),
        Some(SealValue::String(text)) => match text.as_str() {
            "folder" | "msix" => text.clone(),
            other => return Err(format!("package target is folder or msix, not {other}")),
        },
        Some(_) => return Err("package target must be text".into()),
    };
    Ok(ExportPreset {
        name,
        folder,
        profile,
        platform,
        include: string_list(value, "include")?,
        exclude: string_list(value, "exclude")?,
        extra: extra_list(value)?,
        clean,
        pre: step_list(value, "pre")?,
        post: step_list(value, "post")?,
        atlas_tight: optional_bool(value, "atlas_tight", false)?,
        atlas_rotate: optional_bool(value, "atlas_rotate", false)?,
        atlas_page: optional_page(value)?,
        atlas_scale: optional_scale(value)?,
        single_archive,
        package_target,
        msix_publisher: match value.get("msix_publisher") {
            None => MSIX_PUBLISHER.into(),
            Some(SealValue::String(text)) => text.clone(),
            Some(_) => return Err("msix_publisher must be text".into()),
        },
        msix_version: match value.get("msix_version") {
            None => MSIX_VERSION.into(),
            Some(SealValue::String(text)) if valid_msix_version(text) => text.clone(),
            Some(_) => return Err("msix_version must be four numbers like 1.0.0.0".into()),
        },
    })
}

pub const MSIX_PUBLISHER: &str = "CN=Seal";
pub const MSIX_VERSION: &str = "1.0.0.0";

/// A package version is four whole numbers from 0 to 65535 separated by dots.
pub fn valid_msix_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 4 && parts.iter().all(|p| !p.is_empty() && p.parse::<u16>().is_ok())
}

fn optional_bool(value: &SealValue, key: &str, default: bool) -> Result<bool, String> {
    match value.get(key) {
        None => Ok(default),
        Some(SealValue::Bool(flag)) => Ok(*flag),
        Some(_) => Err(format!("{key} must be true or false")),
    }
}

fn optional_page(value: &SealValue) -> Result<u32, String> {
    match value.get("atlas_page") {
        None => Ok(crate::atlas::PAGE_SIZE),
        Some(SealValue::Number(text)) => {
            let page: u32 =
                text.parse().map_err(|_| String::from("atlas page size must be a whole number"))?;
            if page.is_power_of_two() && (1024..=8192).contains(&page) {
                Ok(page)
            } else {
                Err("atlas page size is 1024, 2048, 4096, or 8192".into())
            }
        }
        Some(_) => Err("atlas page size must be a number".into()),
    }
}

fn optional_scale(value: &SealValue) -> Result<u32, String> {
    match value.get("atlas_scale") {
        None => Ok(100),
        Some(SealValue::Number(text)) => {
            let scale: u32 =
                text.parse().map_err(|_| String::from("atlas scale must be a whole number"))?;
            if (1..=100).contains(&scale) {
                Ok(scale)
            } else {
                Err("atlas scale is a percent from 1 to 100".into())
            }
        }
        Some(_) => Err("atlas scale must be a number".into()),
    }
}

fn need_string(value: &SealValue, key: &str) -> Result<String, String> {
    match value.get(key) {
        Some(SealValue::String(text)) if !text.trim().is_empty() => Ok(text.trim().to_string()),
        Some(SealValue::String(_)) => Err(format!("{key} is empty")),
        Some(_) => Err(format!("{key} must be text")),
        None => Err(format!("missing {key}")),
    }
}

fn optional_string(value: &SealValue, key: &str) -> Result<String, String> {
    match value.get(key) {
        None => Ok(String::new()),
        Some(SealValue::String(text)) => Ok(text.clone()),
        Some(_) => Err(format!("{key} must be text")),
    }
}

fn string_list(value: &SealValue, key: &str) -> Result<Vec<String>, String> {
    match value.get(key) {
        None => Ok(Vec::new()),
        Some(SealValue::Array(items)) => items
            .iter()
            .map(|item| match item {
                SealValue::String(text) => Ok(text.clone()),
                _ => Err(format!("{key} entries must be text")),
            })
            .collect(),
        Some(_) => Err(format!("{key} must be a list")),
    }
}

fn extra_list(value: &SealValue) -> Result<Vec<(String, String)>, String> {
    match value.get("extra") {
        None => Ok(Vec::new()),
        Some(SealValue::Array(items)) => items
            .iter()
            .map(|item| {
                let from = need_string(item, "from")?;
                let to = need_string(item, "to")?;
                if let Some(map) = item.as_map() {
                    for (key, _) in map {
                        if key != "from" && key != "to" {
                            return Err(format!("unknown extra file field {key}"));
                        }
                    }
                } else {
                    return Err("an extra file must be a map".into());
                }
                Ok((from, to))
            })
            .collect(),
        Some(_) => Err("extra must be a list".into()),
    }
}

fn step_list(value: &SealValue, key: &str) -> Result<Vec<ExportStep>, String> {
    match value.get(key) {
        None => Ok(Vec::new()),
        Some(SealValue::Array(items)) => items.iter().map(|item| parse_step(item, key)).collect(),
        Some(_) => Err(format!("{key} must be a list")),
    }
}

fn parse_step(value: &SealValue, key: &str) -> Result<ExportStep, String> {
    let action = need_string(value, "action")?;
    match action.as_str() {
        "copy" => {
            let from = need_string(value, "from")?;
            let to = need_string(value, "to")?;
            Ok(ExportStep::Copy { from, to })
        }
        "zip" => {
            let folder = optional_string(value, "folder")?;
            Ok(ExportStep::Zip { folder })
        }
        other => Err(format!("{key} action is copy or zip, not {other}")),
    }
}

pub fn load_book(root: &Path) -> Result<ExportBook, String> {
    let Ok(text) = std::fs::read_to_string(root.join("project.seal")) else {
        return Ok(ExportBook::default());
    };
    let Ok(raw) = seal_sealtext::parse(&text) else {
        return Err("project.seal could not be read".into());
    };
    let Some(settings) = raw.get("settings") else {
        return Ok(ExportBook::default());
    };
    match settings.get(KEY) {
        None => Ok(ExportBook::default()),
        Some(SealValue::String(body)) => parse_book(body),
        Some(_) => Err("export presets must be text".into()),
    }
}

pub fn store_book(root: &Path, book: &ExportBook) -> Result<(), CoreError> {
    let text = to_text(book);
    parse_book(&text).map_err(CoreError::new)?;
    crate::project::set_setting(root, KEY, SealValue::String(text))
}

/// Run one built-in step. Relative paths start at `project`. A zip with an
/// empty folder zips `export_dir`.
pub fn run_step(step: &ExportStep, project: &Path, export_dir: &Path) -> Result<(), CoreError> {
    match step {
        ExportStep::Copy { from, to } => {
            let source = resolve_under(project, from);
            if !source.is_file() {
                return Err(CoreError::new(format!("copy could not find {}", source.display())));
            }
            let dest = resolve_under(export_dir, to);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&source, &dest)?;
            Ok(())
        }
        ExportStep::Zip { folder } => {
            let dir = if folder.trim().is_empty() {
                export_dir.to_path_buf()
            } else {
                resolve_under(project, folder)
            };
            if !dir.is_dir() {
                return Err(CoreError::new(format!("zip could not find {}", dir.display())));
            }
            let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("export");
            let dest = dir.parent().unwrap_or(dir.as_path()).join(format!("{name}.zip"));
            write_zip(&dir, &dest)?;
            Ok(())
        }
    }
}

fn resolve_under(base: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

pub fn resolve_folder(project: &Path, folder: &str, name: &str) -> PathBuf {
    if folder.trim().is_empty() {
        package::default_package_folder(project, name)
    } else {
        resolve_under(project, folder.trim())
    }
}

/// Export `preset`, then run its post steps. Pre steps run after a clean and
/// before the copy, into a folder this export owns.
pub fn export_preset(
    project: &Path,
    project_name: &str,
    exe: &Path,
    preset: &ExportPreset,
    window_text: &str,
    render_text: &str,
) -> Result<PackageReport, CoreError> {
    let out = resolve_folder(project, &preset.folder, project_name);
    if out.starts_with(project) {
        return Err(CoreError::new("the package folder cannot be inside the project"));
    }
    if out.exists() && !out.join(package::GAME_MARKER).is_file() && !dir_empty(&out) {
        return Err(CoreError::new(format!(
            "{} already exists and is not a packaged game",
            out.display()
        )));
    }
    if preset.clean && out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    std::fs::create_dir_all(&out)?;
    for step in &preset.pre {
        run_step(step, project, &out)?;
    }
    let allow = |rel: &str| included(rel, preset);
    let extra = preset
        .extra
        .iter()
        .map(|(from, to)| ExtraCopy {
            from: resolve_under(project, from),
            to: to.replace('\\', "/"),
        })
        .collect::<Vec<_>>();
    let platform = crate::import_platform::ImportPlatform::from_id(&preset.platform)
        .unwrap_or(crate::import_platform::ImportPlatform::Windows);
    let book = crate::import_presets::load_book(project);
    let atlas_page = crate::import_platform::atlas_page_size(&book, platform);
    let report = package::package_filtered_with(
        project,
        project_name,
        exe,
        &out,
        &allow,
        &extra,
        false,
        true,
        &preset.profile,
        window_text,
        Some((platform, atlas_page)),
        render_text,
        &preset.atlas_options(),
        preset.single_archive,
    )?;
    if preset.package_target == "msix" {
        let exe_name = report.exe.file_name().and_then(|name| name.to_str()).unwrap_or("Game.exe");
        let layout = out.join("msix_layout");
        let publisher = if preset.msix_publisher.trim().is_empty() { MSIX_PUBLISHER } else { preset.msix_publisher.trim() };
        let version =
            if valid_msix_version(&preset.msix_version) { preset.msix_version.as_str() } else { MSIX_VERSION };
        crate::msix_pack::write_layout(&layout, project_name, publisher, version, exe_name)?;
        std::fs::copy(&report.exe, layout.join(exe_name))?;
        if out.join("data").is_dir() {
            copy_tree_to(&out.join("data"), &layout.join("data"))?;
        }
        if out.join("data.pack").is_file() {
            std::fs::copy(out.join("data.pack"), layout.join("data.pack"))?;
        }
        std::fs::copy(out.join(package::GAME_MARKER), layout.join(package::GAME_MARKER))?;
        for name in ["window.txt", "README.txt"] {
            let from = out.join(name);
            if from.is_file() {
                std::fs::copy(&from, layout.join(name))?;
            }
        }
        let msix_path = out.join(format!("{project_name}.msix"));
        if let Err(message) = crate::msix_pack::try_build_msix(&layout, &msix_path) {
            std::fs::write(out.join("msix-build.txt"), format!("{message}\n"))?;
        }
    }
    for step in &preset.post {
        run_step(step, project, &out)?;
    }
    Ok(report)
}

fn dir_empty(path: &Path) -> bool {
    std::fs::read_dir(path).ok().and_then(|mut dir| dir.next()).is_none()
}

fn copy_tree_to(from: &Path, to: &Path) -> Result<(), CoreError> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let dest = to.join(name);
        if path.is_dir() {
            copy_tree_to(&path, &dest)?;
        } else {
            std::fs::copy(path, dest)?;
        }
    }
    Ok(())
}

pub fn launch_game(exe: &Path) -> Result<u32, CoreError> {
    let mut command = std::process::Command::new(exe);
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    let child = command
        .spawn()
        .map_err(|err| CoreError::new(format!("could not start the game: {err}")))?;
    Ok(child.id())
}

/// Start the exported program. Tests use this to check the path that runs.
pub fn launch_target(report: &PackageReport) -> PathBuf {
    report.exe.clone()
}

fn write_zip(dir: &Path, dest: &Path) -> Result<(), CoreError> {
    let mut files = Vec::new();
    collect_zip_files(dir, dir, &mut files)?;
    files.sort();
    let mut body = Vec::new();
    let mut central = Vec::new();
    let mut count = 0u16;
    for rel in files {
        let data = std::fs::read(dir.join(&rel))?;
        let compressed = miniz_oxide::deflate::compress_to_vec(&data, 6);
        let (method, payload) =
            if compressed.len() < data.len() { (8u16, compressed) } else { (0u16, data.clone()) };
        let name = rel.replace('\\', "/");
        let crc = crc32(&data);
        let offset = body.len() as u32;
        write_u32(&mut body, 0x0403_4b50);
        write_u16(&mut body, 20);
        write_u16(&mut body, 0);
        write_u16(&mut body, method);
        write_u16(&mut body, 0);
        write_u16(&mut body, 0);
        write_u32(&mut body, crc);
        write_u32(&mut body, payload.len() as u32);
        write_u32(&mut body, data.len() as u32);
        write_u16(&mut body, name.len() as u16);
        write_u16(&mut body, 0);
        body.extend(name.as_bytes());
        body.extend(&payload);
        write_u32(&mut central, 0x0201_4b50);
        write_u16(&mut central, 20);
        write_u16(&mut central, 20);
        write_u16(&mut central, 0);
        write_u16(&mut central, method);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u32(&mut central, crc);
        write_u32(&mut central, payload.len() as u32);
        write_u32(&mut central, data.len() as u32);
        write_u16(&mut central, name.len() as u16);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u32(&mut central, 0);
        write_u32(&mut central, offset);
        central.extend(name.as_bytes());
        count += 1;
    }
    let central_at = body.len() as u32;
    body.extend(&central);
    write_u32(&mut body, 0x0605_4b50);
    write_u16(&mut body, 0);
    write_u16(&mut body, 0);
    write_u16(&mut body, count);
    write_u16(&mut body, count);
    write_u32(&mut body, central.len() as u32);
    write_u32(&mut body, central_at);
    write_u16(&mut body, 0);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest, body)?;
    Ok(())
}

fn collect_zip_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), CoreError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_zip_files(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            if rel != "manifest.txt" {
                out.push(rel);
            }
        }
    }
    Ok(())
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend(value.to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}

pub(crate) fn open_export_panel(
    session: &mut crate::EditorSession,
    _args: &crate::CommandArgs,
) -> Result<(), CoreError> {
    session.focus_panel("export");
    Ok(())
}

pub(crate) fn write_book_cmd(
    session: &mut crate::EditorSession,
    args: &crate::CommandArgs,
) -> Result<(), CoreError> {
    let Some(project) = session.project.clone() else {
        return Err(CoreError::new(session.messages.get("package-no-project")));
    };
    let text =
        args.get("text").ok_or_else(|| CoreError::new("missing argument 'text'"))?.to_string();
    let book = parse_book(&text).map_err(CoreError::new)?;
    store_book(&project.root, &book)?;
    session.models.panel_states.insert("export.book".into(), to_text(&book));
    Ok(())
}

pub(crate) fn export_go(
    session: &mut crate::EditorSession,
    args: &crate::CommandArgs,
) -> Result<(), CoreError> {
    crate::import_pipeline::begin_export(session, export_request(args, false))
}

pub(crate) fn export_run(
    session: &mut crate::EditorSession,
    args: &crate::CommandArgs,
) -> Result<(), CoreError> {
    crate::import_pipeline::begin_export(session, export_request(args, true))
}

pub(crate) fn preflight_export(
    session: &mut crate::EditorSession,
    request: &crate::import_pipeline::ExportRequest,
) -> Result<(), CoreError> {
    export_and_maybe_run(session, &request_args(request), request.run, false)
}

pub(crate) fn execute_export(
    session: &mut crate::EditorSession,
    request: &crate::import_pipeline::ExportRequest,
) -> Result<(), CoreError> {
    export_and_maybe_run(session, &request_args(request), request.run, true)
}

fn export_request(args: &crate::CommandArgs, run: bool) -> crate::import_pipeline::ExportRequest {
    crate::import_pipeline::ExportRequest {
        preset: args.get("preset").unwrap_or("").to_string(),
        exe: args.get("exe").unwrap_or("").to_string(),
        out: args.get("out").unwrap_or("").to_string(),
        run,
    }
}

fn request_args(request: &crate::import_pipeline::ExportRequest) -> crate::CommandArgs {
    let mut pairs = Vec::new();
    if !request.preset.is_empty() {
        pairs.push(("preset", request.preset.as_str()));
    }
    if !request.exe.is_empty() {
        pairs.push(("exe", request.exe.as_str()));
    }
    if !request.out.is_empty() {
        pairs.push(("out", request.out.as_str()));
    }
    crate::CommandArgs::new(&pairs)
}

fn export_and_maybe_run(
    session: &mut crate::EditorSession,
    args: &crate::CommandArgs,
    run: bool,
    commit: bool,
) -> Result<(), CoreError> {
    crate::editor_tools::note_worker_budget(&session.foundation.settings);
    let Some(project) = session.project.clone() else {
        return Err(CoreError::new(session.messages.get("package-no-project")));
    };
    let book = load_book(&project.root).map_err(CoreError::new)?;
    let preset = match args.get("preset").map(str::trim).filter(|name| !name.is_empty()) {
        // A project with no saved presets shows (and exports) the implied default one.
        Some(name) if book.presets.is_empty() && name == project.name => ExportPreset::named(
            &project.name,
            &package::default_package_folder(&project.root, &project.name).display().to_string(),
        ),
        Some(name) => {
            book.presets.iter().find(|preset| preset.name == name).cloned().ok_or_else(|| {
                CoreError::new(session.messages.format("export-no-preset", &[("name", name)]))
            })?
        }
        None => book.selected_preset().cloned().unwrap_or_else(|| {
            ExportPreset::named(
                &project.name,
                &package::default_package_folder(&project.root, &project.name)
                    .display()
                    .to_string(),
            )
        }),
    };
    let mut preset = preset;
    if let Some(out) = args.get("out").map(str::trim).filter(|out| !out.is_empty()) {
        preset.folder = out.to_string();
    }
    let exe = match args.get("exe") {
        Some(exe) => PathBuf::from(exe),
        None => std::env::current_exe()?,
    };
    let platform = args
        .get("platform")
        .map(str::trim)
        .filter(|p| !p.is_empty() && crate::platform_overrides::platform_id_valid(p))
        .map(str::to_string)
        .or_else(|| {
            // The preset's Target platform drives settings overrides and import overrides alike.
            Some(preset.platform.clone())
                .filter(|p| crate::platform_overrides::platform_id_valid(p))
        })
        .unwrap_or_else(|| crate::platform_overrides::host_platform().to_string());
    preset.platform = platform.clone();
    let platform = platform.as_str();
    let window = crate::window_game::GameWindow::from_pairs(|id| {
        crate::platform_overrides::effective_from_session(session, id, platform)
    });
    let window_text = window.runner_text(&project.name);
    if !commit {
        return Ok(());
    }
    let render = crate::render_quality::for_export(session, platform);
    let render_text = crate::render_quality::runner_text(&render);
    let report = export_preset(&project.root, &project.name, &exe, &preset, &window_text, &render_text).map_err(
        |err| {
            CoreError::new(
                session.messages.format("package-failed", &[("error", &err.to_string())]),
            )
        },
    )?;
    if run {
        launch_game(&report.exe).map_err(|err| {
            CoreError::new(
                session.messages.format("package-failed", &[("error", &err.to_string())]),
            )
        })?;
        let name =
            report.exe.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        session
            .notify(session.messages.format("export-launched", &[("name", name.as_str())]), false);
    } else {
        let folder = report.folder.display().to_string();
        let files = report.files.to_string();
        session.notify(
            session.messages.format("package-done", &[("folder", &folder), ("files", &files)]),
            false,
        );
        session.host_action = Some(crate::session::HostAction::OpenPath(report.folder));
    }
    Ok(())
}

/// The book the Export panel shows: the project file, then the panel draft.
pub fn book_for_session(session: &crate::EditorSession) -> Result<ExportBook, String> {
    if let Some(project) = &session.project {
        let book = load_book(&project.root)?;
        if !book.presets.is_empty() {
            return Ok(book);
        }
    }
    if let Some(text) = session.models.panel_states.get("export.book") {
        return parse_book(text);
    }
    Ok(ExportBook::default())
}
