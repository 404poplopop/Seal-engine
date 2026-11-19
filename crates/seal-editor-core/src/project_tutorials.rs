//! In-project tutorials: numbered steps shipped with templates, progress autosaved.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use seal_sealtext::{parse, SealValue};

use crate::error::CoreError;
use crate::session::EditorSession;
use crate::PlayMode;

pub const TUTORIAL_DIR: &str = "tutorial";
pub const GUIDE_FILE: &str = "guide.sealtutorial";
pub const PROGRESS_FILE: &str = "progress.sealtutorial";
pub const TEMPLATE_MARKER: &str = "template_id.txt";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TutorialStep {
    pub id: String,
    pub number: u32,
    pub title: String,
    pub body: String,
    /// `play`, `select`, `object_tag`, `recipe_import`
    pub detect: String,
    pub target: String,
}

#[derive(Clone, Debug, Default)]
pub struct TutorialGuide {
    pub template_id: String,
    pub steps: Vec<TutorialStep>,
}

pub fn write_shipped(folder: &Path, template_id: &str) -> Result<(), CoreError> {
    let guide = guide_for_template(template_id);
    std::fs::create_dir_all(folder.join(TUTORIAL_DIR))?;
    std::fs::write(folder.join(TEMPLATE_MARKER), template_id)?;
    std::fs::write(folder.join(TUTORIAL_DIR).join(GUIDE_FILE), guide_to_text(&guide))?;
    let progress = "schema_version = 1\ndone = []\n";
    std::fs::write(folder.join(TUTORIAL_DIR).join(PROGRESS_FILE), progress)?;
    Ok(())
}

pub fn ensure_legacy_templates(folder: &Path, template_id: &str) -> Result<(), CoreError> {
    let guide_path = folder.join(TUTORIAL_DIR).join(GUIDE_FILE);
    if guide_path.is_file() {
        return Ok(());
    }
    write_shipped(folder, template_id)
}

fn guide_for_template(id: &str) -> TutorialGuide {
    let steps: Vec<(&str, &str, &str, &str, &str, &str)> = match id {
        "2d" => vec![
            (
                "open_outliner",
                "1",
                "Open the Outliner",
                "Find the scene tree on the left.",
                "select",
                "World",
            ),
            (
                "press_play",
                "2",
                "Press Play",
                "Start play mode to test the empty scene.",
                "play",
                "",
            ),
        ],
        "3d" => vec![
            (
                "pick_player",
                "1",
                "Select Player Start",
                "Choose the player start in the Outliner.",
                "select",
                "Player Start",
            ),
            ("press_play", "2", "Press Play", "Walk in the 3D scene with W, A, S, D.", "play", ""),
        ],
        "puzzle" => vec![
            ("press_play", "1", "Press Play", "Open the puzzle from the main menu.", "play", ""),
            (
                "toggle_cell",
                "2",
                "Flip a cell",
                "Click a lit cell to toggle it and neighbors.",
                "score",
                "progress",
            ),
        ],
        "topdown" => vec![
            ("press_play", "1", "Press Play", "Enter the village scene.", "play", ""),
            (
                "move_hero",
                "2",
                "Move the hero",
                "Walk with W, A, S, D or the gamepad stick.",
                "key",
                "W",
            ),
        ],
        "platformer" => vec![
            ("press_play", "1", "Press Play", "Run level 1.", "play", ""),
            (
                "collect_coin",
                "2",
                "Collect a coin",
                "Touch a coin along the level.",
                "score",
                "coins",
            ),
        ],
        "first_person" => vec![
            ("press_play", "1", "Press Play", "Enter the arena in first person.", "play", ""),
            (
                "look_around",
                "2",
                "Look around",
                "Hold the right mouse button and move the mouse while playing.",
                "mouse",
                "",
            ),
            (
                "reach_goal",
                "3",
                "Reach the goal pickup",
                "Walk to the gold sphere.",
                "score",
                "goal",
            ),
        ],
        "third_person" => vec![
            ("press_play", "1", "Press Play", "Enter the arena behind the player.", "play", ""),
            (
                "orbit_camera",
                "2",
                "Orbit the camera",
                "Hold the right mouse button and move the mouse to orbit.",
                "mouse",
                "",
            ),
            (
                "reach_goal",
                "3",
                "Reach the goal pickup",
                "Walk to the gold sphere.",
                "score",
                "goal",
            ),
        ],
        _ => vec![("press_play", "1", "Press Play", "Start play mode.", "play", "")],
    };
    TutorialGuide {
        template_id: id.to_string(),
        steps: steps
            .into_iter()
            .map(|(sid, num, title, body, detect, target)| TutorialStep {
                id: sid.into(),
                number: num.parse().unwrap_or(1),
                title: title.into(),
                body: body.into(),
                detect: detect.into(),
                target: target.into(),
            })
            .collect(),
    }
}

fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn guide_to_text(guide: &TutorialGuide) -> String {
    let mut out = String::from("schema_version = 1\n");
    out.push_str(&format!("template = {}\n", quote(&guide.template_id)));
    out.push_str("steps = [\n");
    for step in &guide.steps {
        out.push_str(&format!(
            "  {{ id = {}, number = {}, title = {}, body = {}, detect = {}, target = {} }},\n",
            quote(&step.id),
            step.number,
            quote(&step.title),
            quote(&step.body),
            quote(&step.detect),
            quote(&step.target)
        ));
    }
    out.push_str("]\n");
    out
}

pub fn load_guide(root: &Path) -> Result<Option<TutorialGuide>, CoreError> {
    let path = root.join(TUTORIAL_DIR).join(GUIDE_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    parse_guide(&text).map(Some)
}

fn parse_guide(text: &str) -> Result<TutorialGuide, CoreError> {
    let raw = parse(text).map_err(|e| CoreError::new(e.to_string()))?;
    let template_id = match raw.get("template") {
        Some(SealValue::String(s)) => s.clone(),
        _ => String::new(),
    };
    let mut steps = Vec::new();
    if let Some(SealValue::Array(items)) = raw.get("steps") {
        for item in items {
            let Some(map) = item.as_map() else { continue };
            steps.push(TutorialStep {
                id: field_str(map, "id"),
                number: field_str(map, "number").parse().unwrap_or(1),
                title: field_str(map, "title"),
                body: field_str(map, "body"),
                detect: field_str(map, "detect"),
                target: field_str(map, "target"),
            });
        }
    }
    Ok(TutorialGuide { template_id, steps })
}

fn field_str(map: &[(String, SealValue)], key: &str) -> String {
    map.iter()
        .find(|(name, _)| name == key)
        .and_then(|(_, value)| match value {
            // Step numbers are written bare (`number = 2`), the rest as strings.
            SealValue::String(s) | SealValue::Number(s) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or("")
        .to_string()
}

pub fn load_done(root: &Path) -> Result<BTreeSet<String>, CoreError> {
    let path = root.join(TUTORIAL_DIR).join(PROGRESS_FILE);
    if !path.is_file() {
        return Ok(BTreeSet::new());
    }
    let text = std::fs::read_to_string(&path)?;
    if text.trim().is_empty() {
        return Ok(BTreeSet::new());
    }
    let raw = match parse(&text) {
        Ok(raw) => raw,
        Err(_) => return Ok(BTreeSet::new()),
    };
    let mut done = BTreeSet::new();
    if let Some(SealValue::Array(items)) = raw.get("done") {
        for item in items {
            if let SealValue::String(s) = item {
                done.insert(s.clone());
            }
        }
    }
    Ok(done)
}

pub fn save_done(root: &Path, done: &BTreeSet<String>) -> Result<(), CoreError> {
    std::fs::create_dir_all(root.join(TUTORIAL_DIR))?;
    let mut text = String::from("schema_version = 1\ndone = [\n");
    for id in done {
        text.push_str(&format!("  {:?},\n", id));
    }
    text.push_str("]\n");
    crate::persist::atomic_write(&root.join(TUTORIAL_DIR).join(PROGRESS_FILE), text.as_bytes())
}

pub fn template_id_from_project(root: &Path) -> Option<String> {
    std::fs::read_to_string(root.join(TEMPLATE_MARKER))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn progress_path(root: &Path) -> PathBuf {
    root.join(TUTORIAL_DIR).join(PROGRESS_FILE)
}

/// Called after a project is opened in the editor.
pub fn on_project_open(session: &mut EditorSession) {
    let Some(root) = session.project.as_ref().map(|p| p.root.clone()) else {
        return;
    };
    if let Some(id) = template_id_from_project(&root) {
        let _ = ensure_legacy_templates(&root, &id);
    }
    if load_guide(&root).ok().flatten().is_some() {
        session.reveal_panel = Some("tutorial".into());
    }
}

pub fn open_panel(session: &mut EditorSession) {
    session.reveal_panel = Some("tutorial".into());
}

pub fn tick(session: &mut EditorSession) {
    let Some(root) = session.project.as_ref().map(|p| p.root.clone()) else {
        return;
    };
    let Ok(Some(guide)) = load_guide(&root) else {
        return;
    };
    let Ok(mut done) = load_done(&root) else {
        return;
    };
    let before = done.len();
    for step in &guide.steps {
        if done.contains(&step.id) {
            continue;
        }
        if detect_step(session, step) {
            done.insert(step.id.clone());
        }
    }
    if done.len() != before {
        let _ = save_done(&root, &done);
    }
}

fn detect_step(session: &EditorSession, step: &TutorialStep) -> bool {
    match step.detect.as_str() {
        "play" => session.world.mode == PlayMode::Playing,
        "select" => session
            .models
            .selection
            .as_deref()
            .is_some_and(|id| entity_matches(session, id, &step.target)),
        "key" => {
            session.world.mode == PlayMode::Playing
                && session.world.play_keys.iter().any(|(k, down)| *down && k == &step.target)
        }
        "mouse" => {
            session.world.mode == PlayMode::Playing
                && (session.world.controls.mouse_delta[0].abs() > 0.5
                    || session.world.controls.mouse_delta[1].abs() > 0.5)
        }
        "score" => {
            session
                .world
                .play_game()
                .and_then(|g| g.scores.get(step.target.as_str()))
                .copied()
                .unwrap_or(0)
                > 0
        }
        "recipe_import" => {
            session.models.panel_states.get("recipe.last_import") == Some(&step.target)
        }
        _ => false,
    }
}

fn entity_matches(session: &EditorSession, id: &str, target: &str) -> bool {
    session.world.entities.iter().any(|e| {
        (e.id == id || e.name == id) && (e.name == target || e.id == target || e.tag == target)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_project, EditorSession, Layout};

    #[test]
    fn guide_round_trip_and_steps_for_every_template() {
        for id in ["2d", "3d", "puzzle", "topdown", "platformer", "first_person", "third_person"] {
            let g = guide_for_template(id);
            let text = guide_to_text(&g);
            let back = parse_guide(&text).unwrap();
            assert_eq!(back.template_id, id);
            assert!(!back.steps.is_empty(), "{id}");
            // Step numbers survive the round trip (they were all read back as 1).
            let numbers: Vec<u32> = back.steps.iter().map(|s| s.number).collect();
            let want: Vec<u32> = g.steps.iter().map(|s| s.number).collect();
            assert_eq!(numbers, want, "{id}");
        }
    }

    #[test]
    fn shipped_tutorial_loads_after_create() {
        let dir = std::env::temp_dir().join(format!("seal-tut-ship-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        create_project(&dir, "Game").unwrap();
        write_shipped(&dir, "first_person").unwrap();
        let guide = load_guide(&dir).unwrap().expect("guide");
        assert_eq!(guide.template_id, "first_person");
        assert_eq!(guide.steps.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn play_step_marks_done() {
        let dir = std::env::temp_dir().join(format!("seal-tut-play-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        create_project(&dir, "Game").unwrap();
        write_shipped(&dir, "2d").unwrap();
        let mut session = EditorSession::new(Layout::default());
        session
            .dispatch(
                "file.open-project",
                &crate::CommandArgs::new(&[("path", dir.to_str().unwrap())]),
            )
            .unwrap();
        session.world.play();
        tick(&mut session);
        let done = load_done(&dir).unwrap();
        assert!(done.contains("press_play"), "{done:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
