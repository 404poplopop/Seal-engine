//! Screen designer in the Asset Editor: a live preview of the in-game UI
//! screen, its node tree and the picked node's properties. Every change is a
//! command that writes the file at once.

use std::path::Path;

use egui::{Color32, Sense, Stroke, Vec2};
use seal_editor_core::gameui::{
    self, draw, scale_draws, Live, Screen, Theme, ANCHOR_PRESETS, KINDS, THEME_KEYS,
};
use seal_editor_core::ui_designer::UI_NODE_DRAFT;
use seal_editor_core::{CommandArgs, EditorSession};
use seal_ui::PanelFrame;

use crate::PanelEvent;

/// Size the preview lays the screen out at.
const DESIGN_SIZE: [f32; 2] = [1280.0, 720.0];
const PICK: Color32 = Color32::from_rgb(0xF0, 0x9A, 0x30);

fn cmd(id: &str, pairs: &[(&str, &str)]) -> (String, CommandArgs) {
    (id.to_string(), CommandArgs::new(pairs))
}

fn pick_cmd(name: &str) -> (String, CommandArgs) {
    cmd("draft.set", &[("key", UI_NODE_DRAFT), ("value", name)])
}

#[derive(Clone, Copy, Default)]
struct DragState {
    node: usize,
    /// 0 moves the node, 1 stretches its right edge, 2 its bottom edge.
    edge: u8,
    carry: [f32; 2],
}

/// Text typed into one property field that the file does not hold yet
/// (it is still being typed, or it was refused).
fn typed_id(rel: &str, node: &str, key: &str) -> egui::Id {
    egui::Id::new(("asset-ui-typed", rel, node, key))
}

