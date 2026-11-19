//! Cubemap drawn behind the perspective view. Absent on old scenes.

use seal_ecs::text_component;

/// Project-relative `.texture` cubemap. Empty draws no sky image.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkyCubemap {
    pub texture: String,
}

text_component!(SkyCubemap as "sky_cubemap" { texture });
