//! Buffer visualization modes for the 3D viewport (base colour, normals, depth, overdraw).

/// Modes under **Buffer views** in the viewport shade menu.
pub const BUFFER_SHADE_MODES: &[&str] = &["base_colour", "normals", "depth", "overdraw"];

pub fn is_buffer_mode(shade: &str) -> bool {
    BUFFER_SHADE_MODES.contains(&shade)
}

/// Returns the WGSL-oriented debug channel name used when the GPU path is active.
pub fn gpu_debug_channel(shade: &str) -> Option<&'static str> {
    match shade {
        "base_colour" => Some("albedo"),
        "normals" => Some("normal"),
        "depth" => Some("depth"),
        "overdraw" => Some("overdraw"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewport_selection;

    #[test]
    fn buffer_mode_selection_reaches_gpu_channel() {
        assert_eq!(gpu_debug_channel("normals"), Some("normal"));
        assert_eq!(gpu_debug_channel("depth"), Some("depth"));
        assert!(viewport_selection::shade_mode_allowed("base_colour"));
    }
}
