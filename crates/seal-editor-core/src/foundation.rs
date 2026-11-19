use std::path::Path;

use crate::command::{CommandArgs, CommandSpec};
use crate::error::CoreError;
use crate::keymap::Chord;
use crate::kinds::KindRegistry;
use crate::layout::{Layout, DEFAULT_SIDEBAR_PINS};
use crate::session::EditorSession;
use crate::settings::SettingsStore;
use crate::when::WhenContext;

/// Shell services that later tools share: settings, notices, tasks, layouts, and the agent bus.
#[derive(Clone, Debug)]
pub struct Foundation {
    pub settings: SettingsStore,
    pub notices: Notifications,
    pub modal: Option<Modal>,
    pub clipboard: String,
    /// `text` is plain text. `scene` is a scene snapshot that also pastes as text.
    pub clipboard_kind: String,
    pub events: Vec<EditorEvent>,
    pub tasks: Vec<BackgroundTask>,
    pub kinds: KindRegistry,
    pub modules: Vec<String>,
    pub layouts: Vec<(String, String)>,
    pub previous_layout: Option<Layout>,
    pub selection: Vec<String>,
    pub focused_panel_id: String,
    pub text_input: bool,
    pub fullscreen: bool,
    pub layout_locked: bool,
    pub maximized_panel: bool,
    pub loading: bool,
    pub restart_safe: Option<bool>,
    pub agent_activity: String,
    pub dock_request: Option<DockSide>,
    /// Source panel id for `panel.duplicate` on the next frame.
    pub duplicate_panel: Option<String>,
    /// Panels removed from the dock and shown on the window edge rail.
    pub sidebar_pins: Vec<String>,
    /// When set, the pinned panel slides in over the layout.
    pub sidebar_open: Option<String>,
    /// Slide-up asset browser over the lower part of the window.
    pub asset_drawer_open: bool,
    /// Slide-up console over the lower part of the window.
    pub output_drawer_open: bool,
    pub open_panels: Vec<String>,
    pub console_epoch: u64,
    /// In-memory log records with a lower index stay hidden until the logger is cleared.
    pub console_clear_before: usize,
    pub reveal: Option<String>,
}

impl Foundation {
    pub fn new() -> Self {
        Self {
            settings: SettingsStore::builtin(),
            notices: Notifications::default(),
            modal: None,
            clipboard: String::new(),
            clipboard_kind: "text".into(),
            events: Vec::new(),
            tasks: Vec::new(),
            kinds: KindRegistry::default(),
            modules: Vec::new(),
            layouts: Vec::new(),
            previous_layout: None,
            selection: Vec::new(),
            focused_panel_id: "outliner".into(),
            text_input: false,
            fullscreen: false,
            layout_locked: false,
            maximized_panel: false,
            loading: false,
            restart_safe: None,
            agent_activity: String::new(),
            dock_request: None,
            duplicate_panel: None,
            sidebar_pins: DEFAULT_SIDEBAR_PINS.iter().map(|id| (*id).to_string()).collect(),
            sidebar_open: None,
            asset_drawer_open: false,
            output_drawer_open: false,
            open_panels: [
                "outliner",
                "viewport",
                "inspector",
                "world",
                "console",
                "content",
                "place",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            console_epoch: 0,
            console_clear_before: 0,
            reveal: None,
        }
    }
}

impl Default for Foundation {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub id: u64,
    pub text: String,
    pub error: bool,
    pub at_ms: u64,
}

const NOTICE_HISTORY_CAP: usize = 200;

#[derive(Clone, Debug, Default)]
pub struct Notifications {
    history: Vec<Notice>,
    next: u64,
    /// Notices up to this id came from an earlier session: history only, never toasts.
    restored_upto: u64,
    /// Toasts the user closed.
    dismissed: Vec<u64>,
}

impl Notifications {
    pub fn push(&mut self, text: impl Into<String>, error: bool, at_ms: u64) -> u64 {
        self.next = self.next.saturating_add(1);
        let id = self.next;
        self.history.push(Notice { id, text: text.into(), error, at_ms });
        let excess = self.history.len().saturating_sub(NOTICE_HISTORY_CAP);
        if excess > 0 {
            self.history.drain(..excess);
        }
        id
    }

    pub fn history(&self) -> &[Notice] {
        &self.history
    }

    pub fn restore(&mut self, items: Vec<Notice>) {
        self.next = items.iter().map(|notice| notice.id).max().unwrap_or(0);
        self.restored_upto = self.next;
        self.history = items;
    }

