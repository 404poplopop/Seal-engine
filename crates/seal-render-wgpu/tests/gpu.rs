//! The GPU device must draw what the software device draws.
//! Skips (with a note) on machines without any GPU or software rasteriser.

use seal_render::graph::TexturePool;
use seal_render::*;
use seal_render_wgpu::WgpuDevice;

fn gpu() -> Option<WgpuDevice> {
    match WgpuDevice::headless() {
        Ok(d) => {
            eprintln!("GPU: {}", d.adapter_name);
            Some(d)
        }
        Err(e) => {
            eprintln!("skipping GPU test: {e}");
            None
        }
    }
}

fn quad(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Instance {
    Instance {
        corners: [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
        uv: [0.0, 0.0, 1.0, 1.0],
        color,
        corner_uv: None,
    }
}

/// A frame that uses every feature: clear, opaque, alpha, additive, a texture,
/// a frame-local texture and a copy, all through the render graph.
fn frame<D: Device>(device: &mut D) -> Vec<u8> {
    let screen = device.create_texture(TextureDesc::rgba8(32, 24));
    let checker = device.create_texture(TextureDesc::rgba8(4, 4));
    let mut texels = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            texels.extend_from_slice(if (x + y) % 2 == 0 {
                &[250, 40, 10, 255]
            } else {
                &[10, 60, 240, 128]
            });
        }
    }
    device.upload(checker, &texels);
    let mut graph = RenderGraph::<D>::new();
    let out = graph.import("screen", screen);
    let scratch = graph.create_texture("scratch", TextureDesc::rgba8(32, 24));
    graph.add_pass(
        "world",
        |b| {
            b.write(scratch);
        },
        move |ctx| {
            let t = ctx.texture(scratch);
            ctx.device.clear(t, [0.1, 0.2, 0.3, 1.0]);
            ctx.device.draw_quads(
                t,
                &QuadBatch {
                    texture: None,
                    blend: BlendMode::Opaque,
                    instances: vec![quad(2.0, 2.0, 10.0, 6.0, [0.9, 0.9, 0.1, 1.0])],
                },
            );
            ctx.device.draw_quads(
                t,
                &QuadBatch {
                    texture: Some(checker),
                    blend: BlendMode::Alpha,
                    instances: vec![quad(8.0, 4.0, 4.0, 4.0, [1.0; 4])],
                },
            );
            ctx.device.draw_quads(
                t,
                &QuadBatch {
                    texture: None,
                    blend: BlendMode::Alpha,
                    instances: vec![quad(6.0, 6.0, 12.0, 10.0, [0.2, 1.0, 0.4, 0.5])],
                },
            );
            ctx.device.draw_quads(
                t,
                &QuadBatch {
                    texture: None,
                    blend: BlendMode::Additive,
                    instances: vec![quad(14.0, 10.0, 16.0, 12.0, [0.5, 0.25, 0.75, 0.5])],
                },
            );
        },
    );
    graph.add_pass(
        "present",
        |b| {
            b.read(scratch).write(out);
        },
        move |ctx| {
            let (s, o) = (ctx.texture(scratch), ctx.texture(out));
            ctx.device.copy(s, o);
        },
    );
    let mut pool = TexturePool::default();
    graph.execute(device, &mut pool).unwrap();
    pool.release(device);
    device.read_back(screen)
}

#[test]
fn gpu_matches_software_device() {
    let Some(mut gpu) = gpu() else { return };
    let soft = frame(&mut SoftDevice::new());
    let hard = frame(&mut gpu);
    assert_eq!(soft.len(), hard.len());
    let mut worst = 0;
    for (i, (a, b)) in soft.iter().zip(&hard).enumerate() {
        let d = a.abs_diff(*b);
        assert!(d <= 1, "byte {i} (pixel {}, {}): soft {a} gpu {b}", (i / 4) % 32, i / 128);
        worst = worst.max(d);
    }
    eprintln!("worst channel difference: {worst}");
}

#[test]
fn textures_are_recycled() {
    let Some(mut gpu) = gpu() else { return };
    let a = gpu.create_texture(TextureDesc::rgba8(8, 8));
    gpu.destroy_texture(a);
    let b = gpu.create_texture(TextureDesc::rgba8(16, 16));
    assert_eq!(a, b, "a freed slot is reused");
    assert_eq!(gpu.live_textures(), 1);
    assert_eq!(gpu.desc(b), TextureDesc::rgba8(16, 16));
    gpu.clear(b, [1.0, 0.0, 0.5, 1.0]);
    assert_eq!(&gpu.read_back(b)[..4], &[255, 0, 128, 255]);
}
