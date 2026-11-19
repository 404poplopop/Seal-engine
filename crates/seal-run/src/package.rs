//! `seal-run package`: build a game in release mode and lay it out as a
//! folder you can zip and hand to players, with a manifest of file hashes.
//! `seal-run verify-package` checks a folder against its manifest.

use std::path::{Path, PathBuf};
use std::process::Command;

pub struct PackageOptions {
    pub package: String,
    pub bin: String,
    pub data: Option<PathBuf>,
    pub out: PathBuf,
    pub name: String,
    pub toolchain: Option<String>,
    pub target: Option<String>,
    pub build: bool,
    /// Where cargo puts builds; defaults to `CARGO_TARGET_DIR` or `target`.
    pub target_dir: PathBuf,
}

pub fn parse(args: &[String]) -> Result<PackageOptions, String> {
    let mut o = PackageOptions {
        package: String::new(),
        bin: String::new(),
        data: None,
        out: PathBuf::new(),
        name: String::new(),
        toolchain: None,
        target: None,
        build: true,
        target_dir: std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| "target".into()),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |flag: &str| it.next().cloned().ok_or(format!("{flag} needs a value"));
        match a.as_str() {
            "--package" | "-p" => o.package = value(a)?,
            "--bin" => o.bin = value(a)?,
            "--data" => o.data = Some(value(a)?.into()),
            "--out" => o.out = value(a)?.into(),
            "--name" => o.name = value(a)?,
            "--toolchain" => o.toolchain = Some(value(a)?),
            "--target" => o.target = Some(value(a)?),
            "--target-dir" => o.target_dir = value(a)?.into(),
            "--no-build" => o.build = false,
            other => return Err(format!("unknown option {other}")),
        }
    }
    if o.bin.is_empty() {
        return Err("--bin is required".into());
    }
    if o.package.is_empty() {
        o.package = o.bin.clone();
    }
    if o.name.is_empty() {
        o.name = o.bin.clone();
    }
    if o.out.as_os_str().is_empty() {
        o.out = PathBuf::from("dist").join(&o.name);
    }
    Ok(o)
}

/// FNV-1a 64 of a file's bytes.
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3))
}

