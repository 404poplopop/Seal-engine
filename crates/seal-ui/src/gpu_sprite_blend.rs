//! Textured and solid triangles in the viewport framebuffer with per-batch blend modes.

use std::sync::{Arc, Mutex};

use eframe::egui_glow;
use eframe::glow::{self, HasContext};
use egui::Rect;

use crate::sprite_blend::{batch_setup, split_batches, BlendDraw, BlendVertex, SavedBlendState};

static STATE: Mutex<Option<GlSpriteState>> = Mutex::new(None);

struct GlSpriteState {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
}

const FLOATS_PER_VERTEX: usize = 8;

fn build(gl: &glow::Context) -> Result<GlSpriteState, String> {
    let version = egui_glow::ShaderVersion::get(gl);
    let new = version.is_new_shader_interface();
    let (attr, vout, fin) =
        if new { ("in", "out", "in") } else { ("attribute", "varying", "varying") };
    let out_decl = if new { "out vec4 out_color;\n" } else { "" };
    let out_name = if new { "out_color" } else { "gl_FragColor" };
    let precision = if version.is_embedded() { "precision mediump float;\n" } else { "" };
    let tex = if new { "texture" } else { "texture2D" };
    let vs = format!(
        "{}{precision}{attr} vec2 a_pos;\n{attr} vec2 a_tc;\n{attr} vec4 a_tint;\n\
         {vout} vec2 v_tc;\n{vout} vec4 v_tint;\n\
         void main() {{ v_tc = a_tc; v_tint = a_tint; gl_Position = vec4(a_pos, 0.0, 1.0); }}\n",
        version.version_declaration()
    );
    let fs = format!(
        "{}{precision}{fin} vec2 v_tc;\n{fin} vec4 v_tint;\n{out_decl}uniform sampler2D u_tex;\n\
         void main() {{ vec4 t = {tex}(u_tex, v_tc); {out_name} = vec4(t.rgb * v_tint.rgb * v_tint.a, t.a * v_tint.a); }}\n",
        version.version_declaration()
    );
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, &src);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(gl.get_shader_info_log(shader));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.bind_attrib_location(program, 0, "a_pos");
        gl.bind_attrib_location(program, 1, "a_tc");
        gl.bind_attrib_location(program, 2, "a_tint");
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        Ok(GlSpriteState { program, vao: gl.create_vertex_array()?, vbo: gl.create_buffer()? })
    }
}

pub(crate) fn vertex_floats(v: &BlendVertex, rect: Rect) -> [f32; FLOATS_PER_VERTEX] {
    [
        (v.pos.x - rect.min.x) / rect.width().max(1e-4) * 2.0 - 1.0,
        1.0 - (v.pos.y - rect.min.y) / rect.height().max(1e-4) * 2.0,
        v.uv[0],
        v.uv[1],
        v.tint[0],
        v.tint[1],
        v.tint[2],
        v.tint[3],
    ]
}

fn save_blend(gl: &glow::Context) -> SavedBlendState {
    unsafe {
        SavedBlendState {
            blend: gl.is_enabled(glow::BLEND),
            src_rgb: gl.get_parameter_i32(glow::BLEND_SRC_RGB) as u32,
            dst_rgb: gl.get_parameter_i32(glow::BLEND_DST_RGB) as u32,
            src_alpha: gl.get_parameter_i32(glow::BLEND_SRC_ALPHA) as u32,
            dst_alpha: gl.get_parameter_i32(glow::BLEND_DST_ALPHA) as u32,
            eq_rgb: gl.get_parameter_i32(glow::BLEND_EQUATION_RGB) as u32,
            eq_alpha: gl.get_parameter_i32(glow::BLEND_EQUATION_ALPHA) as u32,
        }
    }
}

fn restore_blend(gl: &glow::Context, saved: &SavedBlendState) {
    unsafe {
        if saved.blend {
            gl.enable(glow::BLEND);
        } else {
            gl.disable(glow::BLEND);
        }
        gl.blend_func_separate(saved.src_rgb, saved.dst_rgb, saved.src_alpha, saved.dst_alpha);
        gl.blend_equation_separate(saved.eq_rgb, saved.eq_alpha);
    }
}

fn apply_batch_blend(gl: &glow::Context, mode: &str) {
    let setup = batch_setup(mode);
    unsafe {
        if setup.enable_blend {
            gl.enable(glow::BLEND);
        }
        let (s_rgb, d_rgb, s_a, d_a) = setup.factors;
        gl.blend_equation_separate(glow::FUNC_ADD, glow::FUNC_ADD);
        gl.blend_func_separate(s_rgb, d_rgb, s_a, d_a);
    }
}

fn draw_batches(gl: &glow::Context, painter: &egui_glow::Painter, rect: Rect, draws: &[BlendDraw]) {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        match build(gl) {
            Ok(state) => *guard = Some(state),
            Err(err) => {
                eprintln!("GPU sprite blend could not start: {err}");
                return;
            }
        }
    }
    let Some(state) = guard.as_ref() else { return };
    let saved = save_blend(gl);
    let batches = split_batches(draws);
    unsafe {
        gl.use_program(Some(state.program));
        gl.bind_vertex_array(Some(state.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.vbo));
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::CULL_FACE);
        let stride = (FLOATS_PER_VERTEX * 4) as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, stride, 8);
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, stride, 16);
        for batch in batches {
            apply_batch_blend(gl, &batch.key.mode);
            gl.uniform_1_i32(gl.get_uniform_location(state.program, "u_tex").as_ref(), 0);
            gl.active_texture(glow::TEXTURE0);
            if let Some(tex) = batch.items.first().and_then(|q| painter.texture(q.texture)) {
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            } else {
                continue;
            }
            let mut data = Vec::new();
            for draw in &batch.items {
                for chunk in draw.vertices.chunks(3) {
                    if chunk.len() < 3 {
                        continue;
                    }
                    for v in chunk {
                        data.extend(vertex_floats(v, rect));
                    }
                }
            }
            if data.is_empty() {
                continue;
            }
            let bytes: &[u8] = std::slice::from_raw_parts(
                data.as_ptr().cast(),
                std::mem::size_of_val(data.as_slice()),
            );
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
            gl.draw_arrays(glow::TRIANGLES, 0, (data.len() / FLOATS_PER_VERTEX) as i32);
        }
        for index in 0..3 {
            gl.disable_vertex_attrib_array(index);
        }
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.use_program(None);
    }
    restore_blend(gl, &saved);
}

/// Paint callback that composites queued triangles onto the current framebuffer.
pub fn paint_callback(rect: Rect, draws: Vec<BlendDraw>) -> egui::PaintCallback {
    if crate::gpu_viewport_wgpu::active() {
        return crate::gpu_sprite_wgpu::paint_callback(rect, draws);
    }
    let draws = Arc::new(draws);
    egui::PaintCallback {
        rect,
        callback: Arc::new(egui_glow::CallbackFn::new(move |_info, painter| {
            draw_batches(painter.gl(), painter, rect, &draws);
        })),
    }
}
