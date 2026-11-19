//! Pick a window surface format when HDR output is requested.

use wgpu::TextureFormat;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfacePick {
    pub format: TextureFormat,
    pub hdr: bool,
}

pub fn pick_surface_format(formats: &[TextureFormat], want_hdr: bool) -> SurfacePick {
    if want_hdr {
        let offered: Vec<seal_render::hdr_output::HdrBuffer> =
            formats.iter().copied().filter_map(hdr_buffer).collect();
        if let Some(buffer) = seal_render::hdr_output::first_hdr(&offered) {
            return SurfacePick { format: texture_format(buffer), hdr: true };
        }
    }
    SurfacePick {
        format: super::present::pick_format(formats),
        hdr: false,
    }
}

fn hdr_buffer(format: TextureFormat) -> Option<seal_render::hdr_output::HdrBuffer> {
    match format {
        TextureFormat::Rgba16Float => Some(seal_render::hdr_output::HdrBuffer::Rgba16Float),
        TextureFormat::Rgb10a2Unorm => Some(seal_render::hdr_output::HdrBuffer::Rgb10a2),
        _ => None,
    }
}

fn texture_format(buffer: seal_render::hdr_output::HdrBuffer) -> TextureFormat {
    match buffer {
        seal_render::hdr_output::HdrBuffer::Rgba16Float => TextureFormat::Rgba16Float,
        seal_render::hdr_output::HdrBuffer::Rgb10a2 => TextureFormat::Rgb10a2Unorm,
    }
}

pub fn clamp_paper_white_nits(value: f32) -> f32 {
    seal_render::hdr_output::clamp_paper_white_nits(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdr_picks_first_offered_hdr_format() {
        let formats = [
            TextureFormat::Bgra8UnormSrgb,
            TextureFormat::Rgba16Float,
        ];
        let pick = pick_surface_format(&formats, true);
        assert!(pick.hdr);
        assert_eq!(pick.format, TextureFormat::Rgba16Float);
    }

    #[test]
    fn hdr_falls_back_to_sdr_when_none_offered() {
        let formats = [TextureFormat::Bgra8UnormSrgb, TextureFormat::Rgba8Unorm];
        let pick = pick_surface_format(&formats, true);
        assert!(!pick.hdr);
        assert_eq!(pick.format, TextureFormat::Rgba8Unorm);
    }

    #[test]
    fn sdr_ignores_hdr_formats() {
        let formats = [TextureFormat::Bgra8UnormSrgb, TextureFormat::Rgba8Unorm];
        let pick = pick_surface_format(&formats, false);
        assert!(!pick.hdr);
        assert_eq!(pick.format, TextureFormat::Rgba8Unorm);
    }
}