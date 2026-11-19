//! Seal Hop end to end: the bot finishes the level, and a recorded run
//! replays to the same world, frames and sound.

use seal_app::fnv;
use seal_app::input::{Button, InputRecording, RawInput};
use seal_app::render::SoftDevice;
use seal_app::Runner;
use seal_sample_2d::{config, Autopilot, SealHop, State, DEFAULT_LEVEL};

fn runner() -> Runner<SealHop, SoftDevice> {
    Runner::new(
        SealHop::new(DEFAULT_LEVEL),
        SoftDevice::new(),
        config(),
        None,
        seal_app::render_config::GameRenderConfig::default(),
    )
    .unwrap()
}

/// Play with the bot, recording input. Returns the recording.
fn bot_run(r: &mut Runner<SealHop, SoftDevice>, frames: usize) -> InputRecording {
    let mut bot = Autopilot::default();
    let mut rec = InputRecording::new();
    for _ in 0..frames {
        let raw = bot.input(&r.game, &r.cx);
        rec.push(&raw);
        r.step(&raw).unwrap();
    }
    rec
}

fn world_text(r: &mut Runner<SealHop, SoftDevice>) -> String {
    r.cx.world.save_scene(None).text
}

#[test]
fn the_bot_finishes_the_level() {
    let mut r = runner();
    bot_run(&mut r, 1200);
    let g = &r.game;
    assert_eq!(g.coins, g.coins_total, "every coin collected");
    assert_eq!(g.coins_total, 10);
    assert_eq!(g.falls, 0);
    assert!(matches!(g.state, State::Won { .. }), "reached the flag: {:?}", g.state);
}

#[test]
fn a_recorded_run_replays_exactly() {
    let mut a = runner();
    let rec = bot_run(&mut a, 1000);
    let text = rec.to_text();
    let rec = InputRecording::parse(&text).unwrap();
    assert_eq!(rec.to_text(), text);
    let mut b = runner();
    for i in 0..rec.len() {
        b.step(rec.frame(i).unwrap()).unwrap();
    }
    assert_eq!(world_text(&mut a), world_text(&mut b));
    assert_eq!(a.game.state, b.game.state);
    assert_eq!(a.audio, b.audio);
    a.render();
    b.render();
    assert_eq!(a.read_frame(), b.read_frame());
}

// The world text names the scene_settings resource since 0.2. That resource now carries
// the music file, and each sprite carries its order and mask interaction.
// The run itself is unchanged (same frame and audio).
const GOLDEN_WORLD: u64 = 0xafc0_57e9_0554_cf21;
const GOLDEN_FRAME: u64 = 0xeacd_2ee6_be00_640d;
const GOLDEN_AUDIO: u64 = 0x9485_43d6_18bd_38f6;

#[test]
fn golden_run_matches_on_every_machine() {
    let mut r = runner();
    bot_run(&mut r, 1000);
    let world = fnv(world_text(&mut r).as_bytes());
    r.render();
    let frame = fnv(&r.read_frame());
    let audio: Vec<u8> = r.audio.iter().flat_map(|s| s.to_bits().to_le_bytes()).collect();
    let audio = fnv(&audio);
    println!("world {world:#018x} frame {frame:#018x} audio {audio:#018x}");
    assert_eq!(r.audio.len(), 1000 * 800 * 2, "800 stereo frames per step");
    assert!(r.audio.iter().any(|s| s.abs() > 0.05), "jumps and coins made sound");
    assert_eq!(world, GOLDEN_WORLD, "world");
    assert_eq!(frame, GOLDEN_FRAME, "frame");
    assert_eq!(audio, GOLDEN_AUDIO, "audio");
}

#[test]
fn falling_into_a_gap_respawns_at_the_start() {
    let mut r = runner();
    let start = r.game.start;
    let mut raw = RawInput::default();
    raw.press(Button::key("d"));
    // Run right along the ground with no jumps: the first gap swallows the seal.
    for _ in 0..400 {
        r.step(&raw).unwrap();
        if r.game.falls > 0 {
            break;
        }
    }
    assert_eq!(r.game.falls, 1);
    let t = r.cx.world.get::<seal_app::ecs::core::Transform>(r.game.player).unwrap().translation;
    assert!((t[0] - start[0]).abs() < 0.2, "back at the start: {t:?}");
}

#[test]
fn restart_rebuilds_the_level() {
    let mut r = runner();
    bot_run(&mut r, 300);
    assert!(r.game.coins > 0);
    let mut raw = RawInput::default();
    raw.press(Button::key("r"));
    r.step(&raw).unwrap();
    assert_eq!(r.game.coins, 0);
    assert_eq!(r.game.coin_list.iter().filter(|e| r.cx.world.contains(**e)).count(), 10);
    assert_eq!(r.game.state, State::Playing);
}

#[test]
fn players_can_rebind_controls() {
    let map = "action jump = key.k\naction restart = key.r\naction quit = key.escape\naxis move = key.j key.l\n";
    let mut r =
        Runner::new(
            SealHop::new(DEFAULT_LEVEL),
            SoftDevice::new(),
            config(),
            Some(map),
            seal_app::render_config::GameRenderConfig::default(),
        )
        .unwrap();
    let x0 =
        r.cx.world.get::<seal_app::ecs::core::Transform>(r.game.player).unwrap().translation[0];
    let mut raw = RawInput::default();
    raw.press(Button::key("l"));
    for _ in 0..30 {
        r.step(&raw).unwrap();
    }
    let x1 =
        r.cx.world.get::<seal_app::ecs::core::Transform>(r.game.player).unwrap().translation[0];
    assert!(x1 > x0 + 1.0, "moved right with the new key: {x0} -> {x1}");
    let mut old = RawInput::default();
    old.press(Button::key("d"));
    for _ in 0..30 {
        r.step(&old).unwrap();
    }
    let x2 =
        r.cx.world.get::<seal_app::ecs::core::Transform>(r.game.player).unwrap().translation[0];
    assert!((x2 - x1).abs() < 0.3, "the old key does nothing");
}

#[test]
fn demo_mode_plays_by_itself() {
    let mut r = runner();
    r.game.demo = Some(Autopilot::default());
    let idle = RawInput::default();
    for _ in 0..1200 {
        r.step(&idle).unwrap();
    }
    assert!(matches!(r.game.state, State::Won { .. }));
}
