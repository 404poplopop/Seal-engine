//! How an HDR game window opens. The numbers match the ordinary game window.

use crate::window_game::{GameWindow, MODE_FULLSCREEN, MODE_WINDOWED};

pub use seal_render::hdr_output::{
    choose_runner, clamp_paper_white_nits, displayed_channel, displayed_rgb, first_hdr,
    output_space, sdr_gain, HdrBuffer, OutputSpace, RunnerChoice, EXTENDED_LINEAR_WHITE_NITS,
    REFERENCE_WHITE_NITS,
};

/// Size, mode and sync for the game window, read the same way as the ordinary path.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowPlan {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub decorations: bool,
    pub fullscreen: bool,
    pub always_on_top: bool,
    pub vsync: bool,
    pub target_fps: u32,
    pub paper_white_nits: f32,
}

pub fn window_plan(window: &GameWindow) -> WindowPlan {
    WindowPlan {
        title: if window.title.is_empty() { "Game".into() } else { window.title.clone() },
        width: window.width,
        height: window.height,
        resizable: window.resizable,
        decorations: window.mode == MODE_WINDOWED,
        fullscreen: window.mode == MODE_FULLSCREEN,
        always_on_top: window.always_on_top,
        vsync: window.vsync,
        target_fps: window.target_fps,
        paper_white_nits: window.paper_white_nits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> GameWindow {
        GameWindow::default()
    }

    #[test]
    fn windowed_fullscreen_and_borderless_match_the_ordinary_window() {
        let mut game = window();
        game.title = String::new();
        game.mode = MODE_WINDOWED.into();
        game.always_on_top = true;
        game.vsync = false;
        game.resizable = false;
        let plan = window_plan(&game);
        assert_eq!(plan.title, "Game");
        assert!(plan.decorations);
        assert!(!plan.fullscreen);
        assert!(plan.always_on_top);
        assert!(!plan.vsync);
        assert!(!plan.resizable);

        game.mode = MODE_FULLSCREEN.into();
        game.title = "Night".into();
        let full = window_plan(&game);
        assert_eq!(full.title, "Night");
        assert!(full.fullscreen);
        assert!(!full.decorations);

        game.mode = "borderless".into();
        let borderless = window_plan(&game);
        assert!(!borderless.fullscreen);
        assert!(!borderless.decorations);
    }

    #[test]
    fn a_damaged_mode_still_opens_a_window() {
        let mut game = window();
        game.mode = "sideways".into();
        let plan = window_plan(&game);
        assert!(!plan.fullscreen);
        assert!(!plan.decorations);
        assert_eq!(plan.width, 1280);
        assert_eq!(plan.height, 720);
    }
}
