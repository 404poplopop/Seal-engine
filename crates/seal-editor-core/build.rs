use std::process::Command;

fn main() {
    let hash = std::env::var("SEAL_GIT_HASH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(git_hash)
        .or_else(file_hash)
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=SEAL_GIT_HASH={hash}");
    println!("cargo:rerun-if-env-changed=SEAL_GIT_HASH");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.build-hash");
}

fn git_hash() -> Option<String> {
    let output = Command::new("git").args(["rev-parse", "--short=12", "HEAD"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn file_hash() -> Option<String> {
    let text = std::fs::read_to_string(".build-hash").ok()?;
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}
