//! Frame events on a sprite animation, and the finished event for a clip
//! that plays once.
//!
//! A frame's event fires when that frame starts, once per pass, in order.
//! A step that crosses several frames fires each of them. `animation_finished`
//! fires once when a clip that does not loop reaches its end.

use std::collections::BTreeMap;
use std::path::Path;

use crate::frames::{self, Clip};
use crate::Entity;

/// Graph event fired on the object when a frame with a name starts.
pub const EVENT: &str = "animation_event";
/// Graph event fired on the object when a clip that does not loop ends.
pub const FINISHED: &str = "animation_finished";

/// How far an object's clip had been sampled.
#[derive(Clone, Debug, PartialEq)]
pub struct Cursor {
    pub clip: String,
    pub start: f32,
    pub elapsed: f32,
}

/// One graph event to fire on an object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cue {
    pub entity: String,
    pub event: &'static str,
    pub text: String,
}

/// Frames whose start falls in the step, and whether the clip just finished.
///
/// `prev` is `None` before the clip has been sampled, so frame 0 counts.
/// The third value is the elapsed time to remember; it stays at `now`.
pub fn crossings(clip: &Clip, prev: Option<f32>, now: f32) -> (Vec<usize>, bool, f32) {
    let n = clip.frames.len();
    if n == 0 || clip.fps <= 0.0 || !now.is_finite() {
        return (Vec::new(), false, 0.0);
    }
    let now = now.max(0.0);
    let last = (n as u64) - 1;
    let shown = clip.index_unwrapped(now);
    let end = if clip.looping { shown } else { shown.min(last) };
    let from = match prev {
        None => 0,
        Some(p) => clip.index_unwrapped(p).saturating_add(1),
    };
    let mut frames = Vec::new();
    if from <= end {
        let mut i = from;
        while i <= end {
            frames.push((i % n as u64) as usize);
            if frames.len() == 4096 {
                break;
            }
            if i == u64::MAX {
                break;
            }
            i += 1;
        }
    }
    let length = clip.length();
    let finished = !clip.looping
        && length > 0.0
        && now + 1e-5 >= length
        && prev.is_none_or(|p| p + 1e-5 < length);
    (frames, finished, now)
}

/// Events for this play step. Updates `cursor` even when nothing fires, so a
/// frame is not announced twice.
pub fn collect(
    entities: &[Entity],
    project: Option<&Path>,
    play_time: f32,
    cursor: &mut BTreeMap<String, Cursor>,
) -> Vec<Cue> {
    let mut out = Vec::new();
    for entity in entities {
        if !frames::is_frames_path(&entity.sprite) {
            continue;
        }
        let Some(clip) = load_clip(entity, project) else { continue };
        let elapsed = (play_time - entity.anim_start).max(0.0);
        let prev = match cursor.get(&entity.id) {
            Some(seen)
                if seen.clip == clip.name && (seen.start - entity.anim_start).abs() <= 1e-4 =>
            {
                Some(seen.elapsed)
            }
            _ => None,
        };
        let (frames_hit, finished, sampled) = crossings(&clip, prev, elapsed);
        cursor.insert(
            entity.id.clone(),
            Cursor { clip: clip.name.clone(), start: entity.anim_start, elapsed: sampled },
        );
        for index in frames_hit {
            let Some(frame) = clip.frames.get(index) else { continue };
            if frame.event.is_empty() {
                continue;
            }
            out.push(Cue { entity: entity.id.clone(), event: EVENT, text: frame.event.clone() });
        }
        if finished {
            out.push(Cue { entity: entity.id.clone(), event: FINISHED, text: String::new() });
        }
    }
    out
}

fn load_clip(entity: &Entity, project: Option<&Path>) -> Option<Clip> {
    let raw = Path::new(&entity.sprite);
    let full = match project {
        Some(root) if raw.is_relative() => root.join(raw),
        _ => raw.to_path_buf(),
    };
    let lib = frames::load_cached(&full)?;
    lib.clip_or_first(&entity.animation).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::{Clip, Frame};

    fn clip(looping: bool, names: &[&str]) -> Clip {
        let mut clip = Clip::new("run");
        clip.fps = 10.0;
        clip.looping = looping;
        clip.frames = names
            .iter()
            .map(|name| {
                let mut frame = Frame::whole("a.png");
                frame.event = (*name).into();
                frame
            })
            .collect();
        clip
    }

    #[test]
    fn events_fire_once_per_pass_and_a_long_step_keeps_order() {
        let clip = clip(true, &["one", "two", "three", ""]);
        let (hit, done, _) = crossings(&clip, None, 0.0);
        assert_eq!(hit, vec![0]);
        assert!(!done);
        let (hit, _, _) = crossings(&clip, Some(0.0), 0.05);
        assert!(hit.is_empty(), "staying on the frame does not fire again");
        let (hit, done, _) = crossings(&clip, Some(0.0), 0.35);
        assert_eq!(hit, vec![1, 2, 3]);
        assert!(!done, "a looping clip does not finish");
        let (hit, _, _) = crossings(&clip, Some(0.35), 0.45);
        assert_eq!(hit, vec![0], "the next pass starts at the first frame");
    }

    #[test]
    fn a_clip_that_does_not_loop_finishes_once() {
        let clip = clip(false, &["hit", ""]);
        let (hit, done, _) = crossings(&clip, None, 0.25);
        assert_eq!(hit, vec![0, 1]);
        assert!(done);
        let (hit, done, _) = crossings(&clip, Some(0.25), 0.5);
        assert!(hit.is_empty());
        assert!(!done);
        let (hit, done, _) = crossings(&clip, None, 0.05);
        assert_eq!(hit, vec![0]);
        assert!(!done, "the first frame has not reached the end");
    }
}
