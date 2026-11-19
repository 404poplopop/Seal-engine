//! A running game loads a picture through the shared decoder and keeps the
//! sampler flags on the texture.

use std::fs;

use seal_app::render::picture::PictureSampling;
use seal_app::render::{ColourSpace, Device, SoftDevice, TextureWrap};
use seal_app::{AppConfig, Ctx, Game, Runner};

struct Load {
    path: std::path::PathBuf,
}

impl Game for Load {
    fn start(&mut self, cx: &mut Ctx) {
        let sampling = PictureSampling {
            wrap: TextureWrap::Repeat,
            colour_space: ColourSpace::Linear,
            mipmaps: true,
            ..Default::default()
        };
        cx.load_picture("pic", &self.path, sampling).unwrap();
    }

    fn update(&mut self, _cx: &mut Ctx) {}
}

#[test]
fn load_picture_puts_wrap_colour_and_mipmaps_on_the_texture() {
    let dir = std::env::temp_dir().join(format!("seal-picture-load-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("strip.tga");
    // 2×1 true-colour TGA: red then green.
    let tga = [0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 24, 0, 0, 0, 255, 0, 255, 0];
    fs::write(&path, tga).unwrap();
    let mut runner = Runner::new(
        Load { path },
        SoftDevice::new(),
        AppConfig { title: "picture".into(), width: 8, height: 8, input: String::new() },
        None,
        seal_app::render_config::GameRenderConfig::default(),
    )
    .unwrap();
    runner.render();
    let id = runner.renderer.image("pic").unwrap();
    let desc = runner.device.desc(id);
    assert_eq!((desc.width, desc.height), (2, 1));
    assert_eq!(desc.wrap, TextureWrap::Repeat);
    assert_eq!(desc.colour_space, ColourSpace::Linear);
    assert!(desc.mipmaps);
    let _ = fs::remove_dir_all(&dir);
}
