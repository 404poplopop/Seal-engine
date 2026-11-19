//! The sequence open in the Timeline: property tracks, undo and autosave.
//!
//! Every change is written straight back to the `.seq` file, so closing the
//! window never loses a key. Undo and redo apply while the Timeline panel is focused.

use std::path::{Path, PathBuf};

use seal_graph::{Graph, Ty, Value};
use seal_world::seal_scene::sequence::{Ease, PropKey, PropValue, PropertyTrack, Sequence};

use seal_world::gameui::{Screen, SCREEN_EXT};
pub use seal_world::seal_scene::sequence::PropertyTrack as TimelinePropertyTrack;
use seal_world::{entity_field_paths, ui_node_value, Entity};

use crate::command::{always_enabled, CommandArgs, CommandSpec};
use crate::error::CoreError;
use crate::persist::atomic_write;
use crate::sequences::{TIMELINE_SEQ_DRAFT, TIMELINE_TIME_DRAFT};
use crate::session::EditorSession;

const UNDO_LIMIT: usize = 200;

/// The sequence file the Timeline is editing.
#[derive(Clone, Debug)]
pub struct TimelineDoc {
    pub path: PathBuf,
    /// Project-relative path.
    pub label: String,
    pub sequence: Sequence,
    pub save_error: Option<String>,
    past: Vec<Sequence>,
    future: Vec<Sequence>,
    /// Sequence before the current key drag, so the whole drag is one undo step.
    drag: Option<Sequence>,
}

impl TimelineDoc {
    fn open(root: &Path, label: &str) -> Result<Self, CoreError> {
        let path = root.join(label);
        let text = std::fs::read_to_string(&path)?;
        let sequence = Sequence::parse(&text).map_err(CoreError::new)?;
        Ok(Self {
            path,
            label: label.to_string(),
            sequence,
            save_error: None,
            past: Vec::new(),
            future: Vec::new(),
            drag: None,
        })
    }

    fn save(&mut self) -> Result<(), CoreError> {
        let result = atomic_write(&self.path, self.sequence.to_text().as_bytes());
        self.save_error = result.as_ref().err().map(|e| e.to_string());
        result
    }

    fn edit(&mut self, change: impl FnOnce(&mut Sequence) -> bool) -> Result<(), CoreError> {
        let before = self.sequence.clone();
        if change(&mut self.sequence) && self.sequence != before {
            self.past.push(before);
            if self.past.len() > UNDO_LIMIT {
                self.past.remove(0);
            }
            self.future.clear();
            self.save()
        } else {
            self.sequence = before;
            Ok(())
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.past.pop() else { return false };
        self.future.push(std::mem::replace(&mut self.sequence, prev));
        self.drag = None;
        let _ = self.save();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.future.pop() else { return false };
        self.past.push(std::mem::replace(&mut self.sequence, next));
        self.drag = None;
        let _ = self.save();
        true
    }

    fn begin_drag(&mut self) {
        if self.drag.is_none() {
            self.drag = Some(self.sequence.clone());
        }
    }

    fn drag_key(&mut self, entity: &str, path: &str, from: f32, to: f32) -> Result<(), CoreError> {
        if let Some(track) = track_mut(&mut self.sequence, entity, path) {
            let _ = retimed(track, from, to.max(0.0));
        }
        self.save()
    }

    fn end_drag(&mut self) -> Result<(), CoreError> {
        let Some(before) = self.drag.take() else { return Ok(()) };
        if before == self.sequence {
            return Ok(());
        }
        let now = std::mem::replace(&mut self.sequence, before);
        self.past.push(self.sequence.clone());
        if self.past.len() > UNDO_LIMIT {
            self.past.remove(0);
        }
        self.future.clear();
        self.sequence = now;
        self.save()
    }
}

/// Open `rel` when it is not already the Timeline document.
pub fn sync_timeline(session: &mut EditorSession, rel: &str) -> Result<(), CoreError> {
    if session.timeline.as_ref().is_some_and(|doc| doc.label == rel) {
        return Ok(());
    }
    let root = session
        .project
        .as_ref()
        .ok_or_else(|| CoreError::new("open a project first"))?
        .root
        .clone();
    match TimelineDoc::open(&root, rel) {
        Ok(doc) => {
            session.timeline = Some(doc);
            Ok(())
        }
        Err(err) => {
            session.timeline = None;
            Err(err)
        }
    }
}

fn with_doc(
    session: &mut EditorSession,
    args: &CommandArgs,
    edit: impl FnOnce(&mut TimelineDoc) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    let path = seq_path(session, args)?;
    sync_timeline(session, &path)?;
    session.focus_panel("timeline");
    let Some(doc) = session.timeline.as_mut() else {
        return Err(CoreError::new("choose a sequence"));
    };
    edit(doc)
}

fn seq_path(session: &EditorSession, args: &CommandArgs) -> Result<String, CoreError> {
    args.get("path")
        .map(str::to_string)
        .or_else(|| session.draft(TIMELINE_SEQ_DRAFT))
        .filter(|path| !path.is_empty())
        .ok_or_else(|| CoreError::new("choose a sequence"))
}

/// Property paths for the selected object: component fields, script variables and UI controls.
pub fn property_choices(session: &EditorSession, entity_id: &str) -> Vec<String> {
    let Some(entity) = find_entity(&session.world, entity_id) else {
        return Vec::new();
    };
    let mut paths: Vec<String> =
        entity_field_paths(entity).into_iter().map(str::to_string).collect();
    paths.extend(script_paths(session, entity));
    if let Some(root) = session.project.as_ref().map(|project| project.root.clone()) {
        paths.extend(ui_paths(&root));
    }
    paths.sort();
    paths.dedup();
    paths
}

pub fn selected_entity_id(session: &EditorSession) -> Option<String> {
    let known = |id: &str| find_entity(&session.world, id).is_some();
    session
        .models
        .selection
        .clone()
        .filter(|id| known(id))
        .or_else(|| session.foundation.selection.iter().find(|id| known(id)).cloned())
}

fn find_entity<'a>(world: &'a seal_world::World, id: &str) -> Option<&'a Entity> {
    world.entities.iter().find(|entity| entity.id == id || entity.name == id)
}

