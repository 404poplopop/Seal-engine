//! Render frames of a scripted run to PPM files: `cargo run --example shot -- OUTDIR`.
use seal_app::input::RawInput;
use seal_app::render::SoftDevice;
use seal_app::{frame_to_ppm, Runner};
use seal_sample_2d::{config, Autopilot, SealHop, DEFAULT_LEVEL};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "shots".into());
    std::fs::create_dir_all(&out).unwrap();
    let mut r =
        Runner::new(
            SealHop::new(DEFAULT_LEVEL),
            SoftDevice::new(),
            config(),
            None,
            seal_app::render_config::GameRenderConfig::default(),
        )
        .unwrap();
    let mut bot = Autopilot::default();
    let frames: u32 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(1200);
    for f in 0..frames {
        let raw: RawInput = bot.input(&r.game, &r.cx);
        r.step(&raw).unwrap();
        if f % 120 == 0 || f + 1 == frames {
            r.render();
            let px = r.read_frame();
            std::fs::write(format!("{out}/f{f:04}.ppm"), frame_to_ppm(640, 360, &px)).unwrap();
        }
    }
    let g = &r.game;
    let pos =
        r.cx.world.get::<seal_app::ecs::core::Transform>(r.game.player).map(|t| t.translation);
    println!("frame {} at {pos:?}", r.cx.frame);
    println!(
        " {}/{} falls {} jumps {} state {:?}",
        g.coins, g.coins_total, g.falls, g.jumps, g.state
    );
}
