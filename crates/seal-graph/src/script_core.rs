//! Script core nodes: quaternions, planes, text, typed JSON and clocks.

use crate::rt::{Host, Plane, Quat, V3, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptCoreOp {
    QuatEuler,
    QuatAxisAngle,
    QuatMul,
    QuatSlerp,
    QuatRotateVec,
    PlanePointNormal,
    PlaneDistance,
    PlaneRayHit,
    Utf8ToUtf16,
    Utf16ToUtf8,
    LayoutLines,
    ReadTypedJson,
    WriteTypedJson,
    UnscaledTime,
    UnscaledDt,
    SimTick,
}

pub fn eval_core(op: ScriptCoreOp, args: &[Value], host: &mut dyn Host) -> Value {
    match op {
        ScriptCoreOp::QuatEuler => {
            let pitch = args[0].float() as f32;
            let yaw = args[1].float() as f32;
            let roll = args[2].float() as f32;
            let q = seal_math::Quat::from_euler_deg(pitch, yaw, roll);
            Value::Quat(Quat {
                x: f64::from(q.x),
                y: f64::from(q.y),
                z: f64::from(q.z),
                w: f64::from(q.w),
            })
        }
        ScriptCoreOp::QuatAxisAngle => {
            let axis = args[0].vec3();
            let angle = args[1].float() as f32;
            let q = seal_math::Quat::from_axis_angle(
                [axis.x as f32, axis.y as f32, axis.z as f32],
                angle,
            );
            Value::Quat(Quat {
                x: f64::from(q.x),
                y: f64::from(q.y),
                z: f64::from(q.z),
                w: f64::from(q.w),
            })
        }
        ScriptCoreOp::QuatMul => {
            let a = to_math_quat(args[0].quat());
            let b = to_math_quat(args[1].quat());
            let q = a.mul(b);
            Value::Quat(from_math_quat(q))
        }
        ScriptCoreOp::QuatSlerp => {
            let a = to_math_quat(args[0].quat());
            let b = to_math_quat(args[1].quat());
            let t = args[2].float() as f32;
            Value::Quat(from_math_quat(a.slerp(b, t)))
        }
        ScriptCoreOp::QuatRotateVec => {
            let q = to_math_quat(args[0].quat());
            let v = args[1].vec3();
            let out = q.rotate_vec([v.x as f32, v.y as f32, v.z as f32]);
            Value::Vec3(V3 {
                x: f64::from(out[0]),
                y: f64::from(out[1]),
                z: f64::from(out[2]),
            })
        }
        ScriptCoreOp::PlanePointNormal => {
            let point = args[0].vec3();
            let normal = args[1].vec3();
            let p = seal_math::Plane::from_point_normal(
                [point.x as f32, point.y as f32, point.z as f32],
                [normal.x as f32, normal.y as f32, normal.z as f32],
            );
            Value::Plane(Plane {
                nx: f64::from(p.nx),
                ny: f64::from(p.ny),
                nz: f64::from(p.nz),
                d: f64::from(p.d),
            })
        }
        ScriptCoreOp::PlaneDistance => {
            let plane = args[0].plane();
            let point = args[1].vec3();
            let p = seal_math::Plane {
                nx: plane.nx as f32,
                ny: plane.ny as f32,
                nz: plane.nz as f32,
                d: plane.d as f32,
            };
            Value::Float(f64::from(p.distance([point.x as f32, point.y as f32, point.z as f32])))
        }
        ScriptCoreOp::PlaneRayHit => {
            let plane = args[0].plane();
            let origin = args[1].vec3();
            let dir = args[2].vec3();
            let p = seal_math::Plane {
                nx: plane.nx as f32,
                ny: plane.ny as f32,
                nz: plane.nz as f32,
                d: plane.d as f32,
            };
            let hit = p
                .ray_hit(
                    [origin.x as f32, origin.y as f32, origin.z as f32],
                    [dir.x as f32, dir.y as f32, dir.z as f32],
                )
                .map(f64::from)
                .unwrap_or(-1.0);
            Value::Float(hit)
        }
        ScriptCoreOp::Utf8ToUtf16 => Value::Str(utf8_to_utf16_text(&args[0].str())),
        ScriptCoreOp::Utf16ToUtf8 => Value::Str(host.utf16_to_utf8(&args[0].str())),
        ScriptCoreOp::LayoutLines => {
            let text = args[0].str();
            let width = args[1].float() as f32;
            let size = args[2].float() as f32;
            Value::Str(layout_lines_text(&text, width, size))
        }
        ScriptCoreOp::ReadTypedJson => {
            let json = args[0].str();
            let type_path = args[1].str();
            Value::Str(host.read_typed_json(&json, &type_path))
        }
        ScriptCoreOp::WriteTypedJson => {
            let data_path = args[0].str();
            Value::Str(host.write_typed_json(&data_path))
        }
        ScriptCoreOp::UnscaledTime => Value::Float(host.unscaled_time()),
        ScriptCoreOp::UnscaledDt => Value::Float(host.unscaled_dt()),
        ScriptCoreOp::SimTick => Value::Int(host.sim_tick()),
    }
}

fn to_math_quat(q: Quat) -> seal_math::Quat {
    seal_math::Quat {
        x: q.x as f32,
        y: q.y as f32,
        z: q.z as f32,
        w: q.w as f32,
    }
}

fn from_math_quat(q: seal_math::Quat) -> Quat {
    Quat {
        x: f64::from(q.x),
        y: f64::from(q.y),
        z: f64::from(q.z),
        w: f64::from(q.w),
    }
}

fn utf8_to_utf16_text(text: &str) -> String {
    text.encode_utf16()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn layout_lines_text(text: &str, width: f32, font_size: f32) -> String {
    let width = width.max(font_size);
    let advance = font_size * 0.55;
    let line_height = font_size * 1.2;
    let mut out = String::new();
    let mut current = String::new();
    let mut glyphs = String::new();
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;
    let flush = |out: &mut String, current: &mut String, glyphs: &mut String| {
        if current.is_empty() {
            return;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(current);
        out.push('|');
        out.push_str(glyphs);
        current.clear();
        glyphs.clear();
    };
    for word in text.split_whitespace() {
        let word_width = word.chars().count() as f32 * advance;
        let need_space = !current.is_empty();
        let extra = if need_space { advance } else { 0.0 };
        if x + extra + word_width > width && !current.is_empty() {
            flush(&mut out, &mut current, &mut glyphs);
            x = 0.0;
            y += line_height;
        }
        if need_space {
            current.push(' ');
            if !glyphs.is_empty() {
                glyphs.push(',');
            }
            glyphs.push_str(&format!("{x:.3},{y:.3}"));
            x += advance;
        }
        for ch in word.chars() {
            current.push(ch);
            if !glyphs.is_empty() {
                glyphs.push(',');
            }
            glyphs.push_str(&format!("{x:.3},{y:.3}"));
            x += advance;
        }
    }
    flush(&mut out, &mut current, &mut glyphs);
    out
}