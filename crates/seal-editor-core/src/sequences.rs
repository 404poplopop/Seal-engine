//! Timeline support: find a project's .seq files, play one, scrub a paused one.

use std::path::Path;

use seal_world::seal_scene::sequence::Sequence;
use seal_world::PlayMode;

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::session::EditorSession;

/// Draft key holding the project path of the sequence shown in the Timeline.
pub const TIMELINE_SEQ_DRAFT: &str = "timeline-seq";
/// Draft key holding the Timeline playhead in seconds.
pub const TIMELINE_TIME_DRAFT: &str = "timeline-time";

/// Every .seq file under the project, as forward-slash project paths, sorted.
pub fn sequence_files(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>, depth: usize) {
        if depth > 8 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "target" {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out, depth + 1);
            } else if name.ends_with(".seq") {
                if let Ok(rel) = path.strip_prefix(root) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out, 0);
    out.sort();
    out
}

impl EditorSession {
    /// The project's sequence files (empty without a project).
    pub fn sequence_files(&self) -> Vec<String> {
        self.project.as_ref().map(|p| sequence_files(&p.root)).unwrap_or_default()
    }

    /// Read and parse a project sequence file.
    pub fn load_sequence(&self, path: &str) -> Result<Sequence, CoreError> {
        let root = self
            .project
            .as_ref()
            .ok_or_else(|| CoreError::new("open a project first"))?
            .root
            .clone();
        let text = std::fs::read_to_string(root.join(path))?;
        Sequence::parse(&text).map_err(|e| CoreError::new(format!("{path}: {e}")))
    }
}

fn path_arg(session: &EditorSession, args: &CommandArgs) -> Result<String, CoreError> {
    args.get("path")
        .map(str::to_string)
        .or_else(|| session.draft(TIMELINE_SEQ_DRAFT))
        .filter(|p| !p.is_empty())
        .ok_or_else(|| CoreError::new("choose a sequence"))
}

/// `sequence.play path=`: start play if needed, then run the sequence from its start.
pub(crate) fn sequence_play(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let path = path_arg(session, args)?;
    let seq = session.load_sequence(&path)?;
    if session.world.mode == PlayMode::Editing {
        session.world.project_dir = session.project.as_ref().map(|p| p.root.clone());
        session.world.play();
    } else if session.world.mode != PlayMode::Playing {
        session.world.play();
    }
    session.world.sequences.clear();
    session.world.play_sequence(seq);
    let _ = session.set_draft(TIMELINE_SEQ_DRAFT, &path);
    let _ = session.set_draft(TIMELINE_TIME_DRAFT, "0");
    session.forward_play_log();
    Ok(())
}

/// `sequence.scrub path= t=`: pause play and pose every object the sequence drives at `t`.
/// Outside play only the playhead moves, so scrubbing never edits the saved scene.
pub(crate) fn sequence_scrub(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let path = path_arg(session, args)?;
    let seq = session.load_sequence(&path)?;
    let t: f32 = args.get("t").and_then(|v| v.trim().parse().ok()).unwrap_or(0.0);
    let t = t.clamp(0.0, seq.length.max(0.0));
    if session.world.mode != PlayMode::Editing {
        session.world.pause();
        session.world.sequences.clear();
        session.world.scrub_sequence(&seq, t);
    }
    let _ = session.set_draft(TIMELINE_SEQ_DRAFT, &path);
    let _ = session.set_draft(TIMELINE_TIME_DRAFT, &format!("{t:.3}"));
    Ok(())
}

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

pub const COMMANDS: &[(&str, &str, &str, CommandFn)] =
    &[("timeline.new_sequence", "command-timeline-new-sequence", "file", new_sequence)];

/// `timeline.new_sequence`: write an empty five-second sequence under `sequences/` and show it.
fn new_sequence(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let root = session
        .project
        .as_ref()
        .ok_or_else(|| CoreError::new("open a project first"))?
        .root
        .clone();
    let dir = root.join("sequences");
    std::fs::create_dir_all(&dir)?;
    let rel = (1..)
        .map(|n| {
            if n == 1 {
                "sequences/sequence.seq".to_string()
            } else {
                format!("sequences/sequence {n}.seq")
            }
        })
        .find(|rel| !root.join(rel).exists())
        .expect("a free name");
    let empty = Sequence {
        length: 5.0,
        looping: false,
        tracks: vec![],
        properties: vec![],
        events: vec![],
        time_warp: seal_world::seal_scene::time_warp::TimeWarp::default(),
    };
    crate::persist::atomic_write(&root.join(&rel), empty.to_text().as_bytes())?;
    session.set_draft(TIMELINE_SEQ_DRAFT, &rel)?;
    session.set_draft(TIMELINE_TIME_DRAFT, "0")?;
    Ok(())
}

#[cfg(test)]
mod new_sequence_tests {
    use super::*;
    use crate::{CommandArgs, Layout};

    #[test]
    fn new_sequence_writes_a_parsable_file_and_shows_it() {
        let dir = std::env::temp_dir().join(format!("seal-newseq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut session = EditorSession::new(Layout::default());
        session.project = Some(crate::create_project(&dir, "Seq").unwrap());
        session.dispatch("timeline.new_sequence", &CommandArgs::new(&[])).unwrap();
        session.dispatch("timeline.new_sequence", &CommandArgs::new(&[])).unwrap();
        assert_eq!(
            session.sequence_files(),
            vec!["sequences/sequence 2.seq", "sequences/sequence.seq"]
        );
        assert_eq!(session.draft(TIMELINE_SEQ_DRAFT).as_deref(), Some("sequences/sequence 2.seq"));
        let seq = session.load_sequence("sequences/sequence.seq").unwrap();
        assert_eq!(seq.length, 5.0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