pub(crate) fn screen_editor(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    full: &Path,
    text_fallback: impl FnOnce(&mut PanelFrame) -> PanelEvent,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut out: Vec<(String, CommandArgs)> = Vec::new();
    let raw = std::fs::read_to_string(full).unwrap_or_default();
    let as_text = session.draft("asset-ui-text").as_deref() == Some("1");
    let parsed = Screen::from_text(&raw);
    if as_text || parsed.is_err() {
        let mut event = PanelEvent::None;
        frame.bar(|frame| {
            if let Err(error) = &parsed {
                let e = error.to_string();
                frame.muted(
                    &session
                        .messages
                        .format("asset-ui-broken", &[("name", rel), ("error", e.as_str())]),
                );
            }
            if parsed.is_ok() && frame.toggle(&text("asset-ui-edit-text"), true) {
                event = PanelEvent::Commands(vec![cmd(
                    "draft.set",
                    &[("key", "asset-ui-text"), ("value", "0")],
                )]);
            }
        });
        let body = text_fallback(frame);
        if !matches!(body, PanelEvent::None) {
            event = body;
        }
        return event;
    }
    let Ok(screen) = parsed else { return PanelEvent::None };
    let root = session.project.as_ref().map(|p| p.root.clone()).unwrap_or_default();
    let (theme, theme_error) = if screen.theme.is_empty() {
        (Theme::default(), None)
    } else {
        match std::fs::read_to_string(root.join(&screen.theme)) {
            Ok(t) => match Theme::from_text(&t) {
                Ok(t) => (t, None),
                Err(e) => (Theme::default(), Some(format!("{}: {e}", screen.theme))),
            },
            Err(_) => (
                Theme::default(),
                Some(
                    session
                        .messages
                        .format("asset-ui-theme-missing", &[("name", screen.theme.as_str())]),
                ),
            ),
        }
    };
    let picked_name = session.draft(UI_NODE_DRAFT).unwrap_or_default();
    let picked = screen.find(&picked_name);

    // Add strip: one chip per kind of node.
    frame.bar(|frame| {
        frame.muted(&text("asset-ui-add"));
        for kind in KINDS {
            if frame.chip(&text(&format!("asset-ui-kind-{kind}")), false) {
                out.push(cmd("asset.ui_add", &[("kind", kind)]));
            }
        }
    });
    frame.bar(|frame| {
        if picked.is_some() {
            if frame.button(&text("asset-ui-up")) {
                out.push(cmd("asset.ui_move", &[("node", &picked_name), ("dir", "up")]));
            }
            if frame.button(&text("asset-ui-down")) {
                out.push(cmd("asset.ui_move", &[("node", &picked_name), ("dir", "down")]));
            }
            if frame.button(&text("asset-ui-remove")) {
                out.push(cmd("asset.ui_remove", &[("node", &picked_name)]));
            }
        }
        if screen.theme.is_empty() && frame.button(&text("asset-ui-theme-new")) {
            out.push(cmd("asset.theme_new", &[]));
        }
        match crate::inspector_pickers::paint_asset_row(
            frame,
            session,
            "theme",
            "screen-theme",
            &text("asset-ui-theme"),
            &screen.theme,
            false,
        ) {
            crate::inspector_pickers::AssetRow::Set(path) => {
                out.push(cmd("asset.ui_set", &[("key", "theme"), ("value", path.as_str())]));
            }
            crate::inspector_pickers::AssetRow::Clear => {
                out.push(cmd("asset.ui_set", &[("key", "theme"), ("value", "")]));
            }
            crate::inspector_pickers::AssetRow::Show => {
                out.push(cmd("picker.show_asset", &[("path", screen.theme.as_str())]));
            }
            crate::inspector_pickers::AssetRow::Idle => {}
        }
        if frame.toggle(&text("asset-ui-edit-text"), false) {
            out.push(cmd("draft.set", &[("key", "asset-ui-text"), ("value", "1")]));
        }
    });
    {
        let stored = screen.strings.clone();
        live_field(
            frame,
            session,
            egui::Id::new(("ui-strings", rel)),
            &text("asset-ui-strings"),
            &stored,
            |v| {
                let v = v.trim().replace('\\', "/");
                if v.is_empty() || (v.ends_with(".strings") && !v.split('/').any(|p| p == "..")) {
                    None
                } else {
                    Some("ui-error-property")
                }
            },
            |v| out.push(cmd("asset.ui_set", &[("key", "strings"), ("value", v.trim())])),
        );
    }
    if let Some(e) = &theme_error {
        frame.padded(|frame| frame.muted(e));
    }

    // Preview.
    let mut live = Live::new(screen.clone(), theme.clone());
    live.project = session.project.as_ref().map(|p| p.root.clone());
    live.resize(DESIGN_SIZE);
    let lay = live.layout();
    {
        let ui = frame.ui_mut();
        let ctx = ui.ctx().clone();
        let avail = Vec2::new(
            ui.available_width() - 16.0,
            (ui.available_height() * 0.45).clamp(160.0, 420.0),
        );
        ui.add_space(4.0);
        let (area, response) = ui.allocate_exact_size(avail, Sense::click_and_drag());
        let painter = ui.painter_at(area);
        painter.rect_filled(area, 0.0, Color32::from_gray(14));
        let k = (area.width() / DESIGN_SIZE[0]).min(area.height() / DESIGN_SIZE[1]).max(0.05);
        let view = egui::Rect::from_center_size(
            area.center(),
            Vec2::new(DESIGN_SIZE[0] * k, DESIGN_SIZE[1] * k),
        );
        painter.rect_filled(view, 0.0, Color32::from_rgb(0x22, 0x2A, 0x33));
        let mut draws = draw(&live, 0.0);
        gameui::resolve_paths(&mut draws, &root);
        seal_ui::paint_game_screens(&ctx, &painter, view, &scale_draws(&draws, k));
        painter.rect_stroke(
            view,
            0.0,
            Stroke::new(1.0, Color32::from_gray(90)),
            egui::StrokeKind::Outside,
        );
        let to_view = |r: gameui::Rect| {
            egui::Rect::from_min_size(
                view.min + Vec2::new(r.x * k, r.y * k),
                Vec2::new(r.w * k, r.h * k),
            )
        };
        let handles = |r: egui::Rect| {
            [
                egui::Rect::from_center_size(egui::pos2(r.right(), r.center().y), Vec2::splat(8.0)),
                egui::Rect::from_center_size(
                    egui::pos2(r.center().x, r.bottom()),
                    Vec2::splat(8.0),
                ),
            ]
        };
        let movable = |i: usize| {
            screen.nodes[i]
                .parent
                .is_none_or(|p| !matches!(screen.nodes[p].kind.as_str(), "vbox" | "hbox" | "grid"))
        };
        if let Some(i) = picked {
            if let Some(r) = lay.rect(i) {
                let r = to_view(r);
                painter.rect_stroke(r, 0.0, Stroke::new(2.0, PICK), egui::StrokeKind::Outside);
                if movable(i) {
                    for h in handles(r) {
                        painter.rect_filled(h, 0.0, PICK);
                    }
                }
            }
        }
        let to_design = |p: egui::Pos2| [(p.x - view.min.x) / k, (p.y - view.min.y) / k];
        let drag_id = egui::Id::new(("asset-ui-drag", rel));
        if response.clicked() {
            let hit = response
                .interact_pointer_pos()
                .and_then(|p| lay.hit(&screen, to_design(p), |_| true));
            out.push(pick_cmd(hit.map(|i| screen.nodes[i].name.as_str()).unwrap_or("")));
        }
        if response.drag_started() {
            if let Some(p) = response.interact_pointer_pos() {
                let on_handle = picked.filter(|&i| movable(i)).and_then(|i| {
                    let r = to_view(lay.rect(i)?);
                    handles(r)
                        .iter()
                        .position(|h| h.expand(3.0).contains(p))
                        .map(|e| (i, e as u8 + 1))
                });
                let start =
                    on_handle.or_else(|| lay.hit(&screen, to_design(p), |_| true).map(|i| (i, 0)));
                if let Some((i, edge)) = start {
                    if Some(i) != picked {
                        out.push(pick_cmd(&screen.nodes[i].name));
                    }
                    let state =
                        movable(i).then_some(DragState { node: i, edge, carry: [0.0, 0.0] });
                    ctx.data_mut(|d| d.insert_temp(drag_id, state));
                }
            }
        }
        if response.dragged() {
            let state: Option<DragState> = ctx.data(|d| d.get_temp(drag_id)).flatten();
            if let Some(mut s) = state {
                let delta = response.drag_delta();
                s.carry[0] += delta.x / k;
                s.carry[1] += delta.y / k;
                let (dx, dy) = (s.carry[0].trunc(), s.carry[1].trunc());
                if (dx != 0.0 || dy != 0.0) && s.node < screen.nodes.len() {
                    s.carry[0] -= dx;
                    s.carry[1] -= dy;
                    let edge = ["all", "right", "bottom"][s.edge as usize];
                    let (dx, dy) = (gameui::num(dx), gameui::num(dy));
                    out.push(cmd(
                        "asset.ui_nudge",
                        &[
                            ("node", &screen.nodes[s.node].name),
                            ("dx", &dx),
                            ("dy", &dy),
                            ("edge", edge),
                        ],
                    ));
                }
                ctx.data_mut(|d| d.insert_temp(drag_id, Some(s)));
            }
        }
        if response.drag_stopped() {
            ctx.data_mut(|d| d.insert_temp::<Option<DragState>>(drag_id, None));
        }
        ui.add_space(4.0);
    }

    // Tree on the left, properties on the right.
    let ui = frame.ui_mut();
    let height = ui.available_height();
    ui.columns(2, |cols| {
        egui::ScrollArea::vertical().id_salt(("asset-ui-tree", rel)).max_height(height).show(
            &mut cols[0],
            |ui| {
                let mut f = PanelFrame::new(ui);
                for (i, n) in screen.nodes.iter().enumerate() {
                    let detail = text(&format!("asset-ui-kind-{}", n.kind));
                    let row = seal_ui::TreeRow {
                        depth: screen.depth(i),
                        has_children: screen.children(Some(i)).next().is_some(),
                        open: true,
                        label: &n.name,
                        detail: &detail,
                        selected: Some(i) == picked,
                        hidden: !n.visible,
                        locked: false,
                        menu: &[],
                        drag_id: None,
                        place_kind: None,
                        rename_text: None,
                        component_row: false,
                        thumb: None,
                        marks: false,
                    };
                    if matches!(
                        f.tree_row(row),
                        seal_ui::TreeClick::Select
                            | seal_ui::TreeClick::SelectExtend
                            | seal_ui::TreeClick::SelectRange
                    ) {
                        out.push(pick_cmd(&n.name));
                    }
                }
                if screen.nodes.is_empty() {
                    f.muted(&text("asset-ui-empty"));
                }
            },
        );
        egui::ScrollArea::vertical().id_salt(("asset-ui-props", rel)).max_height(height).show(
            &mut cols[1],
            |ui| {
                let mut f = PanelFrame::new(ui);
                match picked {
                    Some(i) => node_fields(&mut f, session, rel, &screen, i, &mut out),
                    None => f.muted(&text("asset-ui-pick")),
                }
                if !screen.theme.is_empty() && theme_error.is_none() {
                    f.section(&text("asset-ui-theme"), |f| {
                        theme_fields(f, session, rel, &theme, &mut out)
                    });
                }
                f.hint(&text("asset-saved-hint"));
            },
        );
    });
    if out.is_empty() {
        PanelEvent::None
    } else {
        PanelEvent::Commands(out)
    }
}

