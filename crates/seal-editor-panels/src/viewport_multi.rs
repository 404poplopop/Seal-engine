//! Multi-viewport tile layout inside the viewport panel.

use std::collections::BTreeMap;

use egui::{self, Align, Layout, Sense, UiBuilder};
use seal_editor_core::viewport_split::{self, PaneRect};
use seal_editor_core::{
    CommandArgs, EditorSession, Entity, PlayMode, Shape, VIEWPORT_SHOW_LAYER_IDS,
};
use seal_ui::{
    paint_viewport_pane_border, panel_body_rect, GridMark, GridMenus, GridViewOptions, PanelFrame,
    ViewportCamera, ViewportContextMenu,
};

use crate::viewport_menu;
use crate::PanelEvent;

fn viewport_axes(
    view: &str,
) -> (usize, usize, usize, f32, f32, (&'static str, &'static str), bool) {
    let wireframe = view != "perspective" && view != "2d";
    match view {
        "top" => (0, 2, 1, 1.0, 1.0, ("X", "Z"), wireframe),
        "bottom" => (0, 2, 1, 1.0, -1.0, ("X", "Z"), wireframe),
        "left" => (2, 1, 0, -1.0, 1.0, ("Z", "Y"), wireframe),
        "right" | "side" => (2, 1, 0, 1.0, 1.0, ("Z", "Y"), wireframe),
        "back" => (0, 1, 2, -1.0, 1.0, ("X", "Y"), wireframe),
        _ => (0, 1, 2, 1.0, 1.0, ("X", "Y"), wireframe),
    }
}

fn collect_shown<'a>(
    session: &'a EditorSession,
    layers: &BTreeMap<String, bool>,
    game: bool,
) -> Vec<&'a Entity> {
    let layer_on = |key: &str| layers.get(key).copied().unwrap_or(true);
    let cull_mask = session
        .foundation
        .selection
        .iter()
        .find_map(|id| {
            session
                .world
                .entity(id)
                .filter(|entity| entity.camera2d.on)
                .map(|entity| entity.camera2d.cull_mask)
        })
        .or_else(|| {
            session
                .world
                .entities
                .iter()
                .find(|entity| entity.camera2d.on)
                .map(|entity| entity.camera2d.cull_mask)
        })
        .unwrap_or(u32::MAX);
    session
        .world
        .entities
        .iter()
        .filter(|entity| {
            if entity.id == "root" || entity.hidden || entity.tag == "folder" {
                return false;
            }
            if !entity.sprite.is_empty()
                && !seal_editor_core::layer_visible(cull_mask, entity.layer)
            {
                return false;
            }
            if game {
                return entity.shape.is_some() || !entity.sprite.is_empty();
            }
            if entity.tag == "light" && !layer_on("lights") {
                return false;
            }
            if entity.shape.is_none() && entity.tag != "light" && !layer_on("icons") {
                return false;
            }
            entity.shape.is_none() || layer_on("statics")
        })
        .collect()
}

