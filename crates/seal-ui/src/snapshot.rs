//! Headless AccessKit trees for visual snapshot tests (no display).

use std::collections::HashMap;

use accesskit::{Node, NodeId, Role, TreeUpdate};

use crate::{apply_look, default_input, finish, look::ThemeColors};

fn rgb_line(name: &str, rgb: [u8; 3]) -> String {
    format!("{name}=#{r:02x}{g:02x}{b:02x}", name = name, r = rgb[0], g = rgb[1], b = rgb[2])
}

/// Theme colors and UI scale baked into the snapshot so color changes fail the diff.
pub fn snapshot_header(theme_id: &str, scale: f32, colors: ThemeColors) -> String {
    let mut out = format!("theme={theme_id} scale={scale:.2}\n");
    out.push_str(&rgb_line("background", colors.background));
    out.push('\n');
    out.push_str(&rgb_line("text", colors.text));
    out.push('\n');
    out.push_str(&rgb_line("accent", colors.accent));
    out.push('\n');
    out
}

/// Run one panel body and return the header plus a stable AccessKit tree dump.
pub fn panel_accesskit_snapshot(
    theme_id: &str,
    scale: f32,
    body_px: f32,
    colors: ThemeColors,
    mut body: impl FnMut(&mut egui::Ui),
) -> String {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    apply_look(&ctx, colors, scale, body_px, 0.0, false);
    let output = finish(ctx.run_ui(default_input(Vec::new()), |ui| body(ui)));
    let mut text = snapshot_header(theme_id, scale, colors);
    if let Some(update) = output.accesskit_update.as_ref() {
        text.push_str(&accesskit_tree_text(update));
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

pub fn accesskit_tree_text(update: &TreeUpdate) -> String {
    let Some(tree) = update.tree.as_ref() else {
        return String::new();
    };
    let root = tree.root;
    let nodes: HashMap<NodeId, Node> =
        update.nodes.iter().map(|(id, node)| (*id, node.clone())).collect();
    let mut out = String::new();
    write_node(&mut out, 0, root, &nodes);
    out
}

fn write_node(out: &mut String, depth: usize, id: NodeId, nodes: &HashMap<NodeId, Node>) {
    let Some(node) = nodes.get(&id) else {
        return;
    };
    if node.is_hidden() {
        return;
    }
    let indent = "  ".repeat(depth);
    let role = role_name(node.role());
    let label = node.label().or_else(|| node.value()).unwrap_or("").replace('\n', "\\n");
    let _ = std::fmt::Write::write_fmt(out, format_args!("{indent}{role} \"{label}\"\n"));
    let mut children: Vec<NodeId> = node.children().to_vec();
    children.sort_by_key(stable_node_key);
    for child in children {
        write_node(out, depth + 1, child, nodes);
    }
}

fn stable_node_key(id: &NodeId) -> String {
    format!("{id:?}")
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Button => "button",
        Role::CheckBox => "checkbox",
        Role::Label => "label",
        Role::Link => "link",
        Role::List => "list",
        Role::ListItem => "listitem",
        Role::TextInput => "textbox",
        Role::GenericContainer => "container",
        Role::Pane => "pane",
        Role::Window => "window",
        Role::Tree => "tree",
        Role::TreeItem => "treeitem",
        _ => "node",
    }
}