/// One text field that saves on every keystroke the file accepts; text the
/// file refuses stays in the field with the reason under it.
fn live_field(
    f: &mut PanelFrame,
    session: &EditorSession,
    id: egui::Id,
    label: &str,
    stored: &str,
    check: impl Fn(&str) -> Option<&'static str>,
    mut save: impl FnMut(&str),
) {
    let ctx = f.ui_mut().ctx().clone();
    let typed: Option<String> = ctx.data(|d| d.get_temp(id));
    let mut buffer = typed.clone().unwrap_or_else(|| stored.to_string());
    let (changed, committed) = f.draft_field(label, &mut buffer);
    if changed {
        if check(&buffer).is_none() && buffer != stored {
            save(&buffer);
        }
        ctx.data_mut(|d| d.insert_temp(id, buffer.clone()));
    } else if typed.as_deref() == Some(stored) || (committed && check(&buffer).is_none()) {
        ctx.data_mut(|d| d.remove::<String>(id));
    }
    if let Some(why) = check(&buffer).filter(|_| buffer != stored) {
        f.muted(
            &session
                .messages
                .format("asset-ui-refused", &[("why", session.messages.get(why).as_str())]),
        );
    }
}

fn node_fields(
    f: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    screen: &Screen,
    i: usize,
    out: &mut Vec<(String, CommandArgs)>,
) {
    let text = |key: &str| session.messages.get(key);
    let n = &screen.nodes[i];
    f.muted(&session.messages.format(
        "asset-ui-node-kind",
        &[("kind", text(&format!("asset-ui-kind-{}", n.kind)).as_str())],
    ));
    let name = n.name.clone();
    live_field(
        f,
        session,
        typed_id(rel, &format!("#{i}"), "name"),
        &text("asset-ui-prop-name"),
        &name,
        |v| {
            let v = v.trim();
            if v.is_empty() || v.contains(char::is_whitespace) {
                Some("ui-error-name")
            } else if screen.find(v).is_some_and(|j| j != i) {
                Some("ui-error-name-taken")
            } else {
                None
            }
        },
        |v| out.push(cmd("asset.ui_set", &[("node", &name), ("key", "name"), ("value", v.trim())])),
    );
    let stacked =
        n.parent.is_some_and(|p| matches!(screen.nodes[p].kind.as_str(), "vbox" | "hbox" | "grid"));
    if stacked {
        f.muted(&text("asset-ui-stacked"));
    } else {
        f.label(&text("asset-ui-anchor-presets"));
        f.ui_mut().horizontal_wrapped(|ui| {
            let mut row = PanelFrame::new(ui);
            for (preset, a) in ANCHOR_PRESETS {
                if row.chip(&text(&format!("asset-ui-preset-{preset}")), n.anchor == *a) {
                    out.push(cmd(
                        "asset.ui_set",
                        &[("node", &name), ("key", "preset"), ("value", preset)],
                    ));
                }
            }
        });
    }
    for key in n.props() {
        if stacked && matches!(key, "anchor" | "offset") {
            continue;
        }
        if !stacked && key == "size" {
            continue;
        }
        let stored = n.prop(key).unwrap_or_default();
        let label = text(&format!("asset-ui-prop-{key}"));
        match key {
            "checked" | "visible" | "enabled" => {
                let on = stored == "yes";
                if f.toggle(&label, on) {
                    out.push(cmd(
                        "asset.ui_set",
                        &[("node", &name), ("key", key), ("value", if on { "no" } else { "yes" })],
                    ));
                }
            }
            "font" => match crate::inspector_pickers::paint_asset_row(
                f,
                session,
                "font",
                &format!("ui-font-{i}"),
                &label,
                &stored,
                false,
            ) {
                crate::inspector_pickers::AssetRow::Set(path) => out.push(cmd(
                    "asset.ui_set",
                    &[("node", &name), ("key", "font"), ("value", path.as_str())],
                )),
                crate::inspector_pickers::AssetRow::Clear => out
                    .push(cmd("asset.ui_set", &[("node", &name), ("key", "font"), ("value", "")])),
                crate::inspector_pickers::AssetRow::Show => {
                    out.push(cmd("picker.show_asset", &[("path", stored.as_str())]));
                }
                crate::inspector_pickers::AssetRow::Idle => {}
            },
            "align" => {
                f.label(&label);
                f.ui_mut().horizontal(|ui| {
                    let mut row = PanelFrame::new(ui);
                    for a in ["left", "center", "right"] {
                        if row.chip(&text(&format!("asset-ui-align-{a}")), stored == a) {
                            out.push(cmd(
                                "asset.ui_set",
                                &[("node", &name), ("key", "align"), ("value", a)],
                            ));
                        }
                    }
                });
            }
            _ => {
                let probe = n.clone();
                live_field(
                    f,
                    session,
                    typed_id(rel, &format!("#{i}"), key),
                    &label,
                    &stored,
                    |v| probe.clone().set_prop(key, v),
                    |v| {
                        out.push(cmd(
                            "asset.ui_set",
                            &[("node", &name), ("key", key), ("value", v)],
                        ))
                    },
                );
            }
        }
    }
}

