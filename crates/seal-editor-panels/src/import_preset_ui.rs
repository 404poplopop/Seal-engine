//! Import preset controls in the Asset Editor and manage dialog.

use seal_editor_core::import_presets::{
    browser_preset_labels, matching_preset_label, ImportKind, ImportSettings, PictureSettings,
    PICTURE_MAX_SIZES,
};
use seal_editor_core::EditorSession;
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

pub fn import_settings_block(
    frame: &mut PanelFrame,
    session: &mut EditorSession,
    rel: &str,
) -> PanelEvent {
    let Some(kind) = ImportKind::from_asset_path(rel) else {
        return PanelEvent::None;
    };
    if kind == ImportKind::Font {
        return PanelEvent::None;
    }
    let title = session.messages.get("import-settings-title");
    let root = match session.project.as_ref() {
        Some(p) => p.root.clone(),
        None => return PanelEvent::None,
    };
    let state = seal_editor_core::import_presets::load_file_state(&root, rel, kind);
    let label = matching_preset_label(&session.models.import_presets, &state.settings);
    let current = session.messages.format("import-preset-current", &[("name", label.as_str())]);
    let apply_l = session.messages.get("import-preset-apply");
    let save_l = session.messages.get("import-preset-save-as");
    let manage_l = session.messages.get("import-preset-manage");
    let presets: Vec<(&str, String)> = session
        .models
        .import_presets
        .presets_for_kind(kind)
        .into_iter()
        .map(|p| (p.name.as_str(), p.name.clone()))
        .collect();
    let field_labels = field_labels(session, &state.settings);
    let save_draft = session
        .draft("import-preset-new-name")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "My Preset".into());
    let mut event = PanelEvent::None;
    let mut open_manage = false;
    frame.section(&title, |f| {
        f.muted(&current);
        if let Some(picked) = f.choice(&apply_l, &label, &presets) {
            if picked != "Custom" && picked != label {
                event =
                    command("import.preset.apply", &[("preset", picked.as_str()), ("paths", rel)]);
            }
        }
        let fields = field_edits(f, rel, &state.settings, &field_labels);
        if !matches!(fields, PanelEvent::None) {
            event = fields;
        }
        if f.button(&save_l) {
            event = command("import.preset.save", &[("name", save_draft.as_str()), ("path", rel)]);
        }
        if f.button(&manage_l) {
            open_manage = true;
        }
    });
    if let ImportSettings::Picture(p) = &state.settings {
        let platform = crate::import_platform_ui::platform_overrides_block(frame, session, rel, p);
        if !matches!(platform, PanelEvent::None) {
            event = platform;
        }
    }
    if open_manage {
        let _ = session.set_draft("import-preset-manage", "1");
    }
    if session.draft("import-preset-manage").as_deref() == Some("1") {
        let manage = manage_dialog(frame, session);
        if !matches!(manage, PanelEvent::None) {
            event = manage;
        }
    }
    event
}

struct FieldLabels {
    filter: String,
    nearest: String,
    linear: String,
    ppu: String,
    ppu_hint: String,
    normalize: String,
    max_size: String,
    max_original: String,
    mipmaps: String,
    colour: String,
    colour_srgb: String,
    colour_linear: String,
    wrap: String,
    wrap_clamp: String,
    wrap_repeat: String,
    wrap_mirror: String,
    sprite_mode: String,
    sprite_single: String,
    sprite_multiple: String,
    sprite_hint: String,
    pivot: String,
    pivot_center: String,
    pivot_top_left: String,
    pivot_top: String,
    pivot_top_right: String,
    pivot_left: String,
    pivot_right: String,
    pivot_bottom_left: String,
    pivot_bottom: String,
    pivot_bottom_right: String,
    pivot_custom: String,
    pivot_x: String,
    pivot_y: String,
    svg_size: String,
    svg_hint: String,
    compression: String,
    compression_none: String,
    compression_lossless: String,
    compression_low: String,
    compression_medium: String,
    compression_high: String,
    compression_hint: String,
    mesh: String,
    mesh_full: String,
    mesh_tight: String,
    mesh_hint: String,
    compression_pcm: String,
    compression_adpcm: String,
    compression_vorbis: String,
    vorbis_quality: String,
    load_type: String,
    load_memory: String,
    load_stream: String,
}

