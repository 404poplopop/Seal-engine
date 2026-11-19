//! Inspector fields for a mesh base-colour texture.

use crate::Entity;

pub const KEYS: &[&str] =
    &["mesh_texture", "mesh_layer", "mesh_slice", "mesh_tile_u", "mesh_tile_v"];

fn num(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".into()
    } else {
        trimmed.into()
    }
}

pub fn rows(entity: &Entity) -> Vec<(String, String)> {
    let mat = &entity.mesh_material;
    vec![
        ("mesh_texture".into(), mat.texture.clone()),
        ("mesh_layer".into(), mat.layer.to_string()),
        ("mesh_slice".into(), num(mat.slice)),
        ("mesh_tile_u".into(), num(mat.tile[0])),
        ("mesh_tile_v".into(), num(mat.tile[1])),
    ]
}

pub fn apply(entity: &mut Entity, key: &str, value: &str) -> bool {
    let mat = &mut entity.mesh_material;
    match key {
        "mesh_texture" => mat.texture = value.trim().replace('\\', "/"),
        "mesh_layer" => {
            mat.layer = value.trim().parse::<u32>().unwrap_or(mat.layer);
        }
        "mesh_slice" => {
            if let Ok(n) = value.trim().parse::<f32>() {
                if n.is_finite() {
                    mat.slice = n.max(0.0);
                }
            }
        }
        "mesh_tile_u" => {
            if let Ok(n) = value.trim().parse::<f32>() {
                if n.is_finite() {
                    mat.tile[0] = n;
                }
            }
        }
        "mesh_tile_v" => {
            if let Ok(n) = value.trim().parse::<f32>() {
                if n.is_finite() {
                    mat.tile[1] = n;
                }
            }
        }
        _ => return false,
    }
    true
}

pub fn read_saved(map: &[(String, seal_sealtext::SealValue)]) -> seal_scene::MeshMaterial {
    let text = |key: &str| {
        map.iter().find(|(k, _)| k == key).and_then(|(_, v)| match v {
            seal_sealtext::SealValue::String(text) => Some(text.clone()),
            seal_sealtext::SealValue::Number(text) => Some(text.clone()),
            _ => None,
        })
    };
    let mut mat = seal_scene::MeshMaterial::default();
    if let Some(path) = text("mesh_texture") {
        mat.texture = path;
    }
    if let Some(layer) = text("mesh_layer") {
        mat.layer = layer.parse().unwrap_or(0);
    }
    if let Some(slice) = text("mesh_slice") {
        mat.slice = slice.parse().unwrap_or(0.0);
    }
    if let Some(u) = text("mesh_tile_u") {
        mat.tile[0] = u.parse().unwrap_or(1.0);
    }
    if let Some(v) = text("mesh_tile_v") {
        mat.tile[1] = v.parse().unwrap_or(1.0);
    }
    mat
}

pub fn copy_one(from: &Entity, to: &mut Entity, field: &str) {
    match field {
        "mesh_texture" => to.mesh_material.texture = from.mesh_material.texture.clone(),
        "mesh_layer" => to.mesh_material.layer = from.mesh_material.layer,
        "mesh_slice" => to.mesh_material.slice = from.mesh_material.slice,
        "mesh_tile_u" => to.mesh_material.tile[0] = from.mesh_material.tile[0],
        "mesh_tile_v" => to.mesh_material.tile[1] = from.mesh_material.tile[1],
        _ => {}
    }
}
