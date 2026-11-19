//! Windows Store package (MSIX) layout and manifest for export presets.

use std::path::Path;

use crate::error::CoreError;

/// Relative path of the manifest inside the package layout.
pub const APPX_MANIFEST: &str = "AppxManifest.xml";

/// Write `AppxManifest.xml` and the folder layout under `layout_dir`.
pub fn write_layout(
    layout_dir: &Path,
    display_name: &str,
    publisher: &str,
    version: &str,
    exe_name: &str,
) -> Result<(), CoreError> {
    std::fs::create_dir_all(layout_dir.join("Assets"))?;
    let manifest = manifest_xml(display_name, publisher, version, exe_name);
    std::fs::write(layout_dir.join(APPX_MANIFEST), manifest)?;
    std::fs::write(layout_dir.join("Assets").join("Logo.png"), LOGO_PNG)?;
    Ok(())
}

/// 1×1 picture the manifest names. MakeAppx refuses a layout that omits it.
const LOGO_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

pub fn manifest_xml(display_name: &str, publisher: &str, version: &str, exe_name: &str) -> String {
    let name = xml_escape(display_name);
    let publisher = xml_escape(publisher);
    let exe = xml_escape(exe_name);
    let version = xml_escape(version);
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
         xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
         xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
         IgnorableNamespaces="uap rescap">
  <Identity Name="SealGame" Version="{version}" Publisher="{publisher}" />
  <Properties>
    <DisplayName>{name}</DisplayName>
    <PublisherDisplayName>Seal</PublisherDisplayName>
    <Logo>Assets\Logo.png</Logo>
  </Properties>
  <Resources>
    <Resource Language="en-us" />
  </Resources>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.17763.0" MaxVersionTested="10.0.22621.0" />
  </Dependencies>
  <Applications>
    <Application Id="Game" Executable="{exe}" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements DisplayName="{name}" Description="{name}" Square150x150Logo="Assets\Logo.png" Square44x44Logo="Assets\Logo.png" BackgroundColor="transparent" />
    </Application>
  </Applications>
  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
  </Capabilities>
</Package>
"#
    )
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Build `.msix` when MakeAppx is on PATH; otherwise return a clear message.
pub fn try_build_msix(layout_dir: &Path, msix_path: &Path) -> Result<(), String> {
    let makeappx = which_makeappx()?;
    let layout = layout_dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", layout_dir.display()))?
        .display()
        .to_string();
    let dest =
        msix_path.canonicalize().unwrap_or_else(|_| msix_path.to_path_buf()).display().to_string();
    let mut command = std::process::Command::new(&makeappx);
    command.args(["pack", "/d", &layout, "/p", &dest, "/o"]);
    no_window(&mut command);
    let output = command.output().map_err(|e| format!("could not run {makeappx}: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        // MakeAppx writes its errors to standard output.
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let reason: Vec<&str> =
            text.lines().map(str::trim).filter(|l| l.contains("error") || l.contains("Error")).collect();
        let reason = if reason.is_empty() { text.trim().to_string() } else { reason.join(" ") };
        Err(format!("MakeAppx failed: {reason}"))
    }
}

pub fn which_makeappx() -> Result<String, String> {
    if let Ok(path) = std::env::var("SEAL_MAKEAPPX") {
        if std::path::Path::new(&path).is_file() {
            return Ok(path);
        }
    }
    if let Some(found) = find_in_build_tools(&tools_dir()) {
        return Ok(found.display().to_string());
    }
    if cfg!(windows) {
        for base in windows_sdk_bins() {
            let candidate = base.join("MakeAppx.exe");
            if candidate.is_file() {
                return Ok(candidate.display().to_string());
            }
        }
    }
    Err("MakeAppx was not found. Use Download MakeAppx in Export Game, or install the Windows SDK.".into())
}

/// Where Seal keeps tools it downloads for packaging (per user, outside any project).
pub fn tools_dir() -> std::path::PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("Seal").join("tools").join("windows-sdk-build-tools")
}

/// `bin/<version>/x64/makeappx.exe` inside an unpacked Windows SDK Build Tools package
/// (the newest version wins).
pub fn find_in_build_tools(root: &Path) -> Option<std::path::PathBuf> {
    let mut versions: Vec<_> = std::fs::read_dir(root.join("bin")).ok()?.flatten().map(|e| e.path()).collect();
    versions.sort();
    versions.into_iter().rev().map(|v| v.join("x64").join("makeappx.exe")).find(|p| p.is_file())
}

/// The Windows SDK Build Tools package (it holds MakeAppx; no installer or admin rights).
pub const BUILD_TOOLS_URL: &str = "https://www.nuget.org/api/v2/package/Microsoft.Windows.SDK.BuildTools";