fn script_paths(session: &EditorSession, entity: &Entity) -> Vec<String> {
    let Some(root) = session.project.as_ref().map(|project| &project.root) else {
        return Vec::new();
    };
    if entity.script.is_empty() {
        return Vec::new();
    }
    let Ok(text) = std::fs::read_to_string(root.join(&entity.script)) else {
        return Vec::new();
    };
    let Ok(graph) = Graph::from_text(&text) else {
        return Vec::new();
    };
    graph
        .vars
        .iter()
        .filter(|var| animatable_ty(var.ty))
        .map(|var| format!("script.{}", var.name))
        .collect()
}

fn animatable_ty(ty: Ty) -> bool {
    matches!(ty, Ty::Bool | Ty::Int | Ty::Float | Ty::Str | Ty::Vec2 | Ty::Vec3)
}

fn ui_paths(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    walk_ui(root, &mut out, 0);
    out
}

fn walk_ui(dir: &Path, out: &mut Vec<String>, depth: usize) {
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
            walk_ui(&path, out, depth + 1);
        } else if name.ends_with(&format!(".{SCREEN_EXT}")) {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Ok(screen) = Screen::from_text(&text) else { continue };
            for node in &screen.nodes {
                for prop in ui_props(&node.kind) {
                    out.push(format!("ui.{}.{}", node.name, prop));
                }
            }
        }
    }
}

fn ui_props(kind: &str) -> &'static [&'static str] {
    match kind {
        "slider" | "progress" => &["value", "min", "max", "visible", "enabled"],
        "toggle" => &["checked", "text", "visible", "enabled"],
        "label" | "button" => &["text", "font_size", "visible", "enabled", "color"],
        "input" => &["text", "visible", "enabled"],
        "dropdown" | "list" => &["selected", "visible", "enabled"],
        "image" | "panel" => &["color", "visible"],
        _ => &["visible", "enabled"],
    }
}

fn current_value(
    session: &EditorSession,
    entity: &str,
    path: &str,
) -> Result<PropValue, CoreError> {
    if let Some(value) = session.world.read_property(entity, path) {
        return Ok(value);
    }
    if let Some(name) = path.strip_prefix("script.") {
        if let Some(value) = script_default(session, entity, name) {
            return Ok(value);
        }
    }
    if let Some(rest) = path.strip_prefix("ui.") {
        if let Some((node, prop)) = rest.rsplit_once('.') {
            if let Some(value) = ui_file_value(session, node, prop) {
                return Ok(value);
            }
        }
    }
    Err(CoreError::new(format!("{path} is not a field on {entity}")))
}