fn field_labels(session: &EditorSession, settings: &ImportSettings) -> FieldLabels {
    let _ = settings;
    FieldLabels {
        filter: session.messages.get("import-field-filter"),
        nearest: session.messages.get("import-filter-nearest"),
        linear: session.messages.get("import-filter-linear"),
        ppu: session.messages.get("import-field-ppu"),
        ppu_hint: session.messages.get("import-field-ppu-hint"),
        normalize: session.messages.get("import-field-normalize"),
        max_size: session.messages.get("import-field-max-size"),
        max_original: session.messages.get("import-max-original"),
        mipmaps: session.messages.get("import-field-mipmaps"),
        colour: session.messages.get("import-field-colour-space"),
        colour_srgb: session.messages.get("import-colour-srgb"),
        colour_linear: session.messages.get("import-colour-linear"),
        wrap: session.messages.get("import-field-wrap"),
        wrap_clamp: session.messages.get("import-wrap-clamp"),
        wrap_repeat: session.messages.get("import-wrap-repeat"),
        wrap_mirror: session.messages.get("import-wrap-mirror"),
        sprite_mode: session.messages.get("import-field-sprite-mode"),
        sprite_single: session.messages.get("import-sprite-single"),
        sprite_multiple: session.messages.get("import-sprite-multiple"),
        sprite_hint: session.messages.get("import-sprite-multiple-hint"),
        pivot: session.messages.get("import-field-pivot"),
        pivot_center: session.messages.get("import-pivot-center"),
        pivot_top_left: session.messages.get("import-pivot-top-left"),
        pivot_top: session.messages.get("import-pivot-top"),
        pivot_top_right: session.messages.get("import-pivot-top-right"),
        pivot_left: session.messages.get("import-pivot-left"),
        pivot_right: session.messages.get("import-pivot-right"),
        pivot_bottom_left: session.messages.get("import-pivot-bottom-left"),
        pivot_bottom: session.messages.get("import-pivot-bottom"),
        pivot_bottom_right: session.messages.get("import-pivot-bottom-right"),
        pivot_custom: session.messages.get("import-pivot-custom"),
        pivot_x: session.messages.get("import-field-pivot-x"),
        pivot_y: session.messages.get("import-field-pivot-y"),
        svg_size: session.messages.get("import-field-svg-size"),
        svg_hint: session.messages.get("import-svg-size-hint"),
        compression: session.messages.get("import-field-compression"),
        compression_none: session.messages.get("import-compression-none"),
        compression_lossless: session.messages.get("import-compression-lossless"),
        compression_low: session.messages.get("import-compression-low"),
        compression_medium: session.messages.get("import-compression-medium"),
        compression_high: session.messages.get("import-compression-high"),
        compression_hint: session.messages.get("import-compression-hint"),
        mesh: session.messages.get("import-field-mesh"),
        mesh_full: session.messages.get("import-mesh-full"),
        mesh_tight: session.messages.get("import-mesh-tight"),
        mesh_hint: session.messages.get("import-mesh-hint"),
        compression_pcm: session.messages.get("import-field-compression-pcm"),
        compression_adpcm: session.messages.get("import-field-compression-adpcm"),
        compression_vorbis: session.messages.get("import-field-compression-vorbis"),
        vorbis_quality: session.messages.get("import-field-vorbis-quality"),
        load_type: session.messages.get("import-field-load-type"),
        load_memory: session.messages.get("import-field-load-memory"),
        load_stream: session.messages.get("import-field-load-stream"),
    }
}

