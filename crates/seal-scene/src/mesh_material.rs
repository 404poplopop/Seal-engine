//! Base colour texture on a mesh. Absent on old scenes, so they stay as they were.

use seal_ecs::text_component;

/// Picture or `.texture` asset sampled on a mesh, multiplied by the object colour.
/// An empty texture keeps the flat colour. `layer` picks an array layer or a cubemap
/// face. `slice` picks a volume slice. `tile` repeats the texture across the mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshMaterial {
    pub texture: String,
    pub layer: u32,
    pub slice: f32,
    pub tile: [f32; 2],
}

impl Default for MeshMaterial {
    fn default() -> Self {
        Self { texture: String::new(), layer: 0, slice: 0.0, tile: [1.0, 1.0] }
    }
}

text_component!(MeshMaterial as "mesh_material" { texture, layer, slice, tile });
