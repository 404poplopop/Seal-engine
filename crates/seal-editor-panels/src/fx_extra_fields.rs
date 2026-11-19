//! Advanced particle effect fields in the Asset Editor and Inspector.

use seal_editor_core::asset_editor::EmitterDef;
use seal_editor_core::EditorSession;
use seal_editor_core::asset_editor::{
    EmitShape, FlipbookMode, MeshSampleMode, RendererMode, SphereVolume,
};
use seal_editor_core::CommandArgs;
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

fn shape_id(shape: &EmitShape) -> &'static str {
    match shape {
        EmitShape::Point => "point",
        EmitShape::Circle(_) => "circle",
        EmitShape::Ring(_, _) => "ring",
        EmitShape::Box(_) => "box",
        EmitShape::Edge(_) => "edge",
        EmitShape::Cone(_, _) => "cone",
        EmitShape::Sphere(_, _) => "sphere",
        EmitShape::Mesh { .. } => "mesh",
    }
}

fn renderer_id(renderer: &RendererMode) -> &'static str {
    match renderer {
        RendererMode::Billboard => "billboard",
        RendererMode::Stretched(_) => "stretched",
        RendererMode::Flipbook { .. } => "flipbook",
        RendererMode::Trail { .. } => "trail",
        RendererMode::Mesh { .. } => "mesh",
    }
}

fn fx_command(key: &str, value: &str, rel: Option<&str>) -> PanelEvent {
    if let Some(path) = rel {
        PanelEvent::Command(
            "asset.fx_set".into(),
            CommandArgs::new(&[("path", path), ("key", key), ("value", value)]),
        )
    } else {
        command("asset.fx_set", &[("key", key), ("value", value)])
    }
}