fn build_marks<'a>(
    session: &'a EditorSession,
    shown: &[&'a Entity],
    view: &str,
    layers: &BTreeMap<String, bool>,
) -> (Vec<GridMark<'a>>, bool) {
    let models = &session.models;
    let (across, up, depth_axis, flip_across, flip_up, names, _wireframe) = viewport_axes(view);
    let layer_on = |key: &str| layers.get(key).copied().unwrap_or(true);
    let draw_grid = models.viewport_grid && layer_on("grid");
    let marks: Vec<GridMark> = shown
        .iter()
        .map(|entity| {
            let scale = entity.scale;
            let (size, round, mesh) = match entity.shape {
                Some(Shape::Cube) => (
                    [0.5 * scale[across], 0.5 * scale[up], 0.5 * scale[depth_axis]],
                    false,
                    Some(seal_ui::MeshPrimitive::Cube),
                ),
                Some(Shape::Sphere) => {
                    let r = 0.5 * scale[across].max(scale[up]).max(scale[depth_axis]);
                    ([r, r, r], true, Some(seal_ui::MeshPrimitive::Sphere))
                }
                Some(Shape::Cylinder) | Some(Shape::Cone) | Some(Shape::Capsule) => (
                    [
                        0.5 * scale[across].max(scale[depth_axis]),
                        0.5 * scale[up],
                        0.5 * scale[across].max(scale[depth_axis]),
                    ],
                    false,
                    Some(seal_ui::MeshPrimitive::Cube),
                ),
                Some(Shape::Plane) => (
                    [20.0 * scale[across], 0.05 * scale[up], 20.0 * scale[depth_axis]],
                    false,
                    Some(seal_ui::MeshPrimitive::Plane),
                ),
                None => ([0.3, 0.3, 0.3], false, None),
            };
            let (sprite, uv) = crate::sprite_draw(session, entity);
            let card =
                sprite.as_deref().map(|path| crate::sprite_card(path, scale[across], scale[up]));
            let (size, round, mesh) = if let Some(card) = card {
                ([card.full[0] * 0.5, card.full[1] * 0.5, 0.5 * scale[depth_axis]], false, None)
            } else {
                (size, round, mesh)
            };
            let mut half = [size[across], size[up]];
            if sprite.is_none() && !entity.sprite_mask.shape.is_empty() {
                half = [0.5 * scale[across].abs(), 0.5 * scale[up].abs()];
            }
            let spin = match view {
                "top" | "bottom" => entity.rotation[1],
                "left" | "right" => entity.rotation[0],
                _ => entity.rotation[2],
            };
            let billboard = if entity.shape.is_none() && sprite.is_none() {
                match entity.tag.as_str() {
                    "camera" => Some(seal_ui::Icon::EntityCamera),
                    "light" => Some(seal_ui::Icon::EntityLightDirectional),
                    "player" => Some(seal_ui::Icon::EntityPlayerStart),
                    _ => None,
                }
            } else {
                None
            };
            GridMark {
                x: entity.translation[across] * flip_across,
                y: entity.translation[up] * flip_up,
                half,
                round,
                billboard,
                selected: session.foundation.selection.contains(&entity.id)
                    || models.selection.as_deref() == Some(entity.id.as_str()),
                faint: size[across] > 1.0 && size[up] > 1.0,
                label: &entity.name,
                tint: entity.color.map(|channel| (channel.clamp(0.0, 1.0) * 255.0) as u8),
                angle: spin.to_radians(),
                depth: entity.translation[depth_axis],
                size,
                mesh,
                pickable: !entity.locked,
                sprite,
                layer: entity.layer,
                uv,
                order: entity.order,
                sprite_tint: entity.tint.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8),
                slice: crate::sprite_place(session, entity).1,
                slice_mode: entity.slice_mode.as_str(),
                slice_size: entity.slice_size,
                // The object's own pivot wins, then the animation frame's pivot.
                pivot: if entity.pivot != [0.5, 0.5] {
                    entity.pivot
                } else {
                    crate::sprite_place(session, entity).0
                },
                normal_map: crate::normal_map_path(session, entity),
                shape2d: crate::shape2d_mark(session, entity, [across, up], [flip_across, flip_up])
                    .or_else(|| {
                        let selected = session.foundation.selection.contains(&entity.id)
                            || models.selection.as_deref() == Some(entity.id.as_str());
                        crate::light2d_view::mark(
                            entity,
                            [across, up],
                            [flip_across, flip_up],
                            selected && view == "front" && !entity.locked,
                        )
                    }),
                sprite_shape: crate::sprite_shape::view_mark(
                    session,
                    entity,
                    [across, up],
                    [flip_across, flip_up],
                    false,
                ),
                material: crate::material_mark(session, entity),
                mask_mode: match entity.mask.as_str() {
                    "inside" => 1,
                    "outside" => 2,
                    _ => 0,
                },
                mask_kind: match entity.sprite_mask.shape.as_str() {
                    "rect" => 1,
                    "circle" => 2,
                    "alpha" => 3,
                    _ => 0,
                },
                surface: crate::mesh_surface(session, entity),
            }
        })
        .collect();
    (marks, draw_grid && !names.0.is_empty())
}

