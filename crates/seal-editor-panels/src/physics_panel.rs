//! Physics worlds, which object steps in which world, and 3D collider shapes.

use seal_editor_core::{CommandArgs, EditorSession};
use seal_ui::PanelFrame;

use super::PanelEvent;

pub const PHYSICS: &str = "physics";

pub fn paint(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    let scene = &session.world.physics.scene;
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&text("panel-physics-title"));
            frame.muted(&text("physics-hint-worlds"));
            frame.section(&text("physics-worlds"), |frame| {
                for name in scene.world_names() {
                    let gravity = scene.gravity_of(&name, session.world.gravity);
                    let backend = scene.backend_of(&name);
                    frame.label(&name);
                    let mut g = gravity;
                    let gravity_label = format!("{} ({name})", text("physics-gravity"));
                    if frame.number(&gravity_label, &mut g, 0.05) {
                        event = command(
                            "physics.world",
                            &[
                                ("name", name.as_str()),
                                ("field", "gravity"),
                                ("value", &g.to_string()),
                            ],
                        );
                    }
                    let options = backend_options(&text);
                    let backend_label = format!("{} ({name})", text("physics-backend"));
                    if let Some(picked) = frame.choice(&backend_label, &backend, &options) {
                        event = command(
                            "physics.world",
                            &[
                                ("name", name.as_str()),
                                ("field", "backend"),
                                ("value", picked.as_str()),
                            ],
                        );
                    }
                    let remove = format!("{} {name}", text("physics-remove-world"));
                    if name != "main" && frame.button(&remove) {
                        event = command("physics.remove_world", &[("name", name.as_str())]);
                    }
                }
                if frame.button(&text("physics-add-world")) {
                    event = command("physics.add_world", &[]);
                }
            });
            frame.section(&text("physics-object"), |frame| {
                let ids = session.selected_ids();
                if ids.is_empty() {
                    frame.muted(&text("physics-object-empty"));
                    return;
                }
                let world = shared(&ids, |id| scene.member(id).unwrap_or("").to_string());
                let mut world_opts = vec![("".to_string(), text("physics-none"))];
                for name in scene.world_names() {
                    world_opts.push((name.clone(), name));
                }
                let world_refs: Vec<(&str, String)> =
                    world_opts.iter().map(|(id, title)| (id.as_str(), title.clone())).collect();
                if let Some(picked) = frame.choice(&text("physics-world"), &world, &world_refs) {
                    event = command("physics.member", &[("world", picked.as_str())]);
                }
                let shape = shared(&ids, |id| {
                    scene.collider(id).map(|c| c.shape.clone()).unwrap_or_default()
                });
                let shapes = shape_options(&text);
                if let Some(picked) = frame.choice(&text("physics-shape"), &shape, &shapes) {
                    event = command(
                        "physics.collider",
                        &[("field", "shape"), ("value", picked.as_str())],
                    );
                }
                if shape.is_empty() {
                    frame.muted(&text("physics-hint-shape"));
                    return;
                }
                let collider = ids.first().and_then(|id| scene.collider(id));
                match shape.as_str() {
                    "cylinder" => {
                        let mut radius = collider.map(|c| c.radius).unwrap_or(0.5);
                        if frame.number(&text("physics-radius"), &mut radius, 0.05) {
                            event = command(
                                "physics.collider",
                                &[("field", "radius"), ("value", &radius.to_string())],
                            );
                        }
                        let mut half = collider.map(|c| c.half_height).unwrap_or(0.5);
                        if frame.number(&text("physics-half-height"), &mut half, 0.05) {
                            event = command(
                                "physics.collider",
                                &[("field", "half_height"), ("value", &half.to_string())],
                            );
                        }
                        frame.muted(&text("physics-hint-cylinder"));
                    }
                    "trimesh" | "convex" => {
                        let mut mesh = collider.map(|c| c.mesh.clone()).unwrap_or_default();
                        let (changed, _) = frame.draft_field(&text("physics-mesh"), &mut mesh);
                        if changed {
                            event = command(
                                "physics.collider",
                                &[("field", "mesh"), ("value", mesh.as_str())],
                            );
                        }
                        if mesh.trim().is_empty() {
                            frame.muted(&text("physics-hint-mesh"));
                        }
                        if shape == "convex" {
                            let mut hulls = collider.map(|c| c.max_hulls as f32).unwrap_or(4.0);
                            if frame.number(&text("physics-max-hulls"), &mut hulls, 1.0) {
                                let n = hulls.round().clamp(1.0, 64.0) as u32;
                                event = command(
                                    "physics.collider",
                                    &[("field", "max_hulls"), ("value", &n.to_string())],
                                );
                            }
                            frame.muted(&text("physics-hint-convex"));
                        }
                    }
                    "heightfield" => {
                        let mut picture = collider.map(|c| c.picture.clone()).unwrap_or_default();
                        let (changed, _) =
                            frame.draft_field(&text("physics-picture"), &mut picture);
                        if changed {
                            event = command(
                                "physics.collider",
                                &[("field", "picture"), ("value", picture.as_str())],
                            );
                        }
                        if picture.trim().is_empty() {
                            frame.muted(&text("physics-hint-picture"));
                        }
                        let size = collider.map(|c| c.size).unwrap_or([4.0, 1.0, 4.0]);
                        let mut x = size[0];
                        if frame.number(&text("physics-size-x"), &mut x, 0.1) {
                            event = command(
                                "physics.collider",
                                &[("field", "size_x"), ("value", &x.to_string())],
                            );
                        }
                        let mut y = size[1];
                        if frame.number(&text("physics-size-y"), &mut y, 0.1) {
                            event = command(
                                "physics.collider",
                                &[("field", "size_y"), ("value", &y.to_string())],
                            );
                        }
                        let mut z = size[2];
                        if frame.number(&text("physics-size-z"), &mut z, 0.1) {
                            event = command(
                                "physics.collider",
                                &[("field", "size_z"), ("value", &z.to_string())],
                            );
                        }
                    }
                    _ => {}
                }
            });
        });
    });
    event
}

fn command(id: &str, pairs: &[(&str, &str)]) -> PanelEvent {
    PanelEvent::Command(id.to_string(), CommandArgs::new(pairs))
}

fn shared(ids: &[String], value: impl Fn(&str) -> String) -> String {
    let Some(first) = ids.first() else { return String::new() };
    let text = value(first);
    if ids.iter().skip(1).all(|id| value(id) == text) {
        text
    } else {
        String::new()
    }
}

fn backend_options(text: &impl Fn(&str) -> String) -> Vec<(&str, String)> {
    vec![("builtin", text("physics-backend-builtin")), ("drift", text("physics-backend-drift"))]
}

fn shape_options(text: &impl Fn(&str) -> String) -> Vec<(&str, String)> {
    vec![
        ("", text("physics-none")),
        ("cylinder", text("physics-shape-cylinder")),
        ("trimesh", text("physics-shape-trimesh")),
        ("heightfield", text("physics-shape-heightfield")),
        ("convex", text("physics-shape-convex")),
    ]
}
