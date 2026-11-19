//! GPU upload helpers for array, volume and cubemap textures.

use seal_render::{TextureDesc, TextureShape};

pub fn wgpu_dimension(desc: TextureDesc) -> wgpu::TextureDimension {
    match desc.shape {
        TextureShape::D2 | TextureShape::D2Array { .. } | TextureShape::Cube => {
            wgpu::TextureDimension::D2
        }
        TextureShape::D3 { .. } => wgpu::TextureDimension::D3,
    }
}

pub fn wgpu_view_dimension(desc: TextureDesc) -> wgpu::TextureViewDimension {
    match desc.shape {
        TextureShape::D2 => wgpu::TextureViewDimension::D2,
        TextureShape::D2Array { .. } => wgpu::TextureViewDimension::D2Array,
        TextureShape::D3 { .. } => wgpu::TextureViewDimension::D3,
        TextureShape::Cube => wgpu::TextureViewDimension::Cube,
    }
}

pub fn wgpu_extent(desc: TextureDesc) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: desc.width.max(1),
        height: desc.height.max(1),
        depth_or_array_layers: desc.depth_or_layers(),
    }
}

pub fn adapter_feature_names(features: wgpu::Features) -> Vec<&'static str> {
    let mut out = Vec::new();
    if features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC) {
        out.push("texture_compression_bc");
    }
    if features.contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2) {
        out.push("texture_compression_etc2");
    }
    if features.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC) {
        out.push("texture_compression_astc");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use seal_render::TextureDesc;

    #[test]
    fn array_extent_layers() {
        let d = TextureDesc::array_rgba8(8, 8, 4);
        assert_eq!(wgpu_extent(d).depth_or_array_layers, 4);
        assert!(matches!(wgpu_view_dimension(d), wgpu::TextureViewDimension::D2Array));
    }

    #[test]
    fn volume_dimension_d3() {
        let d = TextureDesc::volume_rgba8(4, 4, 8);
        assert_eq!(wgpu_dimension(d), wgpu::TextureDimension::D3);
        assert_eq!(wgpu_extent(d).depth_or_array_layers, 8);
    }

    #[test]
    fn cubemap_six_faces() {
        let d = TextureDesc::cubemap_rgba8(16);
        assert_eq!(wgpu_extent(d).depth_or_array_layers, 6);
        assert!(matches!(wgpu_view_dimension(d), wgpu::TextureViewDimension::Cube));
    }
}