fn pane_masks(session: &EditorSession, view: &str) -> Vec<seal_ui::sprite_mask::ViewMask> {
    if view == "perspective" {
        return Vec::new();
    }
    let (across, up, _, flip_across, flip_up, _, _) = viewport_axes(view);
    let root = session.project.as_ref().map(|project| project.root.as_path());
    seal_ui::sprite_mask::view_masks(
        &session.world.entities,
        [across, up],
        [flip_across, flip_up],
        |entity| match view {
            "top" | "bottom" => entity.rotation[1],
            "left" | "right" => entity.rotation[0],
            _ => entity.rotation[2],
        },
        root,
    )
}

fn vp_cmd(panel_id: &str, id: &str, args: &[(&str, &str)]) -> PanelEvent {
    let mut pairs = vec![("panel", panel_id)];
    pairs.extend_from_slice(args);
    PanelEvent::Command(id.into(), CommandArgs::new(&pairs))
}

pub(crate) fn paint_extra_tab(
    frame: &mut PanelFrame,
    session: &EditorSession,
    panel_id: &str,
) -> PanelEvent {
    let models = &session.models;
    let Some(tab) = models.viewport_tabs.get(panel_id) else {
        return PanelEvent::None;
    };
    let pane = tab;
    let entity_entries = viewport_menu::entity_entries(session);
    let empty_entries = viewport_menu::empty_entries(session);
    let shown = collect_shown(session, &models.viewport_show_layers, false);
    let outer = panel_body_rect(frame.ui_mut());
    let play_frame = seal_editor_core::play_tint_active(session.world.mode);
    paint_viewport_pane_border(frame.ui_mut(), outer, true, play_frame);
    let mut pane_ui = frame.ui_mut().new_child(
        UiBuilder::new().max_rect(outer.shrink(1.0)).layout(Layout::top_down(Align::LEFT)),
    );
    if models.viewport_show_toolbar {
        let key = format!("viewport-view-{}", pane.view);
        let label = session.messages.get(&key);
        let plate = pane_ui.visuals().panel_fill.gamma_multiply(0.75);
        egui::Frame::new().fill(plate).inner_margin(egui::Margin::symmetric(4, 2)).show(
            &mut pane_ui,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.separator();
                    let shade =
                        session.messages.get(&format!("viewport-shade-{}", models.viewport_shade));
                    ui.label(shade);
                });
            },
        );
    }
    let mut child = PanelFrame::new(&mut pane_ui);
    let (marks, draw_grid) =
        build_marks(session, &shown, pane.view.as_str(), &models.viewport_show_layers);
    let masks2d = pane_masks(session, pane.view.as_str());
    let (_, _, _, _, _, names, wireframe) = viewport_axes(pane.view.as_str());
    let mut camera = ViewportCamera {
        pan_x: pane.pan_x,
        pan_y: pane.pan_y,
        zoom: pane.zoom,
        yaw_deg: pane.yaw,
        pitch_deg: pane.pitch,
        distance: pane.distance,
    };
    let hit = child.grid_view(
        &marks,
        draw_grid,
        names,
        "select",
        GridMenus {
            entity: ViewportContextMenu { caption: None, entries: &entity_entries },
            empty: ViewportContextMenu { caption: None, entries: &empty_entries },
        },
        &mut camera,
        GridViewOptions {
            game_view: false,
            stats_overlay: None,
            stats_overlay_edge: seal_editor_core::viewport_overlays::Edge::TopRight,
            scene_caption: None,
            play_badge: None,
            game_ui: &[],
            game_screens: &[],
            warnings: &[],
            show_names: false,
            wireframe,
            shade: models.viewport_shade.as_str(),
            perspective: pane.view == "perspective",
            camera_speed: models.viewport_camera_speed,
            orbit_pivot: None,
            light_dir: [0.2, 0.85, 0.3],
            shadows: false,
            shadow_map_size: 384,
            fill_lights: &[],
            lights2d: &[],
            blockers2d: &[],
            ambient2d: crate::AMBIENT_2D,
            masks2d: &masks2d,
            gi: false,
            axes_on_top: true,
            gizmo_pivot: None,
            gizmo_world: None,
            gizmo_start_rot: None,
            gizmo_start_scale: None,
            gizmo_space: models.viewport_space.as_str(),
            vertex_snap: false,
            paint_brush: None,
            paint_grid: "square",
            paint_erase: false,
            paint_tool: "brush",
            paint_layer: 0,
            prefab_banner: None,
            measure_mode: models.viewport_tool == "collider",
            sprite_order: None,
            glow: None,
            anisotropy: seal_editor_core::render_quality_from_session(session).anisotropy,
            render_scale: seal_editor_core::render_quality_from_session(session).render_scale,
            light_gain: seal_ui::scene_light_gain(
                session
                    .world
                    .entities
                    .iter()
                    .find(|entity| entity.tag == "light")
                    .map(|entity| entity.intensity)
                    .unwrap_or(1.0),
                session.world.glow.enabled,
                session.world.glow.intensity,
            ),
            sky_cubemap: session.world.sky_cubemap.as_str(),
            texture_root: session.project.as_ref().map(|project| project.root.as_path()),
        },
    );
    if hit.camera_dirty
        || (camera.pan_x - pane.pan_x).abs() > 0.01
        || (camera.pan_y - pane.pan_y).abs() > 0.01
        || (camera.zoom - pane.zoom).abs() > f32::EPSILON
    {
        return vp_cmd(
            panel_id,
            "viewport.camera",
            &[
                ("pan_x", &camera.pan_x.to_string()),
                ("pan_y", &camera.pan_y.to_string()),
                ("zoom", &camera.zoom.to_string()),
                ("yaw", &camera.yaw_deg.to_string()),
                ("pitch", &camera.pitch_deg.to_string()),
                ("distance", &camera.distance.to_string()),
            ],
        );
    }
    PanelEvent::None
}

