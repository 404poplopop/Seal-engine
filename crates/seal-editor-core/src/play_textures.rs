//! Picture paths to warm when play starts.

use std::path::Path;

use seal_world::sprite_shape;

/// Absolute picture paths referenced by the live scene (sprites and normal maps).
pub fn scene_sprite_pictures(world: &seal_world::World, root: Option<&Path>) -> Vec<String> {
    let mut out = Vec::new();
    for entity in &world.entities {
        if let Some(path) = sprite_shape::picture_path(root, &entity.sprite) {
            out.push(path);
        }
        if let Some(path) = sprite_shape::picture_path(root, &entity.normal_map) {
            out.push(path);
        }
    }
    for (_, live) in &world.game_screens.shown {
        for node in &live.screen.nodes {
            if node.kind == "image" && !node.image.is_empty() {
                if let Some(path) = sprite_shape::picture_path(root, &node.image) {
                    out.push(path);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}