fn exe_name(bin: &str, target: Option<&str>) -> String {
    let windows = match target {
        Some(t) => t.contains("windows"),
        None => cfg!(windows),
    };
    if windows {
        format!("{bin}.exe")
    } else {
        bin.to_string()
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    let mut entries: Vec<_> = std::fs::read_dir(from)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else {
            std::fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}

/// Every file under `root`, as sorted forward-slash relative paths.
fn files(root: &Path) -> std::io::Result<Vec<String>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
        for e in std::fs::read_dir(dir)? {
            let e = e?;
            if e.file_type()?.is_dir() {
                walk(root, &e.path(), out)?;
            } else {
                let rel = e.path().strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.push(rel);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

pub const MANIFEST: &str = "manifest.txt";

/// Write `manifest.txt`: a header, then `hash size path` for every other file.
pub fn write_manifest(dir: &Path, name: &str) -> Result<usize, String> {
    let err = |e: std::io::Error| format!("{}: {e}", dir.display());
    let mut text = format!("seal_package 1 name {name}\n");
    let list: Vec<String> =
        files(dir).map_err(err)?.into_iter().filter(|f| f != MANIFEST).collect();
    for f in &list {
        let bytes = std::fs::read(dir.join(f)).map_err(err)?;
        text += &format!("{:016x} {} {f}\n", hash(&bytes), bytes.len());
    }
    std::fs::write(dir.join(MANIFEST), text).map_err(err)?;
    Ok(list.len())
}

/// Problems found when checking a package folder against its manifest; empty when it is intact.
pub fn verify(dir: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(dir.join(MANIFEST))
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut lines = text.lines();
    if !lines.next().unwrap_or("").starts_with("seal_package 1 ") {
        return Err("not a Seal package manifest".into());
    }
    let mut problems = Vec::new();
    let mut listed = Vec::new();
    for line in lines {
        let mut parts = line.splitn(3, ' ');
        let (Some(h), Some(_size), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            problems.push(format!("bad manifest line: {line}"));
            continue;
        };
        listed.push(path.to_string());
        match std::fs::read(dir.join(path)) {
            Ok(bytes) if format!("{:016x}", hash(&bytes)) == h => {}
            Ok(_) => problems.push(format!("changed: {path}")),
            Err(_) => problems.push(format!("missing: {path}")),
        }
    }
    for f in files(dir).map_err(|e| e.to_string())? {
        if f != MANIFEST && !listed.contains(&f) {
            problems.push(format!("not in manifest: {f}"));
        }
    }
    Ok(problems)
}

pub fn package(o: &PackageOptions) -> Result<(), String> {
    if o.build {
        // `$CARGO` is the toolchain's own cargo, which does not understand
        // `+toolchain`; a pinned toolchain goes through rustup instead.
        let mut cmd = match &o.toolchain {
            Some(t) => {
                let mut c = Command::new("rustup");
                c.args(["run", t, "cargo"]);
                c.env_remove("RUSTUP_TOOLCHAIN").env_remove("CARGO");
                c
            }
            None => Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())),
        };
        cmd.args(["build", "--release", "-p", &o.package, "--bin", &o.bin]);
        if let Some(t) = &o.target {
            cmd.args(["--target", t]);
        }
        cmd.arg("--target-dir").arg(&o.target_dir);
        eprintln!("building {} ...", o.bin);
        let status = cmd.status().map_err(|e| format!("cargo: {e}"))?;
        if !status.success() {
            return Err(format!("cargo build failed ({status})"));
        }
    }
    let mut built = o.target_dir.clone();
    if let Some(t) = &o.target {
        built.push(t);
    }
    let exe = exe_name(&o.bin, o.target.as_deref());
    built.push("release");
    built.push(&exe);
    if !built.exists() {
        return Err(format!("{} not found; build it or drop --no-build", built.display()));
    }
    if o.out.exists() {
        std::fs::remove_dir_all(&o.out).map_err(|e| format!("{}: {e}", o.out.display()))?;
    }
    std::fs::create_dir_all(&o.out).map_err(|e| format!("{}: {e}", o.out.display()))?;
    std::fs::copy(&built, o.out.join(&exe)).map_err(|e| format!("{}: {e}", built.display()))?;
    if let Some(data) = &o.data {
        copy_dir(data, &o.out.join("data")).map_err(|e| format!("{}: {e}", data.display()))?;
    }
    let mut readme = format!(
        "{name}\n\nRun {exe}. Files in data/ can be edited: input.map changes the controls.\nBuilt with the Seal engine.\n",
        name = o.name
    );
    if let Some(map) =
        o.data.as_ref().and_then(|d| std::fs::read_to_string(d.join("input.map")).ok())
    {
        readme += "\nControls:\n";
        for line in map.lines().filter(|l| !l.trim().is_empty()) {
            readme += &format!("  {line}\n");
        }
    }
    std::fs::write(o.out.join("README.txt"), readme).map_err(|e| e.to_string())?;
    let n = write_manifest(&o.out, &o.name)?;
    let size: u64 = files(&o.out)
        .map_err(|e| e.to_string())?
        .iter()
        .filter_map(|f| std::fs::metadata(o.out.join(f)).ok())
        .map(|m| m.len())
        .sum();
    println!(
        "packaged {} into {} ({n} files, {:.1} MB)",
        o.name,
        o.out.display(),
        size as f64 / 1e6
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("seal-package-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn packages_a_built_program_with_data_and_verifies_it() {
        let root = temp("ok");
        let target = root.join("target");
        std::fs::create_dir_all(target.join("release")).unwrap();
        std::fs::write(target.join("release").join(exe_name("game", None)), b"program bytes")
            .unwrap();
        let data = root.join("data");
        std::fs::create_dir_all(data.join("levels")).unwrap();
        std::fs::write(data.join("input.map"), "action jump = key.space\n").unwrap();
        std::fs::write(data.join("levels").join("one.txt"), "##\n").unwrap();
        let args: Vec<String> = ["--bin", "game", "--name", "Game", "--no-build", "--data"]
            .iter()
            .map(|s| s.to_string())
            .chain([data.to_string_lossy().into_owned()])
            .chain(["--out".into(), root.join("out").to_string_lossy().into_owned()])
            .chain(["--target-dir".into(), target.to_string_lossy().into_owned()])
            .collect();
        let o = parse(&args).unwrap();
        package(&o).unwrap();
        let out = root.join("out");
        let list = files(&out).unwrap();
        let exe = exe_name("game", None);
        let mut expected = vec![
            "README.txt".to_string(),
            "data/input.map".into(),
            "data/levels/one.txt".into(),
            exe,
            MANIFEST.into(),
        ];
        expected.sort();
        assert_eq!(list, expected);
        assert!(std::fs::read_to_string(out.join("README.txt"))
            .unwrap()
            .contains("action jump = key.space"));
        assert_eq!(verify(&out).unwrap(), Vec::<String>::new());

        std::fs::write(out.join("data/input.map"), "action jump = key.k\n").unwrap();
        std::fs::write(out.join("extra.txt"), "x").unwrap();
        std::fs::remove_file(out.join("data/levels/one.txt")).unwrap();
        assert_eq!(
            verify(&out).unwrap(),
            [
                "changed: data/input.map",
                "missing: data/levels/one.txt",
                "not in manifest: extra.txt"
            ]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_build_is_reported() {
        let root = temp("missing");
        let o = parse(&[
            "--bin".into(),
            "nothing".into(),
            "--no-build".into(),
            "--target-dir".into(),
            root.to_string_lossy().into_owned(),
            "--out".into(),
            root.join("out").to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(package(&o).unwrap_err().contains("not found"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
