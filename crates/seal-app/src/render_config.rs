//! Rendering quality read from `render.txt` beside a packaged game.

pub const RENDER_FILE: &str = "render.txt";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameRenderConfig {
    pub texture_max_size: u32,
    pub glow: bool,
    /// Samples per pixel requested (0 or 1 = off; the GPU path uses 4 for any higher value).
    pub msaa: u32,
    /// Anisotropic filtering for linear pictures, 1-16.
    pub anisotropy: u16,
    /// Percent of the game resolution the frame is drawn at; the window shows it scaled.
    pub render_scale: u32,
}

impl Default for GameRenderConfig {
    fn default() -> Self {
        Self { texture_max_size: 4096, glow: true, msaa: 0, anisotropy: 1, render_scale: 100 }
    }
}

pub fn load_next_to_game(game_dir: &std::path::Path) -> GameRenderConfig {
    let path = game_dir.join(RENDER_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return GameRenderConfig::default();
    };
    parse_render_text(&text).unwrap_or_default()
}

fn parse_render_text(text: &str) -> Result<GameRenderConfig, String> {
    let mut texture_max_size = 4096u32;
    let mut glow = true;
    let mut msaa = 0u32;
    let mut anisotropy = 1u32;
    let mut render_scale = 100u32;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once(' ') {
            Some((key, value)) => (key.trim(), value.trim()),
            None => (line, ""),
        };
        match key {
            "texture_max_size" => texture_max_size = parse_u32(value, 0, 8192)?,
            "glow" => glow = matches!(value, "true"),
            "msaa" => msaa = parse_u32(value, 0, 16)?,
            "anisotropy" => anisotropy = parse_u32(value, 1, 16)?,
            "render_scale" => render_scale = parse_u32(value, 25, 200)?,
            _ => {}
        }
    }
    Ok(GameRenderConfig {
        texture_max_size,
        glow,
        msaa,
        anisotropy: anisotropy as u16,
        render_scale,
    })
}

fn parse_u32(text: &str, min: u32, max: u32) -> Result<u32, String> {
    let number: u32 = text.parse().map_err(|_| format!("{text} is not a whole number"))?;
    if !(min..=max).contains(&number) {
        return Err(format!("{text} must be between {min} and {max}"));
    }
    Ok(number)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_keys_are_read_and_old_files_keep_defaults() {
        let cfg =
            parse_render_text("level high\nmsaa 4\nanisotropy 8\nrender_scale 50\nglow false\n")
                .unwrap();
        assert_eq!((cfg.msaa, cfg.anisotropy, cfg.render_scale, cfg.glow), (4, 8, 50, false));
        let old = parse_render_text("texture_max_size 2048\n").unwrap();
        assert_eq!((old.msaa, old.anisotropy, old.render_scale), (0, 1, 100));
        assert!(parse_render_text("render_scale 5\n").is_err());
    }
}