fn field_edits(
    frame: &mut PanelFrame,
    rel: &str,
    settings: &ImportSettings,
    labels: &FieldLabels,
) -> PanelEvent {
    match settings {
        ImportSettings::Picture(p) => {
            let filter_pairs =
                [("nearest", labels.nearest.clone()), ("linear", labels.linear.clone())];
            let pairs: Vec<(&str, String)> =
                filter_pairs.iter().map(|(id, t)| (*id, t.clone())).collect();
            if let Some(picked) = frame.choice(&labels.filter, &p.filter, &pairs) {
                if picked != p.filter {
                    return command(
                        "import.preset.field",
                        &[("path", rel), ("field", "filter"), ("value", picked.as_str())],
                    );
                }
            }
            let mut ppu = p.pixels_per_unit.to_string();
            if frame.property(&labels.ppu, &mut ppu)
                && ppu.parse::<f32>().ok() != Some(p.pixels_per_unit)
            {
                return command(
                    "import.preset.field",
                    &[("path", rel), ("field", "pixels_per_unit"), ("value", ppu.as_str())],
                );
            }
            frame.muted(&labels.ppu_hint);
            if let Some(event) = picture_import_fields(frame, rel, p, labels) {
                return event;
            }
        }
        ImportSettings::Sound(s) => {
            if frame.toggle(&labels.normalize, s.normalize) {
                return command(
                    "import.preset.field",
                    &[
                        ("path", rel),
                        ("field", "normalize"),
                        ("value", if s.normalize { "false" } else { "true" }),
                    ],
                );
            }
            let compression = [
                ("pcm", labels.compression_pcm.clone()),
                ("adpcm", labels.compression_adpcm.clone()),
                ("vorbis", labels.compression_vorbis.clone()),
            ];
            let pairs: Vec<(&str, String)> =
                compression.iter().map(|(id, t)| (*id, t.clone())).collect();
            if let Some(picked) = frame.choice(&labels.compression, &s.compression, &pairs) {
                if picked != s.compression {
                    return command(
                        "import.preset.field",
                        &[("path", rel), ("field", "compression"), ("value", picked.as_str())],
                    );
                }
            }
            let mut quality = s.vorbis_quality.to_string();
            if s.compression == "vorbis"
                && frame.property(&labels.vorbis_quality, &mut quality)
                && quality.parse::<u8>().ok() != Some(s.vorbis_quality)
            {
                return command(
                    "import.preset.field",
                    &[("path", rel), ("field", "vorbis_quality"), ("value", quality.as_str())],
                );
            }
            let load =
                [("memory", labels.load_memory.clone()), ("stream", labels.load_stream.clone())];
            let load_pairs: Vec<(&str, String)> =
                load.iter().map(|(id, t)| (*id, t.clone())).collect();
            if let Some(picked) = frame.choice(&labels.load_type, &s.load_type, &load_pairs) {
                if picked != s.load_type {
                    return command(
                        "import.preset.field",
                        &[("path", rel), ("field", "load_type"), ("value", picked.as_str())],
                    );
                }
            }
        }
        ImportSettings::Font(_) => {}
    }
    PanelEvent::None
}

