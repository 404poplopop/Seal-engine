//! Single-file project packs for exported games.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::CoreError;

const MAGIC: &[u8] = b"seal_pack 1\n";

/// Write every file under `root` (relative paths) into one pack.
pub fn write_pack(root: &Path, files: &[String]) -> Result<Vec<u8>, CoreError> {
    let mut index = BTreeMap::new();
    let mut blobs = Vec::new();
    for rel in files {
        let path = root.join(rel);
        let data = std::fs::read(&path)?;
        index.insert(rel.clone(), (blobs.len(), data.len()));
        blobs.extend_from_slice(&data);
    }
    let mut out = MAGIC.to_vec();
    for (rel, (offset, len)) in &index {
        out.extend_from_slice(rel.as_bytes());
        out.push(0);
        out.extend_from_slice(&offset.to_string().as_bytes());
        out.push(b' ');
        out.extend_from_slice(&len.to_string().as_bytes());
        out.push(b'\n');
    }
    out.push(b'-');
    out.push(b'-');
    out.push(b'-');
    out.push(b'\n');
    out.extend_from_slice(&blobs);
    Ok(out)
}

/// Read a pack into relative path and file bytes.
pub fn read_pack(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if !bytes.starts_with(MAGIC) {
        return Err("not a seal pack".into());
    }
    let body = &bytes[MAGIC.len()..];
    let sep = body.windows(4).position(|w| w == b"---\n").ok_or("pack is missing data")?;
    let header = std::str::from_utf8(&body[..sep]).map_err(|_| "pack header is not text")?;
    let blob = &body[sep + 4..];
    let mut out = BTreeMap::new();
    for line in header.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let (rel, rest) = line.split_once('\0').ok_or_else(|| format!("bad pack entry: {line}"))?;
        if !pack_rel_ok(rel) {
            return Err(format!("pack entry {rel} is not a project path"));
        }
        let (off, len) = rest.split_once(' ').ok_or_else(|| format!("bad pack entry: {line}"))?;
        let offset: usize = off.parse().map_err(|_| format!("bad pack offset: {off}"))?;
        let length: usize = len.parse().map_err(|_| format!("bad pack length: {len}"))?;
        let end = offset
            .checked_add(length)
            .ok_or_else(|| format!("pack entry {rel} runs past the data"))?;
        if end > blob.len() {
            return Err(format!("pack entry {rel} runs past the data"));
        }
        out.insert(rel.to_string(), blob[offset..end].to_vec());
    }
    Ok(out)
}

/// Extract pack to `cache` and return the folder that holds `project.seal`.
pub fn mount_pack(game_dir: &Path, pack_path: &Path) -> Result<PathBuf, CoreError> {
    let bytes = std::fs::read(pack_path)?;
    let files = read_pack(&bytes).map_err(CoreError::new)?;
    if !files.contains_key("project.seal") {
        return Err(CoreError::new("the pack has no project.seal"));
    }
    let stamp = format!("{:016x}", fnv_bytes(&bytes));
    let out = game_dir.join(".seal_pack_cache").join(stamp);
    if out.join("project.seal").is_file() {
        return Ok(out);
    }
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    std::fs::create_dir_all(&out)?;
    for (rel, data) in &files {
        let dest = out.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(dest, data)?;
    }
    Ok(out)
}

fn pack_rel_ok(rel: &str) -> bool {
    let path = Path::new(rel);
    !rel.is_empty()
        && !path.is_absolute()
        && path.components().all(|part| matches!(part, std::path::Component::Normal(_)))
}

fn fnv_bytes(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("seal-pack-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn pack_round_trip_matches_folder_files() {
        let root = temp("rt");
        fs::write(root.join("project.seal"), "id test\n").unwrap();
        fs::create_dir_all(root.join("sprites")).unwrap();
        fs::write(root.join("sprites/a.png"), b"png").unwrap();
        let list = vec!["project.seal".into(), "sprites/a.png".into()];
        let packed = write_pack(&root, &list).unwrap();
        let loaded = read_pack(&packed).unwrap();
        for rel in &list {
            let disk = fs::read(root.join(rel)).unwrap();
            assert_eq!(loaded.get(rel).map(Vec::as_slice), Some(disk.as_slice()));
        }
    }

    #[test]
    fn damaged_pack_header_is_refused() {
        let err = read_pack(b"not a pack").unwrap_err();
        assert!(err.contains("not a seal pack"));
    }

    #[test]
    fn damaged_pack_path_is_refused() {
        let mut bytes = b"seal_pack 1\n".to_vec();
        bytes.extend_from_slice(b"../secret\x001 1\n---\nZ");
        let err = read_pack(&bytes).unwrap_err();
        assert!(err.contains("not a project path"), "{err}");
    }
}
