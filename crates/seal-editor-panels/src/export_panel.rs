//! Export panel: named presets, Export, and Build and Run.

use seal_editor_core::export_preset::{
    book_for_session, to_text, ExportBook, ExportPreset, ExportStep,
};
use seal_editor_core::EditorSession;
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

pub fn export_panel(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let mut event = PanelEvent::None;
    let text = |key: &str| session.messages.get(key);
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&text("panel-export-title"));
            let Some(project) = session.project.clone() else {
                frame.muted(&text("package-no-project"));
                return;
            };
            let mut book = match book_for_session(session) {
                Ok(book) => book,
                Err(err) => {
                    frame.muted(&err);
                    book_draft(session).unwrap_or_default()
                }
            };
            if book.presets.is_empty() {
                let folder =
                    seal_editor_core::package::default_package_folder(&project.root, &project.name);
                let preset = ExportPreset::named(&project.name, &folder.display().to_string());
                book.selected = preset.name.clone();
                book.presets.push(preset);
            }
            if book.selected.is_empty() {
                book.selected = book.presets[0].name.clone();
            }
            let names: Vec<(String, String)> = book
                .presets
                .iter()
                .map(|preset| (preset.name.clone(), preset.name.clone()))
                .collect();
            let options: Vec<(&str, String)> =
                names.iter().map(|(id, title)| (id.as_str(), title.clone())).collect();
            if let Some(picked) = frame.choice(&text("export-preset"), &book.selected, &options) {
                book.selected = picked;
                event = save(&book);
            }
            frame.row(|frame| {
                if frame.button(&text("export-new")) {
                    let name = fresh_name(&book);
                    book.presets.push(ExportPreset::named(&name, ""));
                    book.selected = name;
                    event = save(&book);
                }
                if book.presets.len() > 1 && frame.button(&text("export-delete")) {
                    book.presets.retain(|preset| preset.name != book.selected);
                    book.selected =
                        book.presets.first().map(|preset| preset.name.clone()).unwrap_or_default();
                    event = save(&book);
                }
            });
            let Some(index) = book.presets.iter().position(|preset| preset.name == book.selected)
            else {
                return;
            };
            let mut changed = false;
            let mut name = book.presets[index].name.clone();
            if frame.property(&text("export-name"), &mut name) {
                let name = name.trim().to_string();
                let taken = book
                    .presets
                    .iter()
                    .enumerate()
                    .any(|(i, other)| i != index && other.name == name);
                if !name.is_empty() && !taken {
                    book.presets[index].name = name.clone();
                    book.selected = name;
                    changed = true;
                }
            }
            {
                let preset = &mut book.presets[index];
                if frame.property(&text("export-folder"), &mut preset.folder) {
                    changed = true;
                }
                let profiles = [
                    ("release", text("export-profile-release")),
                    ("debug", text("export-profile-debug")),
                ];
                if let Some(profile) =
                    frame.choice(&text("export-profile"), &preset.profile, &profiles)
                {
                    preset.profile = profile;
                    changed = true;
                }
                let platforms: Vec<(&str, String)> =
                    seal_editor_core::import_platform::ALL_PLATFORMS
                        .iter()
                        .map(|p| {
                            (
                                p.id(),
                                session.messages.format("import-platform-name", &[("id", p.id())]),
                            )
                        })
                        .collect();
                if let Some(platform) =
                    frame.choice(&text("export-platform"), &preset.platform, &platforms)
                {
                    preset.platform = platform;
                    changed = true;
                }
                frame.muted(&text("export-platform-hint"));
                let mut include = preset.include.join("; ");
                if frame.property(&text("export-include"), &mut include) {
                    preset.include = split_list(&include);
                    changed = true;
                }
                frame.muted(&text("export-include-hint"));
                let mut exclude = preset.exclude.join("; ");
                if frame.property(&text("export-exclude"), &mut exclude) {
                    preset.exclude = split_list(&exclude);
                    changed = true;
                }
                let mut extra = preset
                    .extra
                    .iter()
                    .map(|(from, to)| format!("{from} => {to}"))
                    .collect::<Vec<_>>()
                    .join("; ");
                if frame.property(&text("export-extra"), &mut extra) {
                    preset.extra = split_pairs(&extra);
                    changed = true;
                }
                frame.muted(&text("export-extra-hint"));
                if frame.toggle(&text("export-clean"), preset.clean) {
                    preset.clean = !preset.clean;
                    changed = true;
                }
                if frame.toggle(&text("export-atlas-tight"), preset.atlas_tight) {
                    preset.atlas_tight = !preset.atlas_tight;
                    changed = true;
                }
                frame.muted(&text("export-atlas-tight-hint"));
                if frame.toggle(&text("export-atlas-rotate"), preset.atlas_rotate) {
                    preset.atlas_rotate = !preset.atlas_rotate;
                    changed = true;
                }
                frame.muted(&text("export-atlas-rotate-hint"));
                let pages = [
                    ("1024", text("export-atlas-page-1024")),
                    ("2048", text("export-atlas-page-2048")),
                    ("4096", text("export-atlas-page-4096")),
                    ("8192", text("export-atlas-page-8192")),
                ];
                let page = preset.atlas_page.to_string();
                if let Some(picked) = frame.choice(&text("export-atlas-page"), &page, &pages) {
                    if let Ok(size) = picked.parse::<u32>() {
                        preset.atlas_page = size;
                        changed = true;
                    }
                }
                frame.muted(&text("export-atlas-page-hint"));
                let mut scale = preset.atlas_scale as f32;
                if frame.number(&text("export-atlas-scale"), &mut scale, 1.0) {
                    let scale = scale.round().clamp(1.0, 100.0) as u32;
                    if scale != preset.atlas_scale {
                        preset.atlas_scale = scale;
                        changed = true;
                    }
                }
                frame.muted(&text("export-atlas-scale-hint"));
                let mut archive_pick = None;
                let archive = if preset.single_archive {
                    seal_ui::PropertyCheckbox::On
                } else {
                    seal_ui::PropertyCheckbox::Off
                };
                if let Some(next) = frame.property_checkbox_row(
                    &text("export-single-archive"),
                    archive,
                    false,
                    None,
                    None,
                    &mut archive_pick,
                    None,
                ) {
                    preset.single_archive = next == seal_ui::PropertyCheckbox::On;
                    changed = true;
                }
                frame.muted(&text("export-single-archive-hint"));
                let targets = [
                    ("folder", text("export-package-folder")),
                    ("msix", text("export-package-msix")),
                ];
                if let Some(target) =
                    frame.choice(&text("export-package-target"), &preset.package_target, &targets)
                {
                    preset.package_target = target;
                    changed = true;
                }
                if preset.package_target == "msix" {
                    let mut publisher = preset.msix_publisher.clone();
                    if frame.property(&text("export-msix-publisher"), &mut publisher) {
                        preset.msix_publisher = publisher;
                        changed = true;
                    }
                    let mut version = preset.msix_version.clone();
                    if frame.property(&text("export-msix-version"), &mut version) {
                        preset.msix_version = version;
                        changed = true;
                    }
                    if !seal_editor_core::export_preset::valid_msix_version(&preset.msix_version) {
                        frame.muted(&text("export-msix-version-hint"));
                    }
                    use seal_editor_core::msix_pack::{self, ToolState};
                    match (msix_pack::which_makeappx(), msix_pack::tool_state()) {
                        (_, ToolState::Downloading) => {
                            frame.muted(&text("export-makeappx-downloading"));
                            frame
                                .ui_mut()
                                .ctx()
                                .request_repaint_after(std::time::Duration::from_millis(250));
                        }
                        (Ok(path), _) => {
                            frame.muted(&format!("{} {path}", text("export-makeappx-found")))
                        }
                        (Err(_), state) => {
                            frame.muted(&text("export-makeappx-missing"));
                            if let ToolState::Failed(why) = state {
                                frame.muted(&why);
                            }
                            if frame.button(&text("export-makeappx-download")) {
                                msix_pack::start_makeappx_download();
                            }
                        }
                    }
                }
                let mut pre = steps_text(&preset.pre);
                if frame.property(&text("export-pre"), &mut pre) {
                    preset.pre = split_steps(&pre);
                    changed = true;
                }
                let mut post = steps_text(&preset.post);
                if frame.property(&text("export-post"), &mut post) {
                    preset.post = split_steps(&post);
                    changed = true;
                }
                frame.muted(&text("export-steps-hint"));
            }
            if changed {
                event = save(&book);
            }
            frame.row(|frame| {
                if frame.button(&text("command-project-export-go")) {
                    event = command("project.export_go", &[("preset", book.selected.as_str())]);
                }
                if frame.button(&text("command-project-export-run")) {
                    event = command("project.export_run", &[("preset", book.selected.as_str())]);
                }
            });
        });
    });
    event
}