fn picture_import_fields(
    frame: &mut PanelFrame,
    rel: &str,
    picture: &PictureSettings,
    labels: &FieldLabels,
) -> Option<PanelEvent> {
    let send = |field: &str, value: &str| {
        command("import.preset.field", &[("path", rel), ("field", field), ("value", value)])
    };
    let mut sizes = vec![("0".to_string(), labels.max_original.clone())];
    for size in PICTURE_MAX_SIZES {
        let text = size.to_string();
        sizes.push((text.clone(), text));
    }
    let size_pairs: Vec<(&str, String)> =
        sizes.iter().map(|(id, title)| (id.as_str(), title.clone())).collect();
    let current_size = picture.max_size.to_string();
    if let Some(picked) = frame.choice(&labels.max_size, &current_size, &size_pairs) {
        if picked != current_size {
            return Some(send("max_size", &picked));
        }
    }
    if frame.toggle(&labels.mipmaps, picture.mipmaps) {
        return Some(send("mipmaps", if picture.mipmaps { "false" } else { "true" }));
    }
    let colour_pairs =
        [("srgb", labels.colour_srgb.clone()), ("linear", labels.colour_linear.clone())];
    let colour: Vec<(&str, String)> =
        colour_pairs.iter().map(|(id, title)| (*id, title.clone())).collect();
    if let Some(picked) = frame.choice(&labels.colour, &picture.colour_space, &colour) {
        if picked != picture.colour_space {
            return Some(send("colour_space", &picked));
        }
    }
    let wrap_pairs = [
        ("clamp", labels.wrap_clamp.clone()),
        ("repeat", labels.wrap_repeat.clone()),
        ("mirror", labels.wrap_mirror.clone()),
    ];
    let wrap: Vec<(&str, String)> =
        wrap_pairs.iter().map(|(id, title)| (*id, title.clone())).collect();
    if let Some(picked) = frame.choice(&labels.wrap, &picture.wrap, &wrap) {
        if picked != picture.wrap {
            return Some(send("wrap", &picked));
        }
    }
    let mode_pairs =
        [("single", labels.sprite_single.clone()), ("multiple", labels.sprite_multiple.clone())];
    let modes: Vec<(&str, String)> =
        mode_pairs.iter().map(|(id, title)| (*id, title.clone())).collect();
    if let Some(picked) = frame.choice(&labels.sprite_mode, &picture.sprite_mode, &modes) {
        if picked != picture.sprite_mode {
            return Some(send("sprite_mode", &picked));
        }
    }
    frame.muted(&labels.sprite_hint);
    let pivot_pairs = [
        ("center", labels.pivot_center.clone()),
        ("top_left", labels.pivot_top_left.clone()),
        ("top", labels.pivot_top.clone()),
        ("top_right", labels.pivot_top_right.clone()),
        ("left", labels.pivot_left.clone()),
        ("right", labels.pivot_right.clone()),
        ("bottom_left", labels.pivot_bottom_left.clone()),
        ("bottom", labels.pivot_bottom.clone()),
        ("bottom_right", labels.pivot_bottom_right.clone()),
        ("custom", labels.pivot_custom.clone()),
    ];
    let pivots: Vec<(&str, String)> =
        pivot_pairs.iter().map(|(id, title)| (*id, title.clone())).collect();
    if let Some(picked) = frame.choice(&labels.pivot, &picture.pivot, &pivots) {
        if picked != picture.pivot {
            return Some(send("pivot", &picked));
        }
    }
    if picture.pivot == "custom" {
        let mut x = picture.pivot_x.to_string();
        if frame.property(&labels.pivot_x, &mut x) && x.parse::<f32>().ok() != Some(picture.pivot_x)
        {
            return Some(send("pivot_x", &x));
        }
        let mut y = picture.pivot_y.to_string();
        if frame.property(&labels.pivot_y, &mut y) && y.parse::<f32>().ok() != Some(picture.pivot_y)
        {
            return Some(send("pivot_y", &y));
        }
    }
    let compression_pairs = [
        ("none", labels.compression_none.clone()),
        ("lossless", labels.compression_lossless.clone()),
        ("low", labels.compression_low.clone()),
        ("medium", labels.compression_medium.clone()),
        ("high", labels.compression_high.clone()),
    ];
    let compression: Vec<(&str, String)> =
        compression_pairs.iter().map(|(id, title)| (*id, title.clone())).collect();
    if let Some(picked) = frame.choice(&labels.compression, &picture.compression, &compression) {
        if picked != picture.compression {
            return Some(send("compression", &picked));
        }
    }
    frame.muted(&labels.compression_hint);
    let mesh_pairs = [("full", labels.mesh_full.clone()), ("tight", labels.mesh_tight.clone())];
    let meshes: Vec<(&str, String)> =
        mesh_pairs.iter().map(|(id, title)| (*id, title.clone())).collect();
    if let Some(picked) = frame.choice(&labels.mesh, &picture.mesh, &meshes) {
        if picked != picture.mesh {
            return Some(send("mesh", &picked));
        }
    }
    frame.muted(&labels.mesh_hint);
    if rel.rsplit('.').next().unwrap_or("").eq_ignore_ascii_case("svg") {
        let mut sizes = Vec::new();
        for size in PICTURE_MAX_SIZES {
            let text = size.to_string();
            sizes.push((text.clone(), text));
        }
        let size_pairs: Vec<(&str, String)> =
            sizes.iter().map(|(id, title)| (id.as_str(), title.clone())).collect();
        let current = picture.svg_size.to_string();
        if let Some(picked) = frame.choice(&labels.svg_size, &current, &size_pairs) {
            if picked != current {
                return Some(send("svg_size", &picked));
            }
        }
        frame.muted(&labels.svg_hint);
    }
    None
}

