//! Helpers for editor tests, plus the crate-graph check the shell foundation requires.
#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A temporary directory removed when this value is dropped.
pub struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    pub fn new(label: &str) -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("seal-{label}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).expect("temp dir");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// What a persistence test pretends the disk did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoFault {
    None,
    Rename,
    DiskFull,
    Permission,
    Slow,
}

pub fn io_fault(kind: std::io::ErrorKind) -> IoFault {
    match kind {
        std::io::ErrorKind::AlreadyExists => IoFault::Rename,
        std::io::ErrorKind::StorageFull => IoFault::DiskFull,
        std::io::ErrorKind::PermissionDenied => IoFault::Permission,
        std::io::ErrorKind::TimedOut => IoFault::Slow,
        _ => IoFault::None,
    }
}

/// Editor crates may depend only inward: binary, shell, panels, ui, core, then the leaves.
pub fn check_dependency_direction(workspace: &Path) -> Result<(), String> {
    let crates = workspace.join("crates");
    let mut names = Vec::new();
    for entry in fs::read_dir(&crates).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let manifest = entry.path().join("Cargo.toml");
        if manifest.exists() {
            let text = fs::read_to_string(&manifest).map_err(|err| err.to_string())?;
            let name = package_name(&text)
                .ok_or_else(|| format!("no package name in {}", manifest.display()))?;
            let deps = dependency_names(&text);
            for dep in &deps {
                if dep.starts_with("seal-") && !allows(&name, dep) {
                    return Err(format!("{name} must not depend on {dep}"));
                }
                if is_toolkit(dep)
                    && !matches!(
                        name.as_str(),
                        "seal-ui" | "seal-editor-shell" | "seal-editor-panels"
                    )
                {
                    return Err(format!("{name} must not name the UI toolkit ({dep})"));
                }
            }
            names.push(name);
        }
    }
    for required in ["seal-i18n", "seal-testkit", "seal-editor-core", "seal-editor"] {
        if !names.iter().any(|name| name == required) {
            return Err(format!("workspace is missing {required}"));
        }
    }
    let editor =
        fs::read_to_string(crates.join("seal-editor/Cargo.toml")).map_err(|err| err.to_string())?;
    if !editor.contains("dev-tools") || !editor.contains("editor") {
        return Err("seal-editor is missing the editor and dev-tools features".into());
    }
    let core = fs::read_to_string(crates.join("seal-editor-core/Cargo.toml"))
        .map_err(|err| err.to_string())?;
    if !core.contains("mock-world") {
        return Err("seal-editor-core is missing the mock-world feature".into());
    }
    Ok(())
}

fn allows(from: &str, to: &str) -> bool {
    // The test kit depends on nothing, so any crate's tests may use its helpers.
    if to == "seal-testkit" && from != "seal-testkit" {
        return true;
    }
    match from {
        "seal-log" | "seal-sealtext" | "seal-i18n" | "seal-testkit" => false,
        // The world runs object scripts during play, so it sits above the graph runtime,
        // and it maps player controls, so it reads the input map.
        "seal-world" => {
            matches!(
                to,
                "seal-input"
                    | "seal-sealtext"
                    | "seal-log"
                    | "seal-ecs"
                    | "seal-scene"
                    | "seal-physics2d"
                    | "seal-graph"
            )
        }
        "seal-scene" => matches!(to, "seal-ecs"),
        "seal-render" => matches!(to, "seal-trace" | "seal-math"),
        "seal-input" => false,
        "seal-audio" => matches!(to, "seal-math"),
        "seal-physics2d" => matches!(to, "seal-ecs" | "seal-trace"),
        "seal-render-wgpu" => matches!(to, "seal-render" | "seal-kernel" | "seal-jobs"),
        "seal-sample-2d" => matches!(to, "seal-app" | "seal-editor-core"),
        "seal-app" => matches!(
            to,
            "seal-ecs"
                | "seal-render"
                | "seal-render2d"
                | "seal-input"
                | "seal-audio"
                | "seal-physics2d"
                | "seal-trace"
                | "seal-render-wgpu"
        ),
        "seal-render2d" => matches!(to, "seal-render" | "seal-math" | "seal-ecs" | "seal-trace"),
        "seal-editor-core" => {
            matches!(
                to,
                "seal-log"
                    | "seal-sealtext"
                    | "seal-i18n"
                    | "seal-world"
                    | "seal-graph"
                    | "seal-render"
                    // The Asset Editor reads sound files for their length and waveform.
                    | "seal-audio"
            )
        }
        // Runtime crates sit below the editor and never reach up into it.
        "seal-math" | "seal-mem" | "seal-trace" => false,
        "seal-jobs" => matches!(to, "seal-trace"),
        "seal-ecs" => matches!(
            to,
            "seal-jobs" | "seal-trace" | "seal-sealtext" | "seal-math" | "seal-mem" | "seal-log"
        ),
        "seal-run" => matches!(
            to,
            "seal-scene"
                | "seal-jobs"
                | "seal-mem"
                | "seal-ecs"
                | "seal-math"
                | "seal-trace"
                | "seal-sealtext"
                | "seal-log"
                | "seal-graph"
        ),
        "seal-graph" => {
            matches!(
                to,
                "seal-sealtext"
                    | "seal-ecs"
                    | "seal-kernel"
                    | "seal-jobs"
                    | "seal-predict"
                    | "seal-math"
            )
        }
        "seal-graph-tests" => matches!(to, "seal-graph" | "seal-predict"),
        "seal-kernel" => matches!(to, "seal-jobs" | "seal-trace"),
        "seal-predict" => matches!(to, "seal-kernel" | "seal-jobs"),
        "seal-migrate" => matches!(to, "seal-graph" | "seal-sealtext"),
        "seal-agent" | "seal-capi" => {
            matches!(to, "seal-ecs" | "seal-scene" | "seal-sealtext" | "seal-trace" | "seal-log")
        }
        "seal-ui" => matches!(to, "seal-editor-core"),
        "seal-editor-panels" => matches!(to, "seal-editor-core" | "seal-ui" | "seal-log"),
        "seal-editor-shell" => {
            matches!(to, "seal-editor-core" | "seal-editor-panels" | "seal-ui" | "seal-log")
        }
        "seal-editor" => matches!(
            to,
            "seal-editor-core"
                | "seal-editor-shell"
                | "seal-editor-panels"
                | "seal-ui"
                | "seal-log"
                | "seal-i18n"
                // The app plays play-mode sounds through the runtime mixer.
                | "seal-audio"
                | "seal-app"
        ),
        _ => false,
    }
}

fn is_toolkit(name: &str) -> bool {
    matches!(name, "egui" | "eframe" | "egui_dock")
}

fn package_name(text: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package && line.starts_with("name") {
            return line.split('"').nth(1).map(str::to_string);
        }
    }
    None
}

fn dependency_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_deps = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_deps = line.starts_with("[dependencies]")
                || line.starts_with("[dev-dependencies]")
                || line.starts_with("[build-dependencies]");
            continue;
        }
        if !in_deps || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((name, _)) = line.split_once('=') {
            let name = name.trim().trim_matches('"');
            if !name.is_empty() && !name.contains(' ') {
                names.push(name.to_string());
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_workspace_graph_matches_the_shell_rules() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        check_dependency_direction(&root).unwrap();
    }

    #[test]
    fn io_errors_map_onto_faults() {
        assert_eq!(io_fault(std::io::ErrorKind::StorageFull), IoFault::DiskFull);
        assert_eq!(io_fault(std::io::ErrorKind::PermissionDenied), IoFault::Permission);
    }
}

pub mod garbage;
