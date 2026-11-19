//! Resolve flipbook renderer paths to a picture file and texture rectangle.

use std::path::{Path, PathBuf};

use crate::frames;

/// How many flipbook frames the path describes.
pub fn frame_count(path: &str, project: Option<&Path>) -> u32 {
    if frames::is_frames_path(path) {
        let full = resolve_path(path, project);
        if let Some(lib) = frames::load_cached(&full) {
            if let Some(clip) = lib.clip_or_first("") {
                return clip.frames.len().max(1) as u32;
            }
        }
        return 1;
    }
    // A single picture is a row of square cells (the sheet rule of Sprite Mode Multiple).
    sheet_cells(&resolve_path(path, project)).unwrap_or(1)
}

/// Cells in a horizontal sheet of square frames: width / height, read from the PNG header.
fn sheet_cells(full: &Path) -> Option<u32> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::SystemTime;
    // Drawing asks once per particle per frame: remember the answer until the file changes.
    type CellCache = HashMap<PathBuf, (SystemTime, Option<u32>)>;
    static CACHE: Mutex<Option<CellCache>> = Mutex::new(None);
    let stamp = std::fs::metadata(full).and_then(|m| m.modified()).ok()?;
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = cache.get_or_insert_with(HashMap::new);
    if let Some((when, cells)) = map.get(full) {
        if *when == stamp {
            return *cells;
        }
    }
    let cells = read_sheet_cells(full);
    map.insert(full.to_path_buf(), (stamp, cells));
    cells
}

fn read_sheet_cells(full: &Path) -> Option<u32> {
    use std::io::Read;
    let mut head = [0u8; 24];
    std::fs::File::open(full).ok()?.read_exact(&mut head).ok()?;
    if &head[..8] != b"\x89PNG\r\n\x1a\n" || &head[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(head[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(head[20..24].try_into().ok()?);
    (h > 0 && w >= h).then(|| (w / h).max(1))
}

/// Picture path (project-relative when `path` was relative) and UV for one frame index.
pub fn frame_picture(path: &str, index: u32, project: Option<&Path>) -> (String, [f32; 4]) {
    let count = frame_count(path, project).max(1);
    let i = index.min(count.saturating_sub(1));
    if frames::is_frames_path(path) {
        let full = resolve_path(path, project);
        if let Some(lib) = frames::load_cached(&full) {
            if let Some(clip) = lib.clip_or_first("") {
                if let Some(frame) = clip.frames.get(i as usize) {
                    let rel_base = Path::new(path).parent().unwrap_or_else(|| Path::new(""));
                    let joined = rel_base.join(&frame.image);
                    let rel = joined.to_string_lossy().replace('\\', "/");
                    return (rel, frame.uv);
                }
            }
        }
        return (path.to_string(), [0.0, 0.0, 1.0, 1.0]);
    }
    let u0 = i as f32 / count as f32;
    let u1 = (i + 1) as f32 / count as f32;
    (path.to_string(), [u0, 0.0, u1, 1.0])
}

fn resolve_path(path: &str, project: Option<&Path>) -> PathBuf {
    let raw = Path::new(path);
    match project {
        Some(root) if raw.is_relative() => root.join(raw),
        _ => raw.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_picture_sheet_has_one_frame_per_square_cell() {
        let dir = std::env::temp_dir().join(format!("seal-flip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sheet.png");
        // Only the header is read: signature, IHDR length and tag, width 32, height 8.
        let mut head = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        head.extend_from_slice(&32u32.to_be_bytes());
        head.extend_from_slice(&8u32.to_be_bytes());
        std::fs::write(&path, head).unwrap();
        assert_eq!(super::frame_count("sheet.png", Some(&dir)), 4);
        assert_eq!(super::frame_picture("sheet.png", 1, Some(&dir)).1, [0.25, 0.0, 0.5, 1.0]);
        assert_eq!(super::frame_count("missing.png", Some(&dir)), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
