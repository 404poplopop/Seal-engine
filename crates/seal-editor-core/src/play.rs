/// Reserved for editor play. The shell ships no runtime, so every session reports unavailable.
pub trait PlaySession {
    fn unavailable_reason(&self) -> &str {
        "play-unavailable"
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoPlay;

impl PlaySession for NoPlay {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_has_no_play_session() {
        let play = NoPlay;
        assert_eq!(play.unavailable_reason(), "play-unavailable");
    }
}
