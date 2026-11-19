//! Asset Editor for `.texture` composite texture files.

use std::path::Path;

use egui::Sense;
use seal_editor_core::texture_asset_cmd::{
    TEXTURE_ENCODE_BYTES, TEXTURE_ENCODE_PSNR, TEXTURE_SLICE_DRAFT,
};
use seal_editor_core::{CommandArgs, EditorSession};
use seal_editor_core::{GpuBlockFormat, TextureAsset, TextureAssetKind};
use seal_ui::{PanelFrame, PropertyCheckbox};

use crate::{command, PanelEvent};

const GPU_FORMATS: &[GpuBlockFormat] = &[
    GpuBlockFormat::Rgba8,
    GpuBlockFormat::Bc1,
    GpuBlockFormat::Bc2,
    GpuBlockFormat::Bc3,
    GpuBlockFormat::Bc4,
    GpuBlockFormat::Bc5,
    GpuBlockFormat::Bc6h,
    GpuBlockFormat::Bc7,
    GpuBlockFormat::Etc2,
    GpuBlockFormat::Astc4x4,
];

fn draft_cmd(key: &str, value: &str) -> (String, CommandArgs) {
    ("draft.set".into(), CommandArgs::new(&[("key", key), ("value", value)]))
}

fn load_asset(full: &Path) -> Option<TextureAsset> {
    let text = std::fs::read_to_string(full).ok()?;
    TextureAsset::parse(&text).ok()
}