fn manage_dialog(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let title = session.messages.get("import-preset-manage-title");
    let hint = session.messages.get("import-preset-manage-hint");
    let delete = session.messages.get("import-preset-delete");
    let close = session.messages.get("import-preset-manage-close");
    let user_presets: Vec<String> = session
        .models
        .import_presets
        .presets
        .iter()
        .filter(|p| !p.builtin)
        .map(|p| p.name.clone())
        .collect();
    let mut defaults_ui = Vec::new();
    for kind in [ImportKind::Picture, ImportKind::Sound] {
        let current =
            session.models.import_presets.defaults.get(kind.id()).cloned().unwrap_or_default();
        let options: Vec<(&str, String)> = session
            .models
            .import_presets
            .presets_for_kind(kind)
            .into_iter()
            .map(|p| (p.name.as_str(), p.name.clone()))
            .collect();
        let row_title = session.messages.format("import-default-kind", &[("kind", kind.id())]);
        defaults_ui.push((kind, current, options, row_title));
    }
    let mut event = PanelEvent::None;
    let mut close_manage = false;
    frame.section(&title, |f| {
        f.muted(&hint);
        for name in &user_presets {
            f.row(|row| {
                row.label(name);
                if row.button(&delete) {
                    event = command("import.preset.delete", &[("name", name.as_str())]);
                }
            });
        }
        for (kind, current, options, row_title) in defaults_ui {
            if let Some(picked) = f.choice(&row_title, &current, &options) {
                if picked != current {
                    event = command(
                        "import.preset.default",
                        &[("kind", kind.id()), ("preset", picked.as_str())],
                    );
                }
            }
        }
        if f.button(&close) {
            close_manage = true;
        }
    });
    if close_manage {
        let _ = session.set_draft("import-preset-manage", "");
    }
    event
}

pub fn browser_preset_menu(session: &EditorSession, rel: &str) -> Vec<(String, PanelEvent)> {
    let mut out = Vec::new();
    for name in browser_preset_labels(session, rel) {
        out.push((
            session.messages.format("content-menu-apply-import-preset-named", &[("name", &name)]),
            command("import.preset.apply", &[("preset", name.as_str()), ("paths", rel)]),
        ));
    }
    if !out.is_empty() {
        out.insert(0, (session.messages.get("content-menu-apply-import-preset"), PanelEvent::None));
    }
    out
}

pub fn settings_project_section(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let title = session.messages.get("import-settings-project-title");
    let hint = session.messages.get("import-settings-project-hint");
    let path_l = session.messages.get("import-folder-rule-path");
    let preset_l = session.messages.get("import-folder-rule-preset");
    let add = session.messages.get("import-folder-rule-add");
    let rules: Vec<String> = session
        .models
        .import_presets
        .folder_rules
        .iter()
        .map(|rule| format!("{} / {} → {}", rule.path, rule.kind.id(), rule.preset))
        .collect();
    let mut folder_edit = session.draft("import-folder-rule-path").unwrap_or_default();
    let mut preset_edit = session.draft("import-folder-rule-preset").unwrap_or_default();
    let mut event = PanelEvent::None;
    frame.title(&title);
    frame.muted(&hint);
    if frame.property(&path_l, &mut folder_edit) {
        let _ = session.set_draft("import-folder-rule-path", &folder_edit);
    }
    if frame.property(&preset_l, &mut preset_edit) {
        let _ = session.set_draft("import-folder-rule-preset", &preset_edit);
    }
    if frame.button(&add) && !folder_edit.is_empty() && !preset_edit.is_empty() {
        event = command(
            "import.preset.folder_rule",
            &[
                ("path", folder_edit.as_str()),
                ("kind", "picture"),
                ("preset", preset_edit.as_str()),
            ],
        );
    }
    for line in rules {
        frame.muted(&line);
    }
    let atlas = crate::import_platform_ui::atlas_platform_section(frame, session);
    if !matches!(atlas, PanelEvent::None) {
        event = atlas;
    }
    event
}