fn save(book: &ExportBook) -> PanelEvent {
    let text = to_text(book);
    command("export.write", &[("text", text.as_str())])
}

fn book_draft(session: &EditorSession) -> Option<ExportBook> {
    session
        .models
        .panel_states
        .get("export.book")
        .and_then(|text| seal_editor_core::export_preset::parse_book(text).ok())
}

fn fresh_name(book: &ExportBook) -> String {
    for index in 2..100 {
        let name = format!("Preset {index}");
        if book.presets.iter().all(|preset| preset.name != name) {
            return name;
        }
    }
    "Preset".into()
}

fn split_list(text: &str) -> Vec<String> {
    text.split(';').map(|part| part.trim().to_string()).filter(|part| !part.is_empty()).collect()
}

fn split_pairs(text: &str) -> Vec<(String, String)> {
    split_list(text)
        .into_iter()
        .filter_map(|part| {
            let (from, to) = part.split_once("=>")?;
            let from = from.trim().to_string();
            let to = to.trim().to_string();
            (!from.is_empty() && !to.is_empty()).then_some((from, to))
        })
        .collect()
}

fn steps_text(steps: &[ExportStep]) -> String {
    steps
        .iter()
        .map(|step| match step {
            ExportStep::Copy { from, to } => format!("copy {from} => {to}"),
            ExportStep::Zip { folder } => {
                if folder.is_empty() {
                    "zip".into()
                } else {
                    format!("zip {folder}")
                }
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn split_steps(text: &str) -> Vec<ExportStep> {
    split_list(text)
        .into_iter()
        .filter_map(|part| {
            if let Some(rest) = part.strip_prefix("copy ") {
                let (from, to) = rest.split_once("=>")?;
                return Some(ExportStep::Copy {
                    from: from.trim().to_string(),
                    to: to.trim().to_string(),
                });
            }
            if part == "zip" {
                return Some(ExportStep::Zip { folder: String::new() });
            }
            if let Some(folder) = part.strip_prefix("zip ") {
                return Some(ExportStep::Zip { folder: folder.trim().to_string() });
            }
            None
        })
        .collect()
}