fn view_label(session: &EditorSession, view: &str) -> String {
    let key = match view {
        "perspective" => "viewport-view-perspective",
        "top" => "viewport-view-top",
        "front" => "viewport-view-front",
        "right" | "side" => "view-side",
        "2d" => "viewport-view-2d",
        _ => return session.messages.get(&format!("viewport-view-{view}")),
    };
    session.messages.get(key)
}

fn egui_rect(rect: PaneRect) -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(rect.x, rect.y), egui::vec2(rect.w, rect.h))
}

fn center_pan(camera: &ViewportCamera, perspective: bool, x: f32, y: f32) -> (f32, f32) {
    let dolly = if perspective { 10.0 / camera.distance.max(1.0) } else { 1.0 };
    let pitch_scale =
        if perspective { camera.pitch_deg.to_radians().cos().abs().max(0.35) } else { 1.0 };
    let cell = 20.0 * camera.zoom * dolly * pitch_scale;
    let (sin_yaw, cos_yaw) = camera.yaw_deg.to_radians().sin_cos();
    let rx = x * cos_yaw - y * sin_yaw;
    let ry = x * sin_yaw + y * cos_yaw;
    (-rx * cell, ry * cell)
}

fn play_follow(session: &EditorSession, view: &str, camera: &mut ViewportCamera) {
    let (across, up, _, flip_across, flip_up, _, _) = viewport_axes(view);
    let perspective = view == "perspective";
    let shake = session.world.play_shake();
    if let Some(player) = session.world.play_player().and_then(|id| session.world.entity(id)) {
        let x = player.translation[across] * flip_across + shake[0];
        let y = player.translation[up] * flip_up + shake[1];
        let (pan_x, pan_y) = center_pan(camera, perspective, x, y);
        camera.pan_x = pan_x;
        camera.pan_y = pan_y;
    } else if let Some(target) = session.world.play_view_target() {
        let (pan_x, pan_y) =
            center_pan(camera, perspective, target[across] * flip_across, target[up] * flip_up);
        camera.pan_x = pan_x;
        camera.pan_y = pan_y;
    }
}

