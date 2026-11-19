//! The config a runner builds from export window text, and the frame cap.

use std::time::Duration;

use seal_app::window_config::{self, WindowMode};

const TEXT: &str = "\
title Hop
icon sprites/icon.png
mode fullscreen
width 800
height 600
resizable false
always_on_top true
vsync false
fps 30
";

#[test]
fn window_text_becomes_the_runner_config() {
    let config = window_config::parse(TEXT).unwrap();
    assert_eq!(config.title, "Hop");
    assert_eq!(config.icon, "sprites/icon.png");
    assert_eq!(config.mode, WindowMode::Fullscreen);
    assert_eq!((config.width, config.height), (800, 600));
    assert!(!config.resizable);
    assert!(config.always_on_top);
    assert!(!config.vsync);
    assert_eq!(config.target_fps, 30);
    assert!(window_config::parse("mode banana\n").is_err());
    assert!(window_config::parse("width 2\n").is_err());
    assert!(window_config::parse("fps 5000\n").is_err());
    assert!(window_config::parse("glow 1\n").is_err());
    assert!(window_config::parse("").is_err());
    let empty_icon = window_config::parse("title Hop\nicon\nmode windowed\n").unwrap();
    assert_eq!(empty_icon.icon, "");
    assert_eq!(empty_icon.title, "Hop");
}

#[test]
fn the_frame_limiter_waits_out_the_budget() {
    let frame = Duration::from_secs(1) / 60;
    assert_eq!(window_config::frame_wait(0, Duration::from_millis(5)), Duration::ZERO);
    assert_eq!(window_config::frame_wait(60, Duration::ZERO), frame);
    assert_eq!(window_config::frame_wait(60, frame / 2), frame - frame / 2);
    assert_eq!(window_config::frame_wait(60, frame), Duration::ZERO);
    assert_eq!(window_config::frame_wait(60, frame + Duration::from_millis(1)), Duration::ZERO);
    assert_eq!(window_config::frame_wait(30, Duration::from_millis(100)), Duration::ZERO);
}
