use std::cell::RefCell;
use std::rc::Rc;

use seal_render::graph::TexturePool;
use seal_render::*;

fn quad(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Instance {
    Instance {
        corners: [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
        uv: [0.0, 0.0, 1.0, 1.0],
        color,
        corner_uv: None,
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3))
}

#[test]
fn unused_passes_are_culled_and_outputs_kept() {
    let mut device = SoftDevice::new();
    let screen = device.create_texture(TextureDesc::rgba8(4, 4));
    let ran = Rc::new(RefCell::new(Vec::new()));
    let mut graph = RenderGraph::new();
    let out = graph.import("screen", screen);
    let scratch = graph.create_texture("scratch", TextureDesc::rgba8(4, 4));
    let unused = graph.create_texture("unused", TextureDesc::rgba8(4, 4));
    for (name, reads, writes, side) in [
        ("scratch", vec![], vec![scratch], false),
        ("dead", vec![], vec![unused], false),
        ("compose", vec![scratch], vec![out], false),
        ("capture", vec![out], vec![], true),
    ] {
        let ran = ran.clone();
        graph.add_pass(
            name,
            |b| {
                for r in &reads {
                    b.read(*r);
                }
                for w in &writes {
                    b.write(*w);
                }
                if side {
                    b.side_effect();
                }
            },
            move |_| ran.borrow_mut().push(name),
        );
    }
    let compiled = graph.execute(&mut device, &mut TexturePool::default()).unwrap();
    assert_eq!(*ran.borrow(), ["scratch", "compose", "capture"]);
    assert_eq!(compiled.culled, ["dead"]);
}

#[test]
fn reading_before_writing_is_an_error() {
    let mut graph = RenderGraph::<SoftDevice>::new();
    let t = graph.create_texture("t", TextureDesc::rgba8(2, 2));
    graph.add_pass(
        "p",
        |b| {
            b.read(t).side_effect();
        },
        |_| {},
    );
    assert_eq!(
        graph.compile().unwrap_err(),
        GraphError::ReadBeforeWrite { pass: "p".into(), resource: "t".into() }
    );
}

#[test]
fn textures_with_separate_lifetimes_share_memory() {
    let mut device = SoftDevice::new();
    let screen = device.create_texture(TextureDesc::rgba8(8, 8));
    let mut pool = TexturePool::default();
    for frame in 0..3 {
        let mut graph = RenderGraph::<SoftDevice>::new();
        let out = graph.import("screen", screen);
        let desc = TextureDesc::rgba8(8, 8);
        let (a, b, c) = (
            graph.create_texture("a", desc),
            graph.create_texture("b", desc),
            graph.create_texture("c", desc),
        );
        graph.add_pass(
            "a",
            |p| {
                p.write(a);
            },
            move |cx| cx.device.clear(cx.texture(a), [1.0, 0.0, 0.0, 1.0]),
        );
        graph.add_pass(
            "b",
            |p| {
                p.read(a).write(b);
            },
            move |cx| {
                let (src, dst) = (cx.texture(a), cx.texture(b));
                cx.device.copy(src, dst);
            },
        );
        // `a` is dead now, so `c` can reuse its texture.
        graph.add_pass(
            "c",
            |p| {
                p.read(b).write(c);
            },
            move |cx| {
                let (src, dst) = (cx.texture(b), cx.texture(c));
                cx.device.copy(src, dst);
            },
        );
        graph.add_pass(
            "out",
            |p| {
                p.read(c).write(out);
            },
            move |cx| {
                let (src, dst) = (cx.texture(c), cx.texture(out));
                cx.device.copy(src, dst);
            },
        );
        let compiled = graph.execute(&mut device, &mut pool).unwrap();
        assert_eq!(compiled.slot_descs.len(), 2, "three textures in two slots");
        let slot = |r| compiled.slots.iter().find(|(id, _)| *id == r).unwrap().1;
        assert_eq!(slot(a), slot(c));
        // Steady frames allocate nothing new: screen plus two pooled textures.
        assert_eq!(device.live_textures(), 3, "frame {frame}");
    }
    assert_eq!(&device.read_back(screen)[..4], &[255, 0, 0, 255]);
    pool.release(&mut device);
    assert_eq!(device.live_textures(), 1);
}

#[test]
fn blended_quads_have_no_seam_on_the_diagonal() {
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(32, 32));
    device.clear(target, [0.0, 0.0, 0.0, 1.0]);
    let batch = QuadBatch {
        texture: None,
        blend: BlendMode::Alpha,
        instances: vec![quad(0.0, 0.0, 32.0, 32.0, [1.0, 1.0, 1.0, 0.5])],
    };
    device.draw_quads(target, &batch);
    let pixels = device.read_back(target);
    assert!(pixels.chunks(4).all(|p| p == [128, 128, 128, 255]), "every pixel blended once");
    assert_eq!(device.pixels_drawn, 32 * 32);

    // A rotated quad: still each covered pixel exactly once.
    device.clear(target, [0.0, 0.0, 0.0, 1.0]);
    let diamond = Instance {
        corners: [[16.0, 2.0], [30.0, 16.0], [16.0, 30.0], [2.0, 16.0]],
        uv: [0.0, 0.0, 1.0, 1.0],
        color: [1.0, 1.0, 1.0, 0.5],
        corner_uv: None,
    };
    device.draw_quads(target, &QuadBatch { instances: vec![diamond], ..batch });
    let pixels = device.read_back(target);
    assert!(pixels.chunks(4).all(|p| p == [0, 0, 0, 255] || p == [128, 128, 128, 255]));
}

#[test]
fn textures_map_one_to_one() {
    let mut device = SoftDevice::new();
    let image = device.create_texture(TextureDesc::rgba8(4, 4));
    let pixels: Vec<u8> = (0..64).map(|i| (i * 4) as u8).collect();
    device.upload(image, &pixels);
    let target = device.create_texture(TextureDesc::rgba8(4, 4));
    device.draw_quads(
        target,
        &QuadBatch {
            texture: Some(image),
            blend: BlendMode::Opaque,
            instances: vec![quad(0.0, 0.0, 4.0, 4.0, [1.0; 4])],
        },
    );
    assert_eq!(device.read_back(target), pixels);
}

#[test]
fn frames_are_identical_on_every_machine() {
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(64, 48));
    device.clear(target, [0.1, 0.2, 0.3, 1.0]);
    let mut instances = Vec::new();
    for i in 0..40 {
        let f = i as f32;
        let (s, c) = (f * 0.37 % 1.0, f * 0.61 % 1.0);
        instances.push(Instance {
            corners: [
                [f * 1.3 + 3.1, f * 0.7 + 1.9],
                [f * 1.3 + 13.7 * c, f * 0.7 + 4.2 * s],
                [f * 1.3 + 11.3, f * 0.7 + 14.9 * c],
                [f * 1.3 + 2.2 * s, f * 0.7 + 9.6],
            ],
            uv: [0.0, 0.0, 1.0, 1.0],
            color: [s, c, 1.0 - s, 0.3 + 0.5 * c],
            corner_uv: None,
        });
    }
    for blend in [BlendMode::Alpha, BlendMode::Additive] {
        device
            .draw_quads(target, &QuadBatch { texture: None, blend, instances: instances.clone() });
    }
    let hash = fnv(&device.read_back(target));
    // Recorded on Linux; Windows and every other machine must match.
    assert_eq!(hash, GOLDEN, "frame hash {hash:#x}");
}

const GOLDEN: u64 = 0x5073_aab3_54bf_263e;