enum HeaderPick {
    View(String),
    Shade(String),
    Show(String, bool),
    Expand,
}

pub(crate) fn paint(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let models = &session.models;
    let preset = models.viewport_layout_preset.as_str();
    let active = models.viewport_active_pane.min(models.viewport_panes.len().saturating_sub(1));
    let outer = panel_body_rect(frame.ui_mut());
    let gap = 3.0;
    let body = PaneRect { x: outer.min.x, y: outer.min.y, w: outer.width(), h: outer.height() };
    let expanded = models.viewport_pane_expanded && preset != "single";
    let layout_rects = viewport_split::pane_rects(body, preset, &models.viewport_splits, gap);
    let rects = if expanded { vec![body] } else { layout_rects };
    let pointer = frame.ui_mut().input(|input| input.pointer.hover_pos());
    let hot_slot = pointer.and_then(|pos| viewport_split::pane_under_pointer(&rects, pos.x, pos.y));
    let slot_pane = |slot: usize| if expanded { active } else { slot };
    let hot = hot_slot.map(slot_pane);
    let focus = hot.unwrap_or(active);
    let entity_entries = viewport_menu::entity_entries(session);
    let empty_entries = viewport_menu::empty_entries(session);
    let playing = matches!(session.world.mode, PlayMode::Playing | PlayMode::Paused);
    let budget_mb = session
        .foundation
        .settings
        .get(seal_editor_core::texture_memory::TEXTURE_MEMORY_BUDGET_MB)
        .and_then(|v| v.parse().ok())
        .unwrap_or(seal_editor_core::texture_memory::DEFAULT_TEXTURE_MEMORY_BUDGET_MB);
    seal_ui::sprites::sync_texture_budget_mb(budget_mb);
    let play_paths = seal_editor_core::play_textures::scene_sprite_pictures(
        &session.world,
        session.project.as_ref().map(|p| p.root.as_path()),
    );
    seal_ui::sprites::preload_play_textures(frame.ui_mut().ctx(), playing, &play_paths);
    let play_label = match session.world.mode {
        PlayMode::Playing => Some(session.messages.get("play-running")),
        PlayMode::Paused => Some(session.messages.get("play-paused")),
        PlayMode::Editing => None,
    };
    let game_ui = session.world.play_ui_lines();
    let mut game_screens = session.world.play_screen_draws();
    if let Some(project) = session.project.as_ref() {
        seal_editor_core::gameui::resolve_paths(&mut game_screens, &project.root);
    }
    let view_choices = [
        ("perspective", view_label(session, "perspective")),
        ("top", view_label(session, "top")),
        ("front", view_label(session, "front")),
        ("side", view_label(session, "side")),
        ("2d", view_label(session, "2d")),
    ];
    let shade_choices = [
        ("lit", session.messages.get("viewport-shade-lit")),
        ("unlit", session.messages.get("viewport-shade-unlit")),
        ("wireframe", session.messages.get("viewport-shade-wireframe")),
        ("collision", session.messages.get("viewport-shade-collision")),
    ];
    let show_heading = session.messages.get("viewport-show-pill");
    let max_label = session.messages.get(if expanded {
        "viewport-pane-restore"
    } else {
        "viewport-pane-maximize"
    });
    let max_tip = session.messages.get("viewport-pane-max-tip");
    let mut event = PanelEvent::None;
    for (slot, rect) in rects.iter().copied().enumerate() {
        let pane_index = slot_pane(slot);
        if pane_index >= models.viewport_panes.len() {
            continue;
        }
        let pane = &models.viewport_panes[pane_index];
        let is_focus = pane_index == focus;
        let game_here = is_focus && (playing || session.game_view);
        let layers = if pane.show_layers.is_empty() {
            &models.viewport_show_layers
        } else {
            &pane.show_layers
        };
        let shown = collect_shown(session, layers, game_here);
        let play_frame = seal_editor_core::play_tint_active(session.world.mode);
        paint_viewport_pane_border(frame.ui_mut(), egui_rect(rect), is_focus, play_frame);
        let mut pane_ui = frame.ui_mut().new_child(
            UiBuilder::new()
                .max_rect(egui_rect(rect).shrink(1.0))
                .layout(Layout::top_down(Align::LEFT)),
        );
        let mut header_pick = None;
        let plate = pane_ui.visuals().panel_fill.gamma_multiply(0.75);
        egui::Frame::new().fill(plate).inner_margin(egui::Margin::symmetric(4, 2)).show(
            &mut pane_ui,
            |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let grip = ui.allocate_response(egui::vec2(12.0, 16.0), Sense::click());
                    if grip.double_clicked() {
                        header_pick = Some(HeaderPick::Expand);
                    }
                    let current = view_label(session, pane.view.as_str());
                    egui::ComboBox::from_id_salt(format!("pane-view-{pane_index}"))
                        .selected_text(current)
                        .width(88.0)
                        .show_ui(ui, |ui| {
                            for (id, label) in &view_choices {
                                if ui
                                    .selectable_label(
                                        pane.view == *id || (pane.view == "right" && *id == "side"),
                                        label,
                                    )
                                    .clicked()
                                {
                                    header_pick = Some(HeaderPick::View((*id).to_string()));
                                }
                            }
                        });
                    let shade_id = if pane.shade.is_empty() {
                        models.viewport_shade.as_str()
                    } else {
                        pane.shade.as_str()
                    };
                    let shade_label = session.messages.get(&format!("viewport-shade-{shade_id}"));
                    egui::ComboBox::from_id_salt(format!("pane-shade-{pane_index}"))
                        .selected_text(shade_label)
                        .width(78.0)
                        .show_ui(ui, |ui| {
                            for (id, label) in &shade_choices {
                                if ui.selectable_label(shade_id == *id, label).clicked() {
                                    header_pick = Some(HeaderPick::Shade((*id).to_string()));
                                }
                            }
                        });
                    ui.menu_button(&show_heading, |ui| {
                        for id in VIEWPORT_SHOW_LAYER_IDS {
                            let mut on = layers.get(*id).copied().unwrap_or(true);
                            let label = session.messages.get(&format!("viewport-show-{id}"));
                            if ui.checkbox(&mut on, label).changed() {
                                header_pick = Some(HeaderPick::Show((*id).to_string(), on));
                            }
                        }
                    });
                    if ui.button(&max_label).on_hover_text(&max_tip).clicked() {
                        header_pick = Some(HeaderPick::Expand);
                    }
                });
            },
        );
        if let Some(pick) = header_pick {
            if matches!(event, PanelEvent::None) {
                let idx = pane_index.to_string();
                event = match pick {
                    HeaderPick::View(view) => vp_cmd(
                        "viewport",
                        "viewport.view",
                        &[("view", view.as_str()), ("index", idx.as_str())],
                    ),
                    HeaderPick::Shade(shade) => vp_cmd(
                        "viewport",
                        "viewport.shade",
                        &[("mode", shade.as_str()), ("index", idx.as_str())],
                    ),
                    HeaderPick::Show(layer, on) => vp_cmd(
                        "viewport",
                        "viewport.show",
                        &[
                            ("layer", layer.as_str()),
                            ("on", if on { "1" } else { "0" }),
                            ("index", idx.as_str()),
                        ],
                    ),
                    HeaderPick::Expand => {
                        vp_cmd("viewport", "viewport.pane_expand", &[("index", idx.as_str())])
                    }
                };
            }
        }
        let mut child = PanelFrame::new(&mut pane_ui);
        let (marks, mut draw_grid) = build_marks(session, &shown, pane.view.as_str(), layers);
        if game_here {
            draw_grid = false;
        }
        let masks2d = pane_masks(session, pane.view.as_str());
        let (_, _, _, _, _, names, wireframe) = viewport_axes(pane.view.as_str());
        let mut camera = ViewportCamera {
            pan_x: pane.pan_x,
            pan_y: pane.pan_y,
            zoom: pane.zoom,
            yaw_deg: pane.yaw,
            pitch_deg: pane.pitch,
            distance: pane.distance,
        };
        if game_here && playing {
            play_follow(session, pane.view.as_str(), &mut camera);
        }
        let camera_before = camera;
        let perspective = pane.view == "perspective";
        let light_dir = [0.2_f32, 0.85, 0.3];
        let tool = if game_here { "select" } else { models.viewport_tool.as_str() };
        let gizmo_pivot = if game_here || session.world.mode != PlayMode::Editing {
            None
        } else {
            session
                .foundation
                .selection
                .last()
                .and_then(|sel| shown.iter().position(|entity| entity.id == *sel))
                .or_else(|| {
                    models
                        .selection
                        .as_deref()
                        .and_then(|sel| shown.iter().position(|entity| entity.id == sel))
                })
        };
        let gizmo_world = gizmo_pivot.map(|ix| shown[ix].translation);
        let gizmo_start_rot = gizmo_pivot.map(|ix| shown[ix].rotation);
        let gizmo_start_scale = gizmo_pivot.map(|ix| shown[ix].scale);
        let shade = if pane.shade.is_empty() {
            models.viewport_shade.as_str()
        } else {
            pane.shade.as_str()
        };
        let badge = play_label
            .as_ref()
            .filter(|_| game_here)
            .map(|label| (label.as_str(), session.world.mode == PlayMode::Playing));
        let hit = child.grid_view(
            &marks,
            draw_grid,
            names,
            tool,
            GridMenus {
                entity: ViewportContextMenu { caption: None, entries: &entity_entries },
                empty: ViewportContextMenu { caption: None, entries: &empty_entries },
            },
            &mut camera,
            GridViewOptions {
                game_view: game_here,
                stats_overlay: None,
                stats_overlay_edge: seal_editor_core::viewport_overlays::Edge::TopRight,
                scene_caption: None,
                play_badge: badge,
                game_ui: if game_here { &game_ui } else { &[] },
                game_screens: if game_here { &game_screens } else { &[] },
                warnings: &[],
                show_names: false,
                wireframe: wireframe && !game_here,
                shade,
                perspective,
                camera_speed: models.viewport_camera_speed,
                orbit_pivot: None,
                light_dir,
                shadows: layers.get("shadows").copied().unwrap_or(true),
                shadow_map_size: seal_editor_core::editor_shadow_map_size(session),
                fill_lights: &[],
                lights2d: &[],
                blockers2d: &[],
                ambient2d: crate::AMBIENT_2D,
                masks2d: &masks2d,
                gi: false,
                axes_on_top: true,
                gizmo_pivot,
                gizmo_world,
                gizmo_start_rot,
                gizmo_start_scale,
                gizmo_space: models.viewport_space.as_str(),
                vertex_snap: false,
                paint_brush: None,
                paint_grid: "square",
                paint_erase: false,
                paint_tool: "brush",
                paint_layer: 0,
                prefab_banner: None,
                measure_mode: tool == "measure" || tool == "collider",
                sprite_order: None,
                glow: (session.world.glow.enabled && session.models.quality_glow && pane.view == "front").then_some([
                    session.world.glow.threshold,
                    session.world.glow.intensity,
                    session.world.glow.radius,
                ]),
                anisotropy: seal_editor_core::render_quality_from_session(session).anisotropy,
            render_scale: seal_editor_core::render_quality_from_session(session).render_scale,
                light_gain: seal_ui::scene_light_gain(
                    session
                        .world
                        .entities
                        .iter()
                        .find(|entity| entity.tag == "light")
                        .map(|entity| entity.intensity)
                        .unwrap_or(1.0),
                    session.world.glow.enabled,
                    session.world.glow.intensity,
                ),
                sky_cubemap: session.world.sky_cubemap.as_str(),
                texture_root: session.project.as_ref().map(|project| project.root.as_path()),
            },
        );
        // The Measure tool works in the active pane; its path is shared by every pane.
        if is_focus && tool == "measure" && !game_here && !playing {
            let (across, up, depth, flip_across, flip_up, _, _) = viewport_axes(pane.view.as_str());
            let axes = crate::measure_tool::MeasureViewAxes {
                across,
                up,
                _spin: depth,
                flip_across,
                flip_up,
            };
            let ppu = session
                .world
                .entities
                .iter()
                .map(|entity| entity.camera2d.pixel_snap)
                .find(|ppu| *ppu > 0.0)
                .unwrap_or(0.0);
            let escape = matches!(hit.key.as_deref(), Some("Escape") | Some("KeyEscape"));
            let measured = crate::measure_tool::handle_viewport(
                session,
                hit.measure.as_ref(),
                escape,
                pane.view == "2d",
                perspective,
                ppu,
                &camera,
                pane.view.as_str(),
            );
            crate::measure_tool::paint_overlay(
                &mut child,
                session,
                &camera,
                perspective,
                axes,
                hit.measure.as_ref(),
                pane.view.as_str(),
            );
            if matches!(event, PanelEvent::None) && !matches!(measured, PanelEvent::None) {
                event = measured;
            }
        }
        let camera_moved = (camera.pan_x - camera_before.pan_x).abs() > 0.01
            || (camera.pan_y - camera_before.pan_y).abs() > 0.01
            || (camera.zoom - camera_before.zoom).abs() > f32::EPSILON
            || (camera.yaw_deg - camera_before.yaw_deg).abs() > 0.01
            || (camera.pitch_deg - camera_before.pitch_deg).abs() > 0.01
            || (camera.distance - camera_before.distance).abs() > 0.01;
        if matches!(event, PanelEvent::None) && !game_here && (hit.camera_dirty || camera_moved) {
            let idx = pane_index.to_string();
            event = vp_cmd(
                "viewport",
                "viewport.camera",
                &[
                    ("index", idx.as_str()),
                    ("pan_x", &camera.pan_x.to_string()),
                    ("pan_y", &camera.pan_y.to_string()),
                    ("zoom", &camera.zoom.to_string()),
                    ("yaw", &camera.yaw_deg.to_string()),
                    ("pitch", &camera.pitch_deg.to_string()),
                    ("distance", &camera.distance.to_string()),
                ],
            );
        }
        if matches!(event, PanelEvent::None) {
            if let Some(ix) =
                hit.picked.filter(|_| session.world.play_player().is_none() && !game_here)
            {
                if let Some(entity) = shown.get(ix) {
                    event = vp_cmd(
                        "viewport",
                        "outliner.select",
                        &[("id", entity.id.as_str()), ("reveal", "1")],
                    );
                }
            }
        }
    }
    if !expanded && matches!(event, PanelEvent::None) {
        for bar in viewport_split::split_bars(body, preset, &models.viewport_splits, gap) {
            let rect =
                egui::Rect::from_min_size(egui::pos2(bar.x, bar.y), egui::vec2(bar.w, bar.h));
            let cursor = if bar.vertical {
                egui::CursorIcon::ResizeHorizontal
            } else {
                egui::CursorIcon::ResizeVertical
            };
            let response =
                frame.ui_mut().allocate_rect(rect, Sense::drag()).on_hover_cursor(cursor);
            let color = frame.ui_mut().visuals().widgets.noninteractive.bg_stroke.color;
            frame.ui_mut().painter().rect_filled(rect, 0.0, color);
            if response.dragged() {
                let mut next = viewport_split::ratios_for(preset, &models.viewport_splits);
                let span = if bar.vertical { body.w - gap } else { body.h - gap };
                let delta =
                    if bar.vertical { response.drag_delta().x } else { response.drag_delta().y };
                if span > 1.0 && bar.index < next.len() {
                    next[bar.index] = (next[bar.index] + delta / span).clamp(0.15, 0.85);
                    let split_values =
                        next.iter().map(|value| value.to_string()).collect::<Vec<_>>().join(",");
                    event =
                        vp_cmd("viewport", "viewport.split", &[("values", split_values.as_str())]);
                }
            }
        }
    }
    if matches!(event, PanelEvent::None) {
        if let Some(index) = hot {
            if index != active {
                event = vp_cmd("viewport", "viewport.pane_focus", &[("index", &index.to_string())]);
            }
        }
    }
    event
}
