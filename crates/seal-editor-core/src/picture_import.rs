//! Load a picture through the shared decoder and apply its import sidecar.

use std::path::Path;

use crate::import_presets::{picture_look, PictureLook};

/// Picture size from the file header (fast; no decode).
pub fn file_dimensions(path: &Path) -> Option<(u32, u32)> {
    seal_render::picture::file_dimensions(path).ok()
}

/// Pixels after Max Size, plus the import look and whether the file is linear light.
pub struct ImportedPicture {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub linear_file: bool,
    pub look: PictureLook,
}

pub fn load(path: &Path) -> Result<ImportedPicture, String> {
    let look = picture_look(path);
    let picture =
        seal_render::picture::decode_path_with(path, look.svg_size).map_err(|e| e.to_string())?;
    let (rgba, width, height) = seal_render::picture::downscale(
        &picture.rgba,
        picture.width,
        picture.height,
        look.max_size,
    );
    Ok(ImportedPicture { rgba, width, height, linear_file: picture.linear_file, look })
}

/// Whether drawing should encode the bytes as linear light.
///
/// A sidecar that names a colour space wins. With no colour space stored, a
/// Radiance file is linear and every other picture stays on today's path.
pub fn displays_linear(look: &PictureLook, linear_file: bool) -> bool {
    if look.colour_explicit {
        look.linear_colour
    } else {
        look.linear_colour || linear_file
    }
}