fn script_default(session: &EditorSession, entity: &str, name: &str) -> Option<PropValue> {
    let found = find_entity(&session.world, entity)?;
    let root = &session.project.as_ref()?.root;
    if found.script.is_empty() {
        return None;
    }
    let text = std::fs::read_to_string(root.join(&found.script)).ok()?;
    let graph = Graph::from_text(&text).ok()?;
    let var = graph.vars.iter().find(|var| var.name == name)?;
    value_to_prop(&var.default)
}

fn value_to_prop(value: &Value) -> Option<PropValue> {
    Some(match value {
        Value::Bool(v) => PropValue::Bool(*v),
        Value::Int(v) => PropValue::Number(*v as f32),
        Value::Float(v) => PropValue::Number(*v as f32),
        Value::Str(v) => PropValue::Text(v.clone()),
        Value::Vec2(v) => PropValue::Vector(vec![v.x as f32, v.y as f32]),
        Value::Vec3(v) => PropValue::Vector(vec![v.x as f32, v.y as f32, v.z as f32]),
        _ => return None,
    })
}

fn ui_file_value(session: &EditorSession, node: &str, prop: &str) -> Option<PropValue> {
    let root = &session.project.as_ref()?.root;
    let mut found = None;
    walk_ui_value(root, node, prop, &mut found, 0);
    found
}

fn walk_ui_value(dir: &Path, node: &str, prop: &str, found: &mut Option<PropValue>, depth: usize) {
    if found.is_some() || depth > 8 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if found.is_some() {
            return;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "target" {
            continue;
        }
        if path.is_dir() {
            walk_ui_value(&path, node, prop, found, depth + 1);
        } else if name.ends_with(&format!(".{SCREEN_EXT}")) {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Ok(screen) = Screen::from_text(&text) else { continue };
            if let Some(index) = screen.find(node) {
                *found = ui_node_value(&screen.nodes[index], prop);
            }
        }
    }
}

fn track_index(seq: &Sequence, entity: &str, path: &str) -> Option<usize> {
    seq.properties.iter().position(|track| track.entity == entity && track.path == path)
}

fn track_mut<'a>(seq: &'a mut Sequence, entity: &str, path: &str) -> Option<&'a mut PropertyTrack> {
    seq.properties.iter_mut().find(|track| track.entity == entity && track.path == path)
}

fn nearest(track: &PropertyTrack, time: f32) -> Option<usize> {
    track
        .keys
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (a.time - time).abs().total_cmp(&(b.time - time).abs()))
        .map(|(index, _)| index)
}

fn retimed(track: &mut PropertyTrack, from: f32, to: f32) -> bool {
    let Some(index) = nearest(track, from) else { return false };
    if !to.is_finite() {
        return false;
    }
    let saved = track.keys.clone();
    track.keys[index].time = to;
    track.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
    if track.keys.windows(2).any(|pair| pair[1].time <= pair[0].time) {
        track.keys = saved;
        return false;
    }
    true
}

fn playhead(session: &EditorSession, args: &CommandArgs) -> f32 {
    args.get("t")
        .and_then(|text| text.parse().ok())
        .or_else(|| session.draft(TIMELINE_TIME_DRAFT).and_then(|text| text.parse().ok()))
        .unwrap_or(0.0_f32)
        .max(0.0)
}

fn need<'a>(args: &'a CommandArgs, key: &str) -> Result<&'a str, CoreError> {
    args.get(key).ok_or_else(|| CoreError::new(format!("missing argument '{key}'")))
}

pub(crate) fn register(session: &mut EditorSession) -> Result<(), CoreError> {
    let specs: &[(&str, &str, crate::command::CommandHandler)] = &[
        ("sequence.property_add", "command-sequence-property-add", property_add),
        ("sequence.property_key", "command-sequence-property-key", property_key),
        ("sequence.property_ease", "command-sequence-property-ease", property_ease),
        ("sequence.property_move", "command-sequence-property-move", property_move),
        ("sequence.property_delete", "command-sequence-property-delete", property_delete),
        ("sequence.property_drag", "command-sequence-property-drag", property_drag),
        ("sequence.time_warp_add", "command-sequence-time-warp-add", time_warp_add),
        ("sequence.time_warp_clear", "command-sequence-time-warp-clear", time_warp_clear),
    ];
    for (id, title, handler) in specs {
        let spec = CommandSpec::new(*id, *title, "edit").menu(false).requires("project");
        session.commands.register(spec, always_enabled, *handler)?;
    }
    Ok(())
}

