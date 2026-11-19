//! Rasterise an SVG picture to straight RGBA8.

use crate::picture::{Picture, PictureError, MAX_SIDE};

/// Rasterise SVG bytes so the longer side is `longest`. `0` uses 256.
pub(crate) fn raster(bytes: &[u8], longest: u32) -> Result<Picture, PictureError> {
    if bytes.is_empty() {
        return Err(PictureError::new("the picture is empty"));
    }
    let opt = resvg::usvg::Options {
        shape_rendering: resvg::usvg::ShapeRendering::CrispEdges,
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(bytes, &opt)
        .map_err(|err| PictureError::new(err.to_string()))?;
    let (vw, vh) = (tree.size().width(), tree.size().height());
    if !(vw.is_finite() && vh.is_finite()) || vw <= 0.0 || vh <= 0.0 {
        return Err(PictureError::new("the picture size is not usable"));
    }
    let side = if longest == 0 { 256 } else { longest.clamp(1, MAX_SIDE) };
    let scale = side as f32 / vw.max(vh);
    let width = (vw * scale).round().clamp(1.0, MAX_SIDE as f32) as u32;
    let height = (vh * scale).round().clamp(1.0, MAX_SIDE as f32) as u32;
    let mut pixmap = tiny_pixmap(width, height)?;
    let mut view = pixmap.as_mut();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(width as f32 / vw, height as f32 / vh),
        &mut view,
    );
    let rgba = straight(pixmap.data());
    let pixels = (width as usize).saturating_mul(height as usize);
    if rgba.len() != pixels * 4 {
        return Err(PictureError::new("the picture size is not usable"));
    }
    Ok(Picture { width, height, rgba, linear: None, linear_file: false, mesh: None })
}

fn tiny_pixmap(width: u32, height: u32) -> Result<resvg::tiny_skia::Pixmap, PictureError> {
    resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| PictureError::new("the picture size is not usable"))
}

fn straight(data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    for pixel in out.as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        if alpha == 0 {
            pixel[0] = 0;
            pixel[1] = 0;
            pixel[2] = 0;
        } else if alpha < 255 {
            pixel[0] = ((u16::from(pixel[0]) * 255) / alpha).min(255) as u8;
            pixel[1] = ((u16::from(pixel[1]) * 255) / alpha).min(255) as u8;
            pixel[2] = ((u16::from(pixel[2]) * 255) / alpha).min(255) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_red_rectangle_rasterises_again_when_the_size_changes() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="2" viewBox="0 0 4 2"><rect width="4" height="2" fill="#ff0000"/></svg>"##;
        let picture = raster(svg, 4).unwrap();
        assert_eq!((picture.width, picture.height), (4, 2));
        assert!(picture.rgba.chunks(4).all(|pixel| pixel == [255, 0, 0, 255]));
        let bigger = raster(svg, 8).unwrap();
        assert_eq!((bigger.width, bigger.height), (8, 4));
        assert!(bigger.rgba.chunks(4).all(|pixel| pixel == [255, 0, 0, 255]));
        let damaged = std::panic::catch_unwind(|| raster(b"<svg", 32));
        assert!(damaged.unwrap().is_err());
    }
}