pub fn effect_extra_fields(
    frame: &mut PanelFrame,
    session: &EditorSession,
    def: &EmitterDef,
    defaults: &EmitterDef,
    rel: Option<&str>,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;

    let shape_options = [
        ("point", text("asset-fx-shape-point")),
        ("circle", text("asset-fx-shape-circle")),
        ("ring", text("asset-fx-shape-ring")),
        ("box", text("asset-fx-shape-box")),
        ("edge", text("asset-fx-shape-edge")),
        ("cone", text("asset-fx-shape-cone")),
        ("sphere", text("asset-fx-shape-sphere")),
        ("mesh", text("asset-fx-shape-mesh")),
    ];
    if let Some(picked) = frame
        .choice_row(
            &text("asset-fx-shape"),
            shape_id(&def.extras.shape),
            &shape_options,
            false,
            None,
            None,
            None,
            &mut None,
            None,
        )
        .0
    {
        let value = match picked.as_str() {
            "circle" => "circle 0.5".to_string(),
            "ring" => "ring 0.2 0.5".to_string(),
            "box" => "box 0.5 0.5".to_string(),
            "edge" => "edge -0.5 0 0.5 0".to_string(),
            "cone" => "cone 90 20".to_string(),
            "sphere" => "sphere 0.5 volume".to_string(),
            "mesh" => "mesh cube surface".to_string(),
            _ => picked,
        };
        return fx_command("shape", &value, rel);
    }

    match &def.extras.shape {
        EmitShape::Sphere(r, vol) => {
            let vol_options = [
                ("volume", text("asset-fx-sphere-volume")),
                ("surface", text("asset-fx-sphere-surface")),
            ];
            let vol_id = match vol {
                SphereVolume::Volume => "volume",
                SphereVolume::Surface => "surface",
            };
            let mut radius = *r;
            let (r_changed, _, _) = frame.number_row(
                &text("asset-fx-sphere-radius"),
                &mut radius,
                0.05,
                false,
                None,
                None,
                None,
                &mut None,
            );
            if let Some(picked) = frame
                .choice_row(
                    &text("asset-fx-sphere-fill"),
                    vol_id,
                    &vol_options,
                    false,
                    None,
                    None,
                    None,
                    &mut None,
                    None,
                )
                .0
            {
                return fx_command("shape", &format!("sphere {radius} {picked}"), rel);
            }
            if r_changed {
                return fx_command("shape", &format!("sphere {radius} {vol_id}"), rel);
            }
        }
        EmitShape::Mesh { path, mode } => {
            let mode_options = [
                ("surface", text("asset-fx-mesh-surface")),
                ("vertices", text("asset-fx-mesh-vertices")),
            ];
            let mode_id = match mode {
                MeshSampleMode::Surface => "surface",
                MeshSampleMode::Vertices => "vertices",
            };
            match crate::inspector_pickers::paint_asset_row(
                frame,
                session,
                "mesh",
                &format!("fx-mesh-{}", rel.unwrap_or("open")),
                &text("asset-fx-mesh-source"),
                path,
                false,
            ) {
                crate::inspector_pickers::AssetRow::Set(next) if !next.contains(char::is_whitespace) => {
                    return fx_command("shape", &format!("mesh {next} {mode_id}"), rel);
                }
                crate::inspector_pickers::AssetRow::Show => {
                    return command("picker.show_asset", &[("path", path.as_str())]);
                }
                _ => {}
            }
            let prim_options = [
                ("cube", "Cube".to_string()),
                ("sphere", "Sphere".to_string()),
                ("plane", "Plane".to_string()),
            ];
            if let Some(picked) = frame
                .choice_row(
                    &text("asset-fx-mesh-primitive"),
                    if ["cube", "sphere", "plane"].contains(&path.as_str()) {
                        path.as_str()
                    } else {
                        ""
                    },
                    &prim_options,
                    false,
                    None,
                    None,
                    None,
                    &mut None,
                    None,
                )
                .0
            {
                return fx_command("shape", &format!("mesh {picked} {mode_id}"), rel);
            }
            if let Some(picked) = frame
                .choice_row(
                    &text("asset-fx-mesh-sample"),
                    mode_id,
                    &mode_options,
                    false,
                    None,
                    None,
                    None,
                    &mut None,
                    None,
                )
                .0
            {
                return fx_command("shape", &format!("mesh {path} {picked}"), rel);
            }
        }
        _ => {}
    }

    let renderer_options = [
        ("billboard", text("asset-fx-renderer-billboard")),
        ("stretched", text("asset-fx-renderer-stretched")),
        ("flipbook", text("asset-fx-renderer-flipbook")),
        ("trail", text("asset-fx-renderer-trail")),
        ("mesh", text("asset-fx-renderer-mesh")),
    ];
    if let Some(picked) = frame
        .choice_row(
            &text("asset-fx-renderer"),
            renderer_id(&def.extras.renderer),
            &renderer_options,
            false,
            None,
            None,
            None,
            &mut None,
            None,
        )
        .0
    {
        let value = match picked.as_str() {
            "stretched" => "stretched 1.0".to_string(),
            // Start from the first picture or animation in the project; none means no change.
            "flipbook" => match crate::inspector_pickers::asset_paths(session, "sprite").first() {
                Some(path) => format!("flipbook {path} life"),
                None => return PanelEvent::None,
            },
            "trail" => "trail 8 0.2".to_string(),
            "mesh" => "mesh cube".to_string(),
            _ => picked,
        };
        return fx_command("renderer", &value, rel);
    }
    if let RendererMode::Mesh { path } = &def.extras.renderer {
        let prim_options = [
            ("cube", "Cube".to_string()),
            ("sphere", "Sphere".to_string()),
            ("plane", "Plane".to_string()),
        ];
        if let Some(picked) = frame
            .choice_row(
                &text("asset-fx-renderer-mesh-primitive"),
                if ["cube", "sphere", "plane"].contains(&path.as_str()) {
                    path.as_str()
                } else {
                    ""
                },
                &prim_options,
                false,
                None,
                None,
                None,
                &mut None,
                None,
            )
            .0
        {
            return fx_command("renderer", &format!("mesh {picked}"), rel);
        }
        match crate::inspector_pickers::paint_asset_row(
            frame,
            session,
            "mesh",
            &format!("fx-renderer-mesh-{}", rel.unwrap_or("open")),
            &text("asset-fx-renderer-mesh-asset"),
            path,
            false,
        ) {
            crate::inspector_pickers::AssetRow::Set(next) if !next.contains(char::is_whitespace) => {
                return fx_command("renderer", &format!("mesh {next}"), rel);
            }
            crate::inspector_pickers::AssetRow::Show => {
                return command("picker.show_asset", &[("path", path.as_str())]);
            }
            _ => {}
        }
    }
    if let RendererMode::Flipbook { path, mode } = &def.extras.renderer {
        let tag = if matches!(mode, FlipbookMode::Random) { "random" } else { "life" };
        let salt = format!("fx-flipbook-{}", rel.unwrap_or("open"));
        match crate::inspector_pickers::paint_asset_row(
            frame,
            session,
            "sprite",
            &salt,
            &text("asset-fx-flipbook-picture"),
            path,
            false,
        ) {
            crate::inspector_pickers::AssetRow::Set(next)
                if !next.contains(char::is_whitespace) =>
            {
                return fx_command("renderer", &format!("flipbook {next} {tag}"), rel);
            }
            crate::inspector_pickers::AssetRow::Show => {
                return command("picker.show_asset", &[("path", path.as_str())]);
            }
            _ => {}
        }
        let mode_options = [
            ("life", text("asset-fx-flipbook-life")),
            ("random", text("asset-fx-flipbook-random")),
        ];
        if let Some(picked) = frame
            .choice_row(
                &text("asset-fx-flipbook-mode"),
                tag,
                &mode_options,
                false,
                None,
                None,
                None,
                &mut None,
                None,
            )
            .0
        {
            return fx_command("renderer", &format!("flipbook {path} {picked}"), rel);
        }
    }

    let mut set: Option<(String, String)> = None;
    let mut num = |key: &str, label: &str, v: f32, d: f32, step: f32| {
        let mut value = v;
        let tip = session.messages.format("inspector-reset-default", &[("value", &d.to_string())]);
        let (changed, reset, _) = frame.number_row(
            label,
            &mut value,
            step,
            false,
            (v != d).then_some(tip.as_str()),
            None,
            None,
            &mut None,
        );
        if reset {
            set = Some((key.to_string(), d.to_string()));
        } else if changed {
            set = Some((key.to_string(), value.to_string()));
        }
    };

    num("drag", &text("asset-fx-drag"), def.extras.drag, defaults.extras.drag, 0.01);
    num(
        "noise_strength",
        &text("asset-fx-noise-strength"),
        def.extras.noise_strength,
        defaults.extras.noise_strength,
        0.01,
    );
    num(
        "noise_frequency",
        &text("asset-fx-noise-frequency"),
        def.extras.noise_frequency,
        defaults.extras.noise_frequency,
        0.1,
    );
    num("orbital", &text("asset-fx-orbital"), def.extras.orbital, defaults.extras.orbital, 0.05);
    num(
        "burst_repeat",
        &text("asset-fx-burst-repeat"),
        def.extras.burst_repeat,
        defaults.extras.burst_repeat,
        0.1,
    );
    num(
        "soft_fade",
        &text("asset-fx-soft-fade"),
        def.extras.soft_fade,
        defaults.extras.soft_fade,
        0.01,
    );

    num(
        "rotation_start.min",
        &text("asset-fx-rotation-start-min"),
        def.extras.rotation_start.0,
        defaults.extras.rotation_start.0,
        1.0,
    );
    num(
        "rotation_start.max",
        &text("asset-fx-rotation-start-max"),
        def.extras.rotation_start.1,
        defaults.extras.rotation_start.1,
        1.0,
    );
    num(
        "rotation_rate.min",
        &text("asset-fx-rotation-rate-min"),
        def.extras.rotation_rate.0,
        defaults.extras.rotation_rate.0,
        5.0,
    );
    num(
        "rotation_rate.max",
        &text("asset-fx-rotation-rate-max"),
        def.extras.rotation_rate.1,
        defaults.extras.rotation_rate.1,
        5.0,
    );

    for (index, burst) in def.extras.bursts.iter().enumerate() {
        let heading = format!("{} {}", text("asset-fx-burst-row"), index + 1);
        frame.readout(&heading, "");
        let mut time = burst.time;
        let mut count = burst.count as f32;
        let time_label = text("asset-fx-burst-time");
        let count_label = text("asset-fx-burst-count");
        let (t_changed, _, _) =
            frame.number_row(&time_label, &mut time, 0.05, false, None, None, None, &mut None);
        let (c_changed, _, _) =
            frame.number_row(&count_label, &mut count, 1.0, false, None, None, None, &mut None);
        if t_changed || c_changed {
            let value = format!("{index},{time},{}", count.round() as u32);
            return fx_command("burst.set", &value, rel);
        }
        if frame.button(&text("asset-fx-burst-remove")) {
            return fx_command("burst.remove", &index.to_string(), rel);
        }
    }
    if frame.button(&text("asset-fx-burst-add")) {
        return fx_command("burst.add", "0,10", rel);
    }

    if matches!(def.extras.renderer, RendererMode::Billboard) {
        if let Some(mat) = &def.extras.billboard_material {
            let loop_options = [("loop", "Loop".to_string()), ("once", "Once".to_string())];
            let loop_id = if mat.r#loop { "loop" } else { "once" };
            match crate::inspector_pickers::paint_asset_row(
                frame,
                session,
                "sprite",
                &format!("fx-bboard-sheet-{}", rel.unwrap_or("open")),
                &text("asset-fx-billboard-sheet"),
                &mat.sheet,
                false,
            ) {
                crate::inspector_pickers::AssetRow::Set(next) if !next.contains(char::is_whitespace) => {
                    return fx_command(
                        "billboard_material",
                        &format!("{next} {} {} {loop_id}", mat.cols, mat.rows),
                        rel,
                    );
                }
                crate::inspector_pickers::AssetRow::Show => {
                    return command("picker.show_asset", &[("path", mat.sheet.as_str())]);
                }
                _ => {}
            }
            let mut cols = mat.cols as f32;
            let mut rows = mat.rows as f32;
            let (c_changed, _, _) =
                frame.number_row(&text("asset-fx-sheet-cols"), &mut cols, 1.0, false, None, None, None, &mut None);
            let (r_changed, _, _) =
                frame.number_row(&text("asset-fx-sheet-rows"), &mut rows, 1.0, false, None, None, None, &mut None);
            if let Some(picked) = frame
                .choice_row(
                    &text("asset-fx-sheet-loop"),
                    loop_id,
                    &loop_options,
                    false,
                    None,
                    None,
                    None,
                    &mut None,
                    None,
                )
                .0
            {
                return fx_command(
                    "billboard_material",
                    &format!("{} {} {} {picked}", mat.sheet, mat.cols.max(1), mat.rows.max(1)),
                    rel,
                );
            }
            if c_changed || r_changed {
                return fx_command(
                    "billboard_material",
                    &format!(
                        "{} {} {} {loop_id}",
                        mat.sheet,
                        cols.round().max(1.0) as u32,
                        rows.round().max(1.0) as u32
                    ),
                    rel,
                );
            }
        } else if frame.button(&text("asset-fx-billboard-material-add")) {
            if let Some(path) = crate::inspector_pickers::asset_paths(session, "sprite").first() {
                return fx_command("billboard_material", &format!("{path} 4 1 loop"), rel);
            }
        }
    }

    let lit_options = [("0", "Unlit".to_string()), ("1", "Lit".to_string())];
    if let Some(picked) = frame
        .choice_row(
            &text("asset-fx-lit"),
            if def.extras.lit { "1" } else { "0" },
            &lit_options,
            def.extras.lit != defaults.extras.lit,
            None,
            None,
            None,
            &mut None,
            None,
        )
        .0
    {
        set = Some(("lit".into(), picked));
    }

    if let Some((key, value)) = set {
        event = fx_command(&key, &value, rel);
    }
    event
}