fn property_add(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let entity = need(args, "entity")?.to_string();
    let path = need(args, "field")?.to_string();
    with_doc(session, args, |doc| {
        doc.edit(|seq| {
            if track_index(seq, &entity, &path).is_some() || entity.is_empty() || path.is_empty() {
                return false;
            }
            seq.properties.push(PropertyTrack {
                entity: entity.clone(),
                path: path.clone(),
                keys: Vec::new(),
            });
            true
        })
    })
}

fn property_key(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let entity = need(args, "entity")?.to_string();
    let path = need(args, "field")?.to_string();
    let time = playhead(session, args);
    let value = current_value(session, &entity, &path)?;
    with_doc(session, args, |doc| {
        doc.edit(|seq| {
            let Some(track) = track_mut(seq, &entity, &path) else { return false };
            if let Some(key) = track.keys.iter_mut().find(|key| (key.time - time).abs() <= 0.0005) {
                key.value = value.clone();
                if key.value.steps() {
                    key.ease = Ease::Step;
                }
            } else {
                let ease = if value.steps() { Ease::Step } else { Ease::Linear };
                track.keys.push(PropKey { time, value: value.clone(), ease });
                track.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            }
            true
        })
    })
}

fn property_ease(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let entity = need(args, "entity")?.to_string();
    let path = need(args, "field")?.to_string();
    let ease = Ease::parse(need(args, "ease")?).ok_or_else(|| CoreError::new("unknown easing"))?;
    let time = playhead(session, args);
    with_doc(session, args, |doc| {
        doc.edit(|seq| {
            let Some(track) = track_mut(seq, &entity, &path) else { return false };
            let Some(index) = nearest(track, time) else { return false };
            let ease = if track.keys[index].value.steps() { Ease::Step } else { ease };
            if track.keys[index].ease == ease {
                return false;
            }
            track.keys[index].ease = ease;
            true
        })
    })
}

fn property_move(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let entity = need(args, "entity")?.to_string();
    let path = need(args, "field")?.to_string();
    let from: f32 =
        need(args, "from")?.parse().map_err(|_| CoreError::new("from must be a number"))?;
    let to: f32 = need(args, "to")?.parse().map_err(|_| CoreError::new("to must be a number"))?;
    with_doc(session, args, |doc| {
        doc.edit(|seq| {
            let Some(track) = track_mut(seq, &entity, &path) else { return false };
            retimed(track, from, to.max(0.0))
        })
    })
}

fn property_delete(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let entity = need(args, "entity")?.to_string();
    let path = need(args, "field")?.to_string();
    let time = playhead(session, args);
    with_doc(session, args, |doc| {
        doc.edit(|seq| {
            let Some(index) = track_index(seq, &entity, &path) else { return false };
            if seq.properties[index].keys.is_empty() {
                seq.properties.remove(index);
                return true;
            }
            let Some(key) = nearest(&seq.properties[index], time) else { return false };
            seq.properties[index].keys.remove(key);
            if seq.properties[index].keys.is_empty() {
                seq.properties.remove(index);
            }
            true
        })
    })
}

fn time_warp_add(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let clock = playhead(session, args);
    let content: f32 =
        need(args, "content")?.parse().map_err(|_| CoreError::new("content must be a number"))?;
    with_doc(session, args, |doc| {
        doc.edit(|seq| {
            let keys = &mut seq.time_warp.keys;
            if let Some(key) = keys.iter_mut().find(|(t, _)| (*t - clock).abs() <= 0.0005) {
                key.1 = content;
            } else {
                keys.push((clock.max(0.0), content));
                keys.sort_by(|a, b| a.0.total_cmp(&b.0));
            }
            true
        })
    })
}

fn time_warp_clear(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    with_doc(session, args, |doc| {
        doc.edit(|seq| {
            if seq.time_warp.keys.is_empty() {
                return false;
            }
            seq.time_warp.keys.clear();
            true
        })
    })
}

fn property_drag(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let phase = need(args, "phase")?.to_string();
    with_doc(session, args, |doc| match phase.as_str() {
        "begin" => {
            doc.begin_drag();
            Ok(())
        }
        "end" => doc.end_drag(),
        "move" => {
            let entity = need(args, "entity")?;
            let path = need(args, "field")?;
            let from: f32 =
                need(args, "from")?.parse().map_err(|_| CoreError::new("from must be a number"))?;
            let to: f32 =
                need(args, "to")?.parse().map_err(|_| CoreError::new("to must be a number"))?;
            doc.drag_key(entity, path, from, to)
        }
        _ => Err(CoreError::new("unknown drag phase")),
    })
}
