//! Wrap, colour space and mipmaps change the software sampler. The descriptor
//! stores the same flags the sampler uses.

use seal_render::picture::PictureSampling;
use seal_render::{
    BlendMode, ColourSpace, Device, Instance, QuadBatch, SoftDevice, TextureDesc, TextureFilter,
    TextureWrap,
};

fn draw(
    rgba: &[u8],
    width: u32,
    height: u32,
    sampling: PictureSampling,
    target_w: u32,
    target_h: u32,
    uv: [f32; 4],
) -> (Vec<u8>, TextureDesc) {
    let mut device = SoftDevice::new();
    let mut desc = TextureDesc::rgba8(width, height);
    sampling.apply(&mut desc);
    let texture = device.create_texture(desc);
    let stored = device.desc(texture);
    device.upload(texture, rgba);
    let target = device.create_texture(TextureDesc::rgba8(target_w, target_h));
    device.clear(target, [0.0, 0.0, 0.0, 1.0]);
    let (tw, th) = (target_w as f32, target_h as f32);
    device.draw_quads(
        target,
        &QuadBatch {
            texture: Some(texture),
            blend: BlendMode::Opaque,
            instances: vec![Instance {
                corners: [[0.0, 0.0], [tw, 0.0], [tw, th], [0.0, th]],
                uv,
                color: [1.0, 1.0, 1.0, 1.0],
                corner_uv: None,
            }],
        },
    );
    (device.read_back(target), stored)
}

fn pixel(frame: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let i = (y * width + x) * 4;
    [frame[i], frame[i + 1], frame[i + 2], frame[i + 3]]
}

#[test]
fn wrap_repeat_and_clamp_reach_the_sampler() {
    let rgba = [255, 0, 0, 255, 0, 0, 255, 255];
    let repeat = PictureSampling { wrap: TextureWrap::Repeat, ..Default::default() };
    let clamp = PictureSampling { wrap: TextureWrap::Clamp, ..Default::default() };
    let (rep, rep_desc) = draw(&rgba, 2, 1, repeat, 8, 1, [0.0, 0.0, 2.0, 1.0]);
    let (clamped, clamp_desc) = draw(&rgba, 2, 1, clamp, 8, 1, [0.0, 0.0, 2.0, 1.0]);
    assert_eq!(rep_desc.wrap, TextureWrap::Repeat);
    assert_eq!(clamp_desc.wrap, TextureWrap::Clamp);
    // u ≈ 1.125 lands one texel past the picture: repeat comes back to red, clamp stays on blue.
    assert_eq!(pixel(&rep, 8, 4, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(&clamped, 8, 4, 0), [0, 0, 255, 255]);
}

#[test]
fn linear_colour_reaches_the_sampler_and_srgb_stays_the_byte() {
    let rgba = [128, 128, 128, 255];
    let linear = PictureSampling { colour_space: ColourSpace::Linear, ..Default::default() };
    let srgb = PictureSampling { colour_space: ColourSpace::Srgb, ..Default::default() };
    let (lin, lin_desc) = draw(&rgba, 1, 1, linear, 2, 2, [0.0, 0.0, 1.0, 1.0]);
    let (plain, plain_desc) = draw(&rgba, 1, 1, srgb, 2, 2, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(lin_desc.colour_space, ColourSpace::Linear);
    assert_eq!(plain_desc.colour_space, ColourSpace::Srgb);
    assert_eq!(pixel(&plain, 2, 0, 0), [128, 128, 128, 255]);
    assert_eq!(pixel(&lin, 2, 0, 0), [181, 181, 181, 255]);
}

#[test]
fn mipmaps_change_a_minified_draw_and_the_descriptor() {
    let rgba = [255, 0, 0, 255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 0, 255, 255];
    let on = PictureSampling { mipmaps: true, ..Default::default() };
    let off = PictureSampling { mipmaps: false, ..Default::default() };
    let (full, full_desc) = draw(&rgba, 4, 1, on, 4, 1, [0.0, 0.0, 1.0, 1.0]);
    let (small, small_desc) = draw(&rgba, 4, 1, on, 2, 2, [0.0, 0.0, 1.0, 1.0]);
    let (sharp, _) = draw(&rgba, 4, 1, off, 2, 2, [0.0, 0.0, 1.0, 1.0]);
    assert!(full_desc.mipmaps);
    assert!(small_desc.mipmaps);
    assert_eq!(full_desc.filter, TextureFilter::Nearest);
    assert_eq!(pixel(&full, 4, 0, 0), [255, 0, 0, 255]);
    assert_ne!(small, sharp);
    let mixed = pixel(&small, 2, 0, 0);
    assert!((mixed[0] as i32 - 127).abs() <= 1, "{mixed:?}");
    assert!((mixed[2] as i32 - 127).abs() <= 1, "{mixed:?}");
}