fn theme_fields(
    f: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    theme: &Theme,
    out: &mut Vec<(String, CommandArgs)>,
) {
    for key in THEME_KEYS {
        let stored = theme.get(key).unwrap_or_default();
        if key == &"font" {
            let label = session.messages.get(&format!("asset-ui-theme-{key}"));
            match crate::inspector_pickers::paint_asset_row(
                f,
                session,
                "font",
                "theme-font",
                &label,
                &stored,
                false,
            ) {
                crate::inspector_pickers::AssetRow::Set(path) => {
                    out.push(cmd("asset.theme_set", &[("key", "font"), ("value", path.as_str())]));
                }
                crate::inspector_pickers::AssetRow::Clear => {
                    out.push(cmd("asset.theme_set", &[("key", "font"), ("value", "")]));
                }
                crate::inspector_pickers::AssetRow::Show => {
                    out.push(cmd("picker.show_asset", &[("path", stored.as_str())]));
                }
                crate::inspector_pickers::AssetRow::Idle => {}
            }
            continue;
        }
        live_field(
            f,
            session,
            typed_id(rel, "theme", key),
            &session.messages.get(&format!("asset-ui-theme-{key}")),
            &stored,
            |v| theme.clone().set(key, v),
            |v| out.push(cmd("asset.theme_set", &[("key", key), ("value", v)])),
        );
    }
}

/// A `.sealtheme` file on its own: its colors and sizes, saved as they change.
pub(crate) fn theme_editor(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    let raw = std::fs::read_to_string(full).unwrap_or_default();
    let mut out = Vec::new();
    match Theme::from_text(&raw) {
        Ok(theme) => frame.scroll(|f| {
            f.padded(|f| {
                theme_fields(f, session, rel, &theme, &mut out);
                f.hint(&session.messages.get("asset-saved-hint"));
            })
        }),
        Err(e) => {
            let e = e.to_string();
            frame.padded(|f| {
                f.muted(
                    &session
                        .messages
                        .format("asset-ui-broken", &[("name", rel), ("error", e.as_str())]),
                )
            });
        }
    }
    if out.is_empty() {
        PanelEvent::None
    } else {
        PanelEvent::Commands(out)
    }
}