/// Download and unpack the build tools into [`tools_dir`]; returns the MakeAppx path.
pub fn download_makeappx() -> Result<std::path::PathBuf, String> {
    let bytes = seal_world::cloud::https_get(BUILD_TOOLS_URL, 200 * 1024 * 1024)?;
    let dir = tools_dir();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let archive = dir.join("build-tools.zip");
    std::fs::write(&archive, &bytes).map_err(|e| format!("{}: {e}", archive.display()))?;
    // tar.exe ships with Windows 10 and later and unpacks zip files.
    let mut command = std::process::Command::new("tar");
    command.arg("-xf").arg(&archive).arg("-C").arg(&dir);
    no_window(&mut command);
    let status = command.status().map_err(|e| format!("could not unpack the build tools: {e}"))?;
    let _ = std::fs::remove_file(&archive);
    if !status.success() {
        return Err("could not unpack the build tools".into());
    }
    find_in_build_tools(&dir).ok_or_else(|| "the build tools did not contain MakeAppx".into())
}

/// MakeAppx download state for the Export Game panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolState {
    Idle,
    Downloading,
    Failed(String),
}

static TOOL_STATE: std::sync::Mutex<ToolState> = std::sync::Mutex::new(ToolState::Idle);

pub fn tool_state() -> ToolState {
    TOOL_STATE.lock().map(|s| s.clone()).unwrap_or(ToolState::Idle)
}

/// Start the download on a background thread; the panel shows its state.
pub fn start_makeappx_download() {
    if let Ok(mut state) = TOOL_STATE.lock() {
        if *state == ToolState::Downloading {
            return;
        }
        *state = ToolState::Downloading;
    }
    std::thread::spawn(|| {
        let result = download_makeappx();
        if let Ok(mut state) = TOOL_STATE.lock() {
            *state = match result {
                Ok(_) => ToolState::Idle,
                Err(e) => ToolState::Failed(e),
            };
        }
    });
}

#[cfg(windows)]
fn no_window(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn no_window(_command: &mut std::process::Command) {}

#[cfg(windows)]
fn windows_sdk_bins() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(root) = std::env::var("ProgramFiles(x86)") {
        let kits = std::path::Path::new(&root).join("Windows Kits").join("10").join("bin");
        if let Ok(entries) = std::fs::read_dir(&kits) {
            for entry in entries.flatten() {
                out.push(entry.path().join("x64"));
            }
        }
    }
    out
}

#[cfg(not(windows))]
fn windows_sdk_bins() -> Vec<std::path::PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tools_lookup_picks_the_newest_x64_makeappx() {
        let root = std::env::temp_dir().join(format!("seal-bt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for v in ["10.0.26100.0", "10.0.28000.0"] {
            let x64 = root.join("bin").join(v).join("x64");
            std::fs::create_dir_all(&x64).unwrap();
            std::fs::write(x64.join("makeappx.exe"), b"exe").unwrap();
        }
        let found = find_in_build_tools(&root).unwrap();
        assert!(found.ends_with("bin/10.0.28000.0/x64/makeappx.exe") || found.to_string_lossy().contains("10.0.28000.0"));
        assert!(find_in_build_tools(&root.join("missing")).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn manifest_contains_identity_and_executable() {
        let xml = manifest_xml("Hop", "CN=Seal", "1.0.0.0", "Hop.exe");
        assert!(xml.contains("<DisplayName>Hop</DisplayName>"));
        assert!(xml.contains(r#"Executable="Hop.exe""#));
        assert!(xml.contains("Windows.Desktop"));
    }

    #[test]
    fn manifest_escapes_special_characters() {
        let xml = manifest_xml("A & B", "CN=Test", "1.0.0.0", "game.exe");
        assert!(xml.contains("A &amp; B"));
    }

    #[test]
    fn layout_writes_manifest_and_logo() {
        let dir = std::env::temp_dir().join(format!("seal-msix-layout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_layout(&dir, "Hop", "CN=Seal", "1.0.0.0", "Hop.exe").unwrap();
        let xml = std::fs::read_to_string(dir.join(APPX_MANIFEST)).unwrap();
        assert!(xml.contains("<DisplayName>Hop</DisplayName>"));
        assert!(xml.contains(r#"Executable="Hop.exe""#));
        assert!(dir.join("Assets").join("Logo.png").is_file());
        let msix = dir.join("Hop.msix");
        match try_build_msix(&dir, &msix) {
            Ok(()) => assert!(msix.is_file()),
            Err(message) => assert!(message.contains("MakeAppx"), "{message}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
