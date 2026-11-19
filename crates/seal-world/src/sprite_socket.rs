//! Named points on a sprite animation, and objects that ride on them.
//!
//! A socket is a point on a frame, from 0 to 1 across and down the frame.
//! An object whose `socket` names one of its parent's sockets is moved there
//! each step. A flip mirrors the point. A frame that omits a name keeps the
//! point from the closest earlier frame.

use std::path::Path;

use crate::frames::{self, Clip};
use crate::Entity;

/// World offset from a parent for a socket at `at` (0..1, origin top left, v down).
pub fn offset(at: [f32; 2], flip: [bool; 2], scale: [f32; 2], rotation_z: f32) -> [f32; 2] {
    let u = if flip[0] { 1.0 - at[0] } else { at[0] };
    let v = if flip[1] { 1.0 - at[1] } else { at[1] };
    let dx = (u - 0.5) * scale[0];
    let dy = (0.5 - v) * scale[1];
    let rad = rotation_z.to_radians();
    let (s, c) = rad.sin_cos();
    [dx * c - dy * s, dx * s + dy * c]
}

/// The socket on frame `index`, or on the closest earlier frame that has it.
pub fn point_at(clip: &Clip, index: usize, name: &str) -> Option<[f32; 2]> {
    if clip.frames.is_empty() || name.is_empty() {
        return None;
    }
    let index = index.min(clip.frames.len() - 1);
    for step in 0..=index {
        let i = index - step;
        if let Some(socket) = clip.frames[i].sockets.iter().find(|s| s.name == name) {
            return Some(socket.at);
        }
    }
    None
}

/// Socket names on `clip` (or the first clip), in the order they first appear.
pub fn names_of(parent: &Entity, project: Option<&Path>) -> Vec<String> {
    let Some(clip) = clip_of(parent, project) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for frame in &clip.frames {
        for socket in &frame.sockets {
            if !names.iter().any(|n| n == &socket.name) {
                names.push(socket.name.clone());
            }
        }
    }
    names
}

/// Where `child` should sit on its parent's socket, if it has one.
pub fn place_of(
    child: &Entity,
    parent: &Entity,
    project: Option<&Path>,
    seconds: f32,
) -> Option<[f32; 3]> {
    if child.socket.is_empty() {
        return None;
    }
    let clip = clip_of(parent, project)?;
    let index = frame_index(parent, &clip, seconds);
    let at = point_at(&clip, index, &child.socket)?;
    let [x, y] = offset(at, parent.flip, [parent.scale[0], parent.scale[1]], parent.rotation[2]);
    Some([parent.translation[0] + x, parent.translation[1] + y, parent.translation[2]])
}

fn frame_index(parent: &Entity, clip: &Clip, seconds: f32) -> usize {
    match parent.frame_override {
        Some(f) => (f as usize).min(clip.frames.len().saturating_sub(1)),
        None => {
            clip.frame_at(if seconds > 0.0 { (seconds - parent.anim_start).max(0.0) } else { 0.0 })
        }
    }
}

fn clip_of(parent: &Entity, project: Option<&Path>) -> Option<Clip> {
    if !frames::is_frames_path(&parent.sprite) {
        return None;
    }
    let raw = Path::new(&parent.sprite);
    let full = match project {
        Some(root) if raw.is_relative() => root.join(raw),
        _ => raw.to_path_buf(),
    };
    let lib = frames::load_cached(&full)?;
    lib.clip_or_first(&parent.animation).cloned()
}

/// Move every object that names a socket onto that point. Parents are applied
/// first, so a chain of sockets settles in one call.
pub fn apply(entities: &mut [Entity], project: Option<&Path>, seconds: f32) {
    let n = entities.len();
    for _ in 0..n {
        let mut moves = Vec::new();
        for (i, child) in entities.iter().enumerate() {
            if child.socket.is_empty() {
                continue;
            }
            let Some(parent_id) = child.parent.as_deref() else { continue };
            let Some(parent) = entities.iter().find(|e| e.id == parent_id) else { continue };
            if let Some(at) = place_of(child, parent, project, seconds) {
                if child.translation != at {
                    moves.push((i, at));
                }
            }
        }
        if moves.is_empty() {
            break;
        }
        for (i, at) in moves {
            entities[i].translation = at;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::{Clip, Frame, Socket, SpriteFrames};

    fn frame(sockets: Vec<Socket>) -> Frame {
        Frame { sockets, ..Frame::cell("hero.png", [0.0, 0.0, 1.0, 1.0]) }
    }

    #[test]
    fn socket_point_changes_per_frame_and_mirrors_when_flipped() {
        let clip = Clip {
            name: "idle".into(),
            fps: 8.0,
            looping: true,
            frames: vec![
                frame(vec![Socket { name: "hand".into(), at: [0.8, 0.2] }]),
                frame(vec![Socket { name: "hand".into(), at: [0.2, 0.4] }]),
                frame(vec![]),
            ],
        };
        assert_eq!(point_at(&clip, 0, "hand"), Some([0.8, 0.2]));
        assert_eq!(point_at(&clip, 1, "hand"), Some([0.2, 0.4]));
        assert_eq!(point_at(&clip, 2, "hand"), Some([0.2, 0.4]), "a frame keeps the earlier point");
        let still = offset([0.75, 0.25], [false, false], [2.0, 2.0], 0.0);
        let flipped = offset([0.75, 0.25], [true, false], [2.0, 2.0], 0.0);
        assert!((still[0] - 0.5).abs() < 1e-4 && (still[1] - 0.5).abs() < 1e-4, "{still:?}");
        assert!((flipped[0] + 0.5).abs() < 1e-4 && (flipped[1] - 0.5).abs() < 1e-4, "{flipped:?}");
    }

    #[test]
    fn frames_file_round_trip_keeps_sockets_and_rejects_a_damaged_line() {
        let text = "\
# Seal sprite animations
animation idle fps=8 loop=yes
frame hero.png 0 0 0.5 1
socket hand 0.8 0.2
frame hero.png 0.5 0 1 1
socket hand 0.2 0.4
socket head 0.5 0.1
";
        let lib = SpriteFrames::parse(text).unwrap();
        assert_eq!(lib.to_text(), text);
        assert!(SpriteFrames::parse("animation idle\nsocket hand 0 0\n").is_err());
        assert!(SpriteFrames::parse("animation idle\nframe hero.png\nsocket hand no\n").is_err());
    }
}