pub(crate) fn texture_composite_editor(
    frame: &mut PanelFrame,
    session: &mut EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    let mut menu_pick = None;
    let Some(asset) = load_asset(full) else {
        frame.muted(&text("asset-texture-unreadable"));
        return event;
    };
    let slice: usize = session.draft(TEXTURE_SLICE_DRAFT).and_then(|s| s.parse().ok()).unwrap_or(0);
    let layer_count = asset.layer_count().max(1) as usize;
    let slice = slice.min(layer_count - 1);

    frame.bar(|frame| {
        frame.muted(&text(match asset.kind {
            TextureAssetKind::Array2d => "asset-texture-kind-array",
            TextureAssetKind::Volume3d => "asset-texture-kind-volume",
            TextureAssetKind::Cubemap => "asset-texture-kind-cubemap",
        }));
    });
    if asset.validate_pictures().is_err() {
        frame.muted(&text("asset-texture-needs-pictures"));
    }

    let current_format = asset.gpu_format.name();
    let format_opts: Vec<(&str, String)> =
        GPU_FORMATS.iter().map(|f| (f.name(), f.name().to_string())).collect();
    let (picked, _) = frame.choice_row(
        &text("asset-texture-gpu-format"),
        current_format,
        &format_opts,
        asset.gpu_format != GpuBlockFormat::Rgba8,
        None,
        None,
        None,
        &mut menu_pick,
        None,
    );
    if let Some(name) = picked {
        event = command("asset.texture_set", &[("gpu_format", name.as_str())]);
    }

    let stream = if asset.streaming { PropertyCheckbox::On } else { PropertyCheckbox::Off };
    if let Some(next) = frame.property_checkbox_row(
        &text("asset-texture-streaming"),
        stream,
        false,
        None,
        None,
        &mut menu_pick,
        None,
    ) {
        let on = matches!(next, PropertyCheckbox::On);
        event = command("asset.texture_set", &[("streaming", if on { "yes" } else { "no" })]);
    }

    let mut budget = asset.memory_budget_mb.to_string();
    let (changed, _, commit) = frame.property_validated_row(
        &text("asset-texture-memory-budget"),
        &mut budget,
        None,
        false,
        None,
        None,
        None,
        &mut menu_pick,
        None,
    );
    if (changed || commit) && budget.parse::<u32>().is_ok() {
        event = command("asset.texture_set", &[("memory_budget_mb", budget.trim())]);
    }

    if layer_count > 1 {
        let slice_label = match asset.kind {
            TextureAssetKind::Cubemap => text("asset-texture-face"),
            TextureAssetKind::Volume3d => text("asset-texture-slice"),
            TextureAssetKind::Array2d => text("asset-texture-layer"),
        };
        let ui = frame.ui_mut();
        ui.horizontal(|ui| {
            seal_ui::property_name(ui, &slice_label, false, None);
            let mut slice_i = slice as i32;
            if ui.add(egui::Slider::new(&mut slice_i, 0..=layer_count as i32 - 1)).changed() {
                event = PanelEvent::Commands(vec![draft_cmd(
                    TEXTURE_SLICE_DRAFT,
                    &slice_i.to_string(),
                )]);
            }
        });
    }

    // One row per cubemap face; arrays and volumes get one empty row to add the next picture.
    let rows = match asset.kind {
        TextureAssetKind::Cubemap => asset.pictures.len().max(6),
        _ => asset.pictures.len() + 1,
    };
    let empty = String::new();
    for i in 0..rows {
        let pic = asset.pictures.get(i).unwrap_or(&empty);
        let label = session
            .messages
            .format("asset-texture-picture-row", &[("index", &(i + 1).to_string())]);
        let mut path = pic.clone();
        let (changed, _, commit) = frame.property_validated_row(
            &label,
            &mut path,
            None,
            !pic.is_empty(),
            None,
            None,
            None,
            &mut menu_pick,
            None,
        );
        if (changed || commit) && path != *pic && !path.trim().is_empty() {
            event = command(
                "asset.texture_set",
                &[("layer", &i.to_string()), ("picture", path.trim())],
            );
        }
    }

    let quality_opts = [("fast", "Fast"), ("balanced", "Balanced"), ("maximum", "Maximum")];
    let mut encode_quality = String::from("balanced");
    let (picked_q, _) = frame.choice_row(
        &text("asset-texture-encode-quality"),
        encode_quality.as_str(),
        &quality_opts.iter().map(|(id, label)| (*id, label.to_string())).collect::<Vec<_>>(),
        false,
        None,
        None,
        None,
        &mut menu_pick,
        None,
    );
    if let Some(q) = picked_q {
        encode_quality = q;
    }
    if frame.button(&text("asset-texture-export-encode")) {
        event = command(
            "asset.texture_export_encode",
            &[("layer", &slice.to_string()), ("quality", encode_quality.as_str())],
        );
    }
    if let Some(bytes) = session.draft(TEXTURE_ENCODE_BYTES) {
        let psnr = session.draft(TEXTURE_ENCODE_PSNR).unwrap_or_default();
        frame.muted(
            &session
                .messages
                .format("asset-texture-encode-stats", &[("bytes", &bytes), ("psnr", &psnr)]),
        );
    }

    let root = session.project.as_ref().map(|p| p.root.clone());
    if let Some(path) = asset
        .pictures
        .get(slice)
        .filter(|p| !p.is_empty())
        .and_then(|pic| root.as_ref().map(|r| r.join(pic)))
        .filter(|p| p.is_file())
    {
        let ctx = frame.ui_mut().ctx().clone();
        let path_s = path.to_string_lossy().to_string();
        seal_ui::sprites::set_texture_picture_policy(
            &path_s,
            asset.streaming,
            asset.memory_budget_mb,
        );
        let decoded = seal_editor_core::picture::decode_path(&path).ok();
        if seal_ui::gpu_viewport::enabled() {
            if let Some(pic) = decoded {
                let ui = frame.ui_mut();
                let size = egui::Vec2::new(pic.width as f32, pic.height as f32)
                    .min(egui::Vec2::splat(256.0));
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                ui.painter().add(seal_ui::gpu_textures::preview_callback(
                    rect,
                    std::sync::Arc::new(pic.rgba),
                    pic.width.max(1),
                    pic.height.max(1),
                    asset.gpu_format,
                ));
                frame.muted(&text("asset-texture-preview-gpu"));
            }
        } else if let Some(texture) = seal_ui::sprites::texture(&ctx, &path_s) {
            frame.muted(&text("asset-texture-preview-cpu"));
            let ui = frame.ui_mut();
            let sz = seal_ui::sprites::texture_size(&ctx, &path_s)
                .unwrap_or([asset.width as f32, asset.height as f32]);
            let size = egui::Vec2::new(sz[0], sz[1]).min(egui::Vec2::splat(256.0));
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            seal_ui::sprites::paint_quad(
                &ui.painter_at(rect),
                texture,
                [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()],
            );
        }
    } else {
        frame.hint(&text("asset-texture-preview-empty"));
    }

    let _ = rel;
    event
}
