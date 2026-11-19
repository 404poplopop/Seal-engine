//! 2D materials: built-in sprite effects and canvas shader files.

use seal_editor_core::material2d::Material2d;
use seal_editor_core::material2d::{self, load_program, shade, Picture, Program};
use seal_editor_core::{EditorSession, Layout};

fn set(session: &mut EditorSession, id: &str, field: &str, value: &str) {
    session.models.set_field(id, field, value);
    session.mirror_node(id);
}

fn sprite_id(session: &EditorSession) -> String {
    let es = &session.world.entities;
    es.iter()
        .find(|e| !es.iter().any(|c| c.parent.as_deref() == Some(e.id.as_str())))
        .map(|e| e.id.clone())
        .unwrap()
}

fn mat(effect: &str, color: [f32; 4], amount: f32, speed: f32) -> Material2d {
    Material2d {
        effect: effect.into(),
        color,
        amount,
        speed,
        shader: String::new(),
        ..Material2d::default()
    }
}

/// A 5x5 picture: transparent, with one opaque pure-red pixel in the middle.
fn dot() -> Vec<u8> {
    let mut px = vec![0u8; 5 * 5 * 4];
    px[(2 * 5 + 2) * 4..(2 * 5 + 2) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
    px
}

fn at(out: &[u8], x: usize, y: usize) -> [u8; 4] {
    let i = (y * 5 + x) * 4;
    [out[i], out[i + 1], out[i + 2], out[i + 3]]
}

#[test]
fn material_settings_edit_save_load_and_refuse_bad_values() {
    let mut s = EditorSession::new(Layout::default());
    let a = sprite_id(&s);
    for (k, v) in [
        ("material2d", "custom"),
        ("mat_color", "#33E6FF"),
        ("mat_amount", "0.4"),
        ("mat_speed", "2"),
        ("mat_shader", "shaders\\shimmer.shade2d"),
        // Refused: an unknown effect, a bad color, amounts out of range, the wrong file type.
        ("material2d", "sparkle"),
        ("mat_color", "teal-ish"),
        ("mat_amount", "1.5"),
        ("mat_speed", "-1"),
        ("mat_shader", "shaders/shimmer.png"),
    ] {
        set(&mut s, &a, k, v);
    }
    let e = s.world.entity(&a).unwrap().clone();
    let m = &e.material2d;
    assert_eq!((m.effect.as_str(), m.amount, m.speed), ("custom", 0.4, 2.0));
    assert_eq!(seal_editor_core::tint_hex(&m.color), "#33E6FF");
    assert_eq!(m.shader, "shaders/shimmer.shade2d", "backslashes become forward slashes");
    let fields: Vec<String> = e.inspector_fields().into_iter().map(|(k, _)| k).collect();
    for k in material2d::KEYS {
        assert!(fields.iter().any(|f| f == k), "{k} shows in the inspector");
    }
    assert!(material2d::check("mat_amount", "2").is_some());
    assert_eq!(material2d::check("mat_shader", "x.png"), Some("inspector-mat-shader-invalid"));
    assert!(material2d::check("mat_shader", "").is_none(), "no file yet is fine");
    let loaded = seal_world::World::from_text(&s.world.to_text()).unwrap();
    assert_eq!(loaded.entity(&a).unwrap().material2d, e.material2d);
    // Going back to none hides the effect settings again.
    set(&mut s, &a, "material2d", "none");
    let fields: Vec<String> =
        s.world.entity(&a).unwrap().inspector_fields().into_iter().map(|(k, _)| k).collect();
    assert!(fields.iter().any(|f| f == "material2d"));
    assert!(!fields.iter().any(|f| f == "mat_amount"));
}

#[test]
fn built_in_effects_change_the_right_pixels() {
    let px = dot();
    let pic = Picture { pixels: &px, width: 5, height: 5 };
    // Grayscale: the red dot turns grey, the rest is untouched.
    let out = shade(&pic, &mat("grayscale", [1.0; 4], 1.0, 0.0), None, 0.0);
    let [r, g, b, a] = at(&out, 2, 2);
    assert!(r == g && g == b && r > 0 && r < 255 && a == 255, "{:?}", at(&out, 2, 2));
    assert_eq!(at(&out, 0, 0), [0, 0, 0, 0]);
    // Flash at full strength paints the dot in the effect color.
    let out = shade(&pic, &mat("flash", [0.0, 0.0, 1.0, 1.0], 1.0, 0.0), None, 0.0);
    assert_eq!(at(&out, 2, 2), [0, 0, 255, 255]);
    // Outline: the empty pixel beside the dot takes the outline color, far corners stay empty.
    let out = shade(&pic, &mat("outline", [0.0, 1.0, 0.0, 1.0], 0.0, 0.0), None, 0.0);
    assert_eq!(at(&out, 3, 2), [0, 255, 0, 255]);
    assert_eq!(at(&out, 2, 2), [255, 0, 0, 255], "the sprite itself stays");
    assert_eq!(at(&out, 0, 0), [0, 0, 0, 0]);
    // Dissolve all the way removes everything; none removes nothing.
    let out = shade(&pic, &mat("dissolve", [1.0; 4], 1.0, 0.0), None, 0.0);
    assert!(out.chunks(4).all(|p| p[3] == 0));
    assert_eq!(shade(&pic, &mat("", [1.0; 4], 1.0, 0.0), None, 0.0), px);
    // Moving effects are redrawn each frame; still ones are not.
    assert!(material2d::animated(&mat("wave", [1.0; 4], 1.0, 1.0)));
    assert!(!material2d::animated(&mat("wave", [1.0; 4], 1.0, 0.0)));
    assert!(!material2d::animated(&mat("grayscale", [1.0; 4], 1.0, 5.0)));
}

#[test]
fn canvas_shader_files_run_per_pixel_and_report_mistakes() {
    let dir = std::env::temp_dir().join(format!("seal-mat2d-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let good = dir.join("left.shade2d");
    // Paint the left half white and opaque; keep the right half as it was.
    std::fs::write(&good, "# split\nleft = step(u, 0.5)\nr = mix(r, 1, left)\ng = mix(g, 1, left)\nb = mix(b, 1, left)\na = max(a, left)\n").unwrap();
    let program: Program = load_program(&good).unwrap();
    let px = dot();
    let pic = Picture { pixels: &px, width: 5, height: 5 };
    let out = shade(&pic, &mat("custom", [1.0; 4], 1.0, 0.0), Some(&program), 0.0);
    assert_eq!(at(&out, 0, 0), [255, 255, 255, 255]);
    assert_eq!(at(&out, 4, 4), [0, 0, 0, 0]);
    // Without a usable program the sprite is drawn as it is.
    assert_eq!(shade(&pic, &mat("custom", [1.0; 4], 1.0, 0.0), None, 0.0), px);
    // Time feeds in, so an animated shader gives different pictures over time.
    let pulse = Program::parse("a = 0.5 + 0.5 * sin(time * 3)\nr = 1").unwrap();
    let t0 = shade(&pic, &mat("custom", [1.0; 4], 1.0, 1.0), Some(&pulse), 0.0);
    let t1 = shade(&pic, &mat("custom", [1.0; 4], 1.0, 1.0), Some(&pulse), 0.4);
    assert_ne!(t0, t1);
    // Mistakes name the line.
    let bad = dir.join("bad.shade2d");
    std::fs::write(&bad, "r = 1\ng = smoothstep(0, 1, u\n").unwrap();
    let err = load_program(&bad).unwrap_err();
    assert!(err.starts_with("line 2:"), "{err}");
    assert!(load_program(&dir.join("missing.shade2d")).is_err());
    std::fs::remove_dir_all(&dir).ok();
}