    /// Close one toast. It stays in the history.
    pub fn dismiss(&mut self, id: u64) {
        if !self.dismissed.contains(&id) {
            self.dismissed.push(id);
        }
    }

    /// At most four toasts from this session. Notices leave after `timeout_ms`; errors stay
    /// three times as long (or until closed) and remain in the notification history.
    pub fn visible(&self, now_ms: u64, timeout_ms: u64) -> Vec<&Notice> {
        let timeout = timeout_ms.max(1);
        let mut shown: Vec<&Notice> = self
            .history
            .iter()
            .rev()
            .filter(|notice| notice.id > self.restored_upto && !self.dismissed.contains(&notice.id))
            .filter(|notice| {
                let life = if notice.error { timeout.saturating_mul(3) } else { timeout };
                notice.at_ms <= now_ms && now_ms - notice.at_ms < life
            })
            .take(4)
            .collect();
        shown.reverse();
        shown
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalKind {
    Close,
    CloseScene,
    Discard,
    ResetState,
    ResetLayout,
    About,
    ProjectElsewhere,
    ContentPathConfirm,
    DeleteAssetInUse,
    MergeDuplicates,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Modal {
    pub kind: ModalKind,
    pub title: String,
    pub body: String,
    pub confirm: String,
    /// Third action. Empty means the dialog has only confirm and cancel.
    pub alternate: String,
    pub cancel: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalChoice {
    Confirm,
    Alternate,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorEvent {
    Command { id: String },
    Notice { text: String },
    Layout,
    Shutdown,
    DropUnsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockSide {
    Left,
    Right,
    Top,
    Bottom,
    Float,
    Close,
    Reload,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundTask {
    pub id: String,
    pub title: String,
    pub progress: f32,
    pub cancelled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusSnapshot {
    pub project: String,
    pub tasks: String,
    pub selection: String,
    pub hidden: bool,
    pub agent: String,
}

impl EditorSession {
    pub fn notify(&mut self, text: impl Into<String>, error: bool) {
        let text = text.into();
        self.foundation.notices.push(text.clone(), error, self.uptime_ms);
        self.foundation.events.push(EditorEvent::Notice { text: text.clone() });
        self.notice = Some(text);
        if let Err(err) = self.save_memory() {
            self.notice = Some(err.to_string());
        }
    }

    pub fn begin_close(&mut self) {
        if let Some(modal) = self.foundation.modal.as_ref() {
            if modal.kind == ModalKind::Close {
                self.shutdown();
                return;
            }
            self.foundation.modal = None;
        }
        let unsaved =
            self.scene_dirty || self.settings_dirty || self.documents.iter().any(|doc| doc.dirty());
        // Every way of closing (the X, Alt+F4, Task Manager's End task, File > Exit)
        // saves first and then closes without asking. Only a save that already
        // failed stops to ask, since closing then could lose work.
        if unsaved && !self.save_failed && self.foundation.modal.is_none() {
            self.save_on_close = true;
            self.discard_on_close = false;
            self.request_close = true;
            return;
        }
        if self.save_failed && self.foundation.modal.is_none() {
            let body = if self.save_failed { "modal-close-failed" } else { "modal-close-body" };
            let failed_only = close_failed_only(self);
            let (confirm, alternate) = if failed_only {
                ("modal-retry", "command-file-save-as")
            } else {
                ("modal-save-all", "modal-dont-save")
            };
            self.foundation.modal = Some(self.modal(
                ModalKind::Close,
                "modal-close-title",
                body,
                confirm,
                alternate,
                "modal-cancel",
            ));
            return;
        }
        if self.foundation.modal.is_none() {
            self.request_close = true;
        }
    }

    /// Close the scene document. A dirty scene asks Save, Don't Save, or Cancel.
    pub fn begin_close_scene_tab(&mut self) {
        if !self.scene_tab_open || self.foundation.modal.is_some() {
            return;
        }
        if !self.scene_dirty {
            self.scene_tab_open = false;
            return;
        }
        let mut modal = self.modal(
            ModalKind::CloseScene,
            "modal-close-title",
            "modal-close-scene",
            "command-file-save",
            "modal-dont-save",
            "modal-cancel",
        );
        let name = self.scene_file_name();
        modal.body = self.messages.format("modal-close-scene", &[("name", name.as_str())]);
        self.foundation.modal = Some(modal);
    }

    pub fn answer_modal(&mut self, confirm: bool) {
        self.choose_modal(if confirm { ModalChoice::Confirm } else { ModalChoice::Cancel });
    }

    pub fn choose_modal(&mut self, choice: ModalChoice) {
        let Some(modal) = self.foundation.modal.take() else {
            return;
        };
        if choice == ModalChoice::Cancel {
            match modal.kind {
                ModalKind::ContentPathConfirm => {
                    self.models.pending_content_path = None;
                }
                ModalKind::DeleteAssetInUse => {
                    self.models.pending_delete_path = None;
                }
                ModalKind::MergeDuplicates => {
                    self.models.pending_merge_duplicates = None;
                }
                _ => {}
            }
            return;
        }
        match modal.kind {
            ModalKind::Close => match choice {
                ModalChoice::Confirm if close_failed_only(self) => {
                    self.retry_save_now = true;
                }
                ModalChoice::Confirm => {
                    self.save_on_close = true;
                    self.request_close = true;
                }
                ModalChoice::Alternate if close_failed_only(self) => {
                    self.foundation.modal = Some(modal);
                    self.host_action = Some(crate::session::HostAction::SaveAs);
                }
                ModalChoice::Alternate => {
                    self.discard_on_close = true;
                    self.request_close = true;
                }
                ModalChoice::Cancel => {}
            },
            ModalKind::CloseScene => match choice {
                ModalChoice::Confirm => match self.save_scene() {
                    Err(err) => self.notice = Some(err.to_string()),
                    Ok(()) if self.scene_dirty => {
                        self.host_action = Some(crate::session::HostAction::SaveAs);
                    }
                    Ok(()) => self.scene_tab_open = false,
                },
                ModalChoice::Alternate => {
                    self.revert_scene_tab();
                }
                ModalChoice::Cancel => {}
            },
            ModalKind::Discard => {
                if let Err(err) = self.dispatch("edit.discard", &CommandArgs::default()) {
                    self.notice = Some(err.to_string());
                }
            }
            ModalKind::ResetState => {
                crate::session::apply_layout_reset(self);
            }
            ModalKind::ResetLayout => {
                if choice == ModalChoice::Confirm {
                    crate::session::apply_layout_reset(self);
                }
            }
            ModalKind::About => {}
            ModalKind::ProjectElsewhere => {
                if choice == ModalChoice::Confirm {
                    if let Err(err) =
                        crate::persist::write_focus_request(&crate::workspace::user_state_dir())
                    {
                        self.notice = Some(err.to_string());
                    }
                }
            }
            ModalKind::ContentPathConfirm => {
                if choice == ModalChoice::Confirm {
                    if let Err(err) = crate::asset_index::confirm_pending_content_path(self) {
                        self.notice = Some(err.to_string());
                    }
                } else {
                    self.models.pending_content_path = None;
                }
            }
            ModalKind::DeleteAssetInUse => {
                if choice == ModalChoice::Confirm {
                    if let Some(name) = self.models.pending_delete_path.take() {
                        if let Err(err) = self.dispatch(
                            "content.delete",
                            &CommandArgs::new(&[("name", name.as_str()), ("confirm", "yes")]),
                        ) {
                            self.notice = Some(err.to_string());
                        }
                    }
                } else {
                    self.models.pending_delete_path = None;
                }
            }
            ModalKind::MergeDuplicates => {
                if choice == ModalChoice::Confirm {
                    if let Err(err) = crate::asset_database_ops::apply_merge_duplicates(self) {
                        self.notice = Some(err.to_string());
                    }
                } else {
                    self.models.pending_merge_duplicates = None;
                }
            }
        }
    }

    /// OS shutdown uses the same close path, without waiting on a dialog the user cannot see.
    pub fn shutdown(&mut self) {
        self.foundation.modal = None;
        self.save_on_close = true;
        self.discard_on_close = false;
        self.foundation.events.push(EditorEvent::Shutdown);
        self.request_close = true;
    }

    pub fn accept_drop(&mut self, path: &Path) -> Result<(), CoreError> {
        if path.is_dir() {
            let text = path.to_str().ok_or_else(|| CoreError::new("dropped path is not text"))?;
            let text = text.to_string();
            return self.dispatch("file.open-project", &CommandArgs::new(&[("path", &text)]));
        }
        if self.project.is_some() && path.is_file() {
            let text = path.to_string_lossy().to_string();
            return self.dispatch("asset.import", &CommandArgs::new(&[("path", &text)]));
        }
        self.foundation.events.push(EditorEvent::DropUnsupported);
        self.notify(self.messages.get("drop-unsupported"), false);
        Ok(())
    }

    pub fn on_chord(&mut self, chord: &Chord) -> Option<String> {
        let ctx = self.when_context();
        let resolved = self.keymap.resolve(
            self.pending_chord.as_ref(),
            chord,
            self.foundation.text_input,
            &ctx,
        );
        self.pending_chord = resolved.pending;
        resolved.command
    }

    /// Every selected entity: the outliner's multi-selection plus the active one.
    pub fn selected_ids(&self) -> Vec<String> {
        let mut ids = self.foundation.selection.clone();
        if let Some(id) = &self.models.selection {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        ids
    }

    /// The entity single-object commands act on: the active one, else the last picked.
    pub fn primary_selection(&self) -> Option<String> {
        self.models.selection.clone().or_else(|| self.foundation.selection.last().cloned())
    }

    pub fn when_context(&self) -> WhenContext {
        let mut ctx = WhenContext::new();
        ctx.set("hasProject", if self.project.is_some() { "true" } else { "false" });
        ctx.set("textInputFocused", if self.foundation.text_input { "true" } else { "false" });
        ctx.set("panelFocus", &self.foundation.focused_panel_id);
        let outliner = self.foundation.focused_panel_id.starts_with("outliner");
        ctx.set("outlinerFocused", if outliner { "true" } else { "false" });
        ctx.set("layoutLocked", if self.foundation.layout_locked { "true" } else { "false" });
        let selected = self.models.selection.is_some() || !self.foundation.selection.is_empty();
        ctx.set("hasSelection", if selected { "true" } else { "false" });
        ctx
    }

    pub fn start_task(&mut self, id: impl Into<String>, title: impl Into<String>) {
        let id = id.into();
        self.foundation.tasks.retain(|task| task.id != id);
        self.foundation.tasks.push(BackgroundTask {
            id,
            title: title.into(),
            progress: 0.0,
            cancelled: false,
        });
    }

    pub fn set_task_progress(&mut self, id: &str, progress: f32) {
        if let Some(task) = self.foundation.tasks.iter_mut().find(|task| task.id == id) {
            task.progress = progress.clamp(0.0, 1.0);
        }
    }

    pub fn cancel_task(&mut self, id: &str) {
        if let Some(task) = self.foundation.tasks.iter_mut().find(|task| task.id == id) {
            task.cancelled = true;
        }
    }

    pub fn status_snapshot(&self) -> StatusSnapshot {
        let project = self.project.as_ref().map(|project| project.name.clone()).unwrap_or_default();
        let tasks = self
            .foundation
            .tasks
            .iter()
            .filter(|task| !task.cancelled && task.progress < 1.0)
            .map(|task| task.title.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let selection = if self.foundation.selection.is_empty() {
            self.models.selection.clone().unwrap_or_default()
        } else {
            self.foundation.selection.join(", ")
        };
        StatusSnapshot {
            project,
            tasks,
            selection,
            hidden: self.foundation.settings.get("ui.show_status_bar") == Some("false"),
            agent: self.foundation.agent_activity.clone(),
        }
    }

    /// How many of the scene, the settings, and the scratch document still have unsaved edits.
    pub fn dirty_count(&self) -> usize {
        let mut count = 0;
        if self.scene_dirty {
            count += 1;
        }
        if self.settings_dirty {
            count += 1;
        }
        if self.documents.iter().any(|doc| doc.id == "scratch" && doc.dirty()) {
            count += 1;
        }
        count
    }

    pub fn bus_describe(&self, id: &str) -> Option<String> {
        let spec = self.commands.get(id)?;
        Some(format!(
            "{{\"id\":\"{}\",\"title\":\"{}\",\"category\":\"{}\",\"agent_safe\":{},\"schema\":{}}}",
            spec.id,
            self.messages.get(&spec.title_key),
            spec.category,
            spec.agent_safe,
            spec.schema_json()
        ))
    }

    pub fn bus_run(&mut self, id: &str, args_json: &str) -> Result<String, CoreError> {
        if self.foundation.settings.get("agent.bus.enabled") != Some("true") {
            return Err(CoreError::new("agent bus is off"));
        }
        let Some(canonical) = self.commands.canonical(id) else {
            return Err(CoreError::new(format!("unknown command '{id}'")));
        };
        let spec = self
            .commands
            .get(&canonical)
            .cloned()
            .ok_or_else(|| CoreError::new(format!("unknown command '{id}'")))?;
        let require_approval =
            self.foundation.settings.get("agent.bus.require_approval") != Some("false");
        if require_approval || !spec.agent_safe {
            return Err(CoreError::new(format!("command '{id}' needs approval")));
        }
        self.foundation.agent_activity = canonical.clone();
        let args = if args_json.trim().is_empty() {
            CommandArgs::default()
        } else {
            CommandArgs::from_json_object(args_json)?
        };
        self.dispatch(&canonical, &args)?;
        Ok(format!("{{\"ok\":true,\"source\":\"agent\",\"id\":\"{canonical}\"}}"))
    }

    pub fn requirements_met(&self, spec: &CommandSpec) -> bool {
        spec.requires.iter().all(|part| match part.as_str() {
            "project" => self.project.is_some(),
            "selection" => self.models.selection.is_some() || !self.foundation.selection.is_empty(),
            _ => true,
        })
    }

    fn modal(
        &self,
        kind: ModalKind,
        title: &str,
        body: &str,
        confirm: &str,
        alternate: &str,
        cancel: &str,
    ) -> Modal {
        Modal {
            kind,
            title: self.messages.get(title),
            body: self.messages.get(body),
            confirm: self.messages.get(confirm),
            alternate: if alternate.is_empty() {
                String::new()
            } else {
                self.messages.get(alternate)
            },
            cancel: self.messages.get(cancel),
        }
    }
}

fn close_failed_only(session: &EditorSession) -> bool {
    session.save_failed
        && !session.scene_dirty
        && !session.settings_dirty
        && !session.documents.iter().any(|doc| doc.dirty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;
    use crate::session::EditorSession;

    #[test]
    fn dirty_count_includes_scene_settings_and_scratch() {
        let mut session = EditorSession::new(Layout::default());
        assert_eq!(session.dirty_count(), 0);
        session.scene_dirty = true;
        session.settings_dirty = true;
        session.documents[0].edit(0, 0, "note");
        assert_eq!(session.dirty_count(), 3);
    }

    #[test]
    fn notice_history_stops_at_the_cap() {
        let mut notices = Notifications::default();
        for index in 0..205 {
            notices.push(format!("n{index}"), false, index);
        }
        assert_eq!(notices.history().len(), 200);
        assert_eq!(notices.history()[0].text, "n5");
        assert_eq!(notices.history().last().unwrap().text, "n204");
    }

    #[test]
    fn closing_with_unsaved_work_saves_and_closes_without_asking() {
        let mut session = EditorSession::new(Layout::default());
        session.begin_close();
        assert!(session.request_close);
        assert!(session.foundation.modal.is_none());

        session.request_close = false;
        session.scene_dirty = true;
        session.begin_close();
        assert!(session.request_close);
        assert!(session.save_on_close);
        assert!(!session.discard_on_close);
        assert!(session.foundation.modal.is_none());
    }

    #[test]
    fn closing_after_a_failed_save_asks_instead_of_closing() {
        let mut session = EditorSession::new(Layout::default());
        session.scene_dirty = true;
        session.save_failed = true;
        session.begin_close();
        assert!(!session.request_close);
        let modal = session.foundation.modal.as_ref().expect("close prompt");
        assert_eq!(modal.kind, ModalKind::Close);
    }

    #[test]
    fn a_failed_save_close_offers_retry_and_save_as_when_nothing_else_is_dirty() {
        let mut session = EditorSession::new(Layout::default());
        session.save_failed = true;
        session.begin_close();
        let modal = session.foundation.modal.as_ref().expect("close prompt");
        assert_eq!(modal.confirm, session.messages.get("modal-retry"));
        assert_eq!(modal.alternate, session.messages.get("command-file-save-as"));
    }

    #[test]
    fn a_second_close_while_the_prompt_is_open_saves_and_exits() {
        let mut session = EditorSession::new(Layout::default());
        session.scene_dirty = true;
        session.save_failed = true;
        session.begin_close();
        assert!(session.foundation.modal.is_some());
        session.begin_close();
        assert!(session.request_close);
        assert!(session.save_on_close);
        assert!(!session.discard_on_close);
        assert!(session.foundation.modal.is_none());
        assert!(session.foundation.events.contains(&EditorEvent::Shutdown));
    }
}
