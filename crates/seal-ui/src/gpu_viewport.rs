//! Draws the viewport's lit shapes on the graphics card with a depth buffer, so
//! shapes hide each other per pixel. The CPU mesh in `viewport_mesh` is the
//! fallback when no OpenGL context is available.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui_glow;
use eframe::glow::{self, HasContext};
use egui::Rect;

use crate::viewport_mesh::LitTriangle;

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Turned on by the app when it runs on OpenGL with a depth buffer.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

struct GlState {
    /// Programs for flat, array, volume and cubemap sampling. Slots after the first
    /// stay empty when the shading language cannot name those samplers.
    programs: [Option<glow::Program>; 4],
    real_samplers: bool,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    texture: glow::Texture,
}

fn program_slot(kind: crate::gpu_textures::SampleKind) -> usize {
    match kind {
        crate::gpu_textures::SampleKind::None | crate::gpu_textures::SampleKind::D2 => 0,
        crate::gpu_textures::SampleKind::Array => 1,
        crate::gpu_textures::SampleKind::Volume => 2,
        crate::gpu_textures::SampleKind::Cube => 3,
    }
}

static LAYER_COPY_LOGGED: AtomicBool = AtomicBool::new(false);

fn log_layer_copy_once() {
    if !LAYER_COPY_LOGGED.swap(true, Ordering::Relaxed) {
        eprintln!(
            "This shading language is version 1 or GLES 2, so array, volume and cubemap materials use a flat copy of one layer."
        );
    }
}

/// Shadow-map depths packed 24 bits into RGB (texels outside every shape read as farthest).
pub fn pack_shadow(depths: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(depths.len() * 4);
    for &d in depths {
        let v = (d.clamp(0.0, 1.0) * 16_777_215.0) as u32;
        out.extend_from_slice(&[(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]);
    }
    out
}

/// Shadow map handed to the graphics card: side length and packed texels.
pub struct ShadowTexture {
    pub size: usize,
    pub texels: Vec<u8>,
}

static STATE: Mutex<Option<GlState>> = Mutex::new(None);

/// Floats per vertex: x, y, z (clip space), lit colour (4), shadowed colour (4),
/// shadow-map u, v, depth, the shadow bias, texture uv, layer and slice, then the normal.
const STRIDE: usize = 22;

/// One run of vertices that share a texture, sampler, layer and slice.
pub struct AlbedoBatch {
    pub first: i32,
    pub count: i32,
    pub path: String,
    pub kind: crate::gpu_textures::SampleKind,
    pub layer: u32,
    pub slice: f32,
}

/// What the card needs besides the triangles: project anisotropy and the sky cubemap.
#[derive(Clone, Debug)]
pub struct GpuFrame {
    pub anisotropy: u32,
    pub sky: String,
    pub project: Option<std::path::PathBuf>,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub look_through: bool,
    /// Project Settings > Rendering > Render scale, in percent (25 to 100).
    pub render_scale: u32,
}

/// Vertex data for the triangles inside `rect`, depth scaled so the nearest
/// point is -1 and the farthest +1.
pub fn vertex_data(tris: &[LitTriangle], rect: Rect) -> (Vec<f32>, Vec<AlbedoBatch>) {
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for t in tris {
        for d in t.depths {
            lo = lo.min(d);
            hi = hi.max(d);
        }
    }
    let span = (hi - lo).max(1e-6);
    let mut order: Vec<usize> = (0..tris.len()).collect();
    order.sort_by(|&a, &b| {
        (&tris[a].tex_path, tris[a].tex_kind, tris[a].tex_layer, tris[a].tex_slice.to_bits()).cmp(&(
            &tris[b].tex_path,
            tris[b].tex_kind,
            tris[b].tex_layer,
            tris[b].tex_slice.to_bits(),
        ))
    });
    let mut out = Vec::with_capacity(tris.len() * 3 * STRIDE);
    let mut batches = Vec::new();
    for index in order {
        let t = &tris[index];
        let rgba = t.color.to_array();
        if batches.last().is_none_or(|batch: &AlbedoBatch| {
            batch.path != t.tex_path
                || batch.kind != t.tex_kind
                || batch.layer != t.tex_layer
                || batch.slice.to_bits() != t.tex_slice.to_bits()
        }) {
            batches.push(AlbedoBatch {
                first: (out.len() / STRIDE) as i32,
                count: 0,
                path: t.tex_path.clone(),
                kind: t.tex_kind,
                layer: t.tex_layer,
                slice: t.tex_slice,
            });
        }
        for (k, (p, d)) in t.points.iter().zip(t.depths).enumerate() {
            out.push((p.x - rect.min.x) / rect.width() * 2.0 - 1.0);
            out.push(1.0 - (p.y - rect.min.y) / rect.height() * 2.0);
            // Keep a little room at both ends so nothing is clipped.
            out.push(((d - lo) / span * 2.0 - 1.0) * 0.999);
            out.extend(rgba.iter().map(|&c| f32::from(c) / 255.0));
            out.extend(t.dark.to_array().iter().map(|&c| f32::from(c) / 255.0));
            out.extend(t.shadow_uvz[k]);
            out.push(t.shadow_bias);
            out.extend(t.uv[k]);
            out.push(t.tex_layer as f32);
            out.push(t.tex_slice);
            out.extend(t.normal);
        }
        if let Some(batch) = batches.last_mut() {
            batch.count += 3;
        }
    }
    (out, batches)
}

fn with_precision(src: String, embedded: bool) -> String {
    if !embedded {
        return src;
    }
    match src.split_once('\n') {
        Some((head, tail)) => format!("{head}\nprecision mediump float;\n{tail}"),
        None => src,
    }
}

fn use_version(src: String, declaration: &str) -> String {
    match src.split_once('\n') {
        Some((_, rest)) => format!("{declaration}{rest}"),
        None => src,
    }
}

fn link_program(
    gl: &glow::Context,
    kind: crate::gpu_textures::SampleKind,
    new_interface: bool,
    embedded: bool,
    declaration: &str,
) -> Result<glow::Program, String> {
    let (vertex, fragment) = crate::gpu_textures::mesh_shader_source(kind, new_interface);
    let vertex = with_precision(use_version(vertex, declaration), embedded);
    let fragment = with_precision(use_version(fragment, declaration), embedded);
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (stage, src) in [(glow::VERTEX_SHADER, vertex), (glow::FRAGMENT_SHADER, fragment)] {
            let shader = gl.create_shader(stage)?;
            gl.shader_source(shader, &src);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                let err = gl.get_shader_info_log(shader);
                gl.delete_shader(shader);
                return Err(err);
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.bind_attrib_location(program, 0, "a_pos");
        gl.bind_attrib_location(program, 1, "a_color");
        gl.bind_attrib_location(program, 2, "a_dark");
        gl.bind_attrib_location(program, 3, "a_shadow");
        gl.bind_attrib_location(program, 4, "a_tex");
        gl.bind_attrib_location(program, 5, "a_normal");
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let err = gl.get_program_info_log(program);
            gl.delete_program(program);
            return Err(err);
        }
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        Ok(program)
    }
}

fn build(gl: &glow::Context) -> Result<GlState, String> {
    let version = egui_glow::ShaderVersion::get(gl);
    let language = unsafe { gl.get_parameter_string(glow::SHADING_LANGUAGE_VERSION) };
    let mut real = crate::gpu_textures::real_texture_samplers(&language);
    let new = version.is_new_shader_interface();
    let embedded = version.is_embedded();
    let declaration = version.version_declaration();
    let flat = link_program(gl, crate::gpu_textures::SampleKind::D2, new, embedded, declaration)?;
    let mut programs = [None, None, None, None];
    programs[0] = Some(flat);
    if real {
        for kind in [
            crate::gpu_textures::SampleKind::Array,
            crate::gpu_textures::SampleKind::Volume,
            crate::gpu_textures::SampleKind::Cube,
        ] {
            match link_program(gl, kind, true, embedded, declaration) {
                Ok(program) => programs[program_slot(kind)] = Some(program),
                Err(err) => {
                    eprintln!("mesh sampler {kind:?} could not start, using a flat copy: {err}");
                    real = false;
                    log_layer_copy_once();
                    break;
                }
            }
        }
        if !real {
            for program in programs.iter_mut().skip(1) {
                if let Some(program) = program.take() {
                    unsafe { gl.delete_program(program) };
                }
            }
        }
    }
    unsafe {
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        let texture = gl.create_texture()?;
        Ok(GlState { programs, real_samplers: real, vao, vbo, texture })
    }
}

fn draw(
    gl: &glow::Context,
    data: &[f32],
    batches: &[AlbedoBatch],
    shadow: Option<&ShadowTexture>,
    frame: &GpuFrame,
) {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        match build(gl) {
            Ok(state) => *guard = Some(state),
            Err(err) => {
                eprintln!("GPU viewport could not start, using the CPU drawing: {err}");
                set_enabled(false);
                return;
            }
        }
    }
    let Some(state) = guard.as_ref() else { return };
    let bytes: &[u8] =
        unsafe { std::slice::from_raw_parts(data.as_ptr().cast(), std::mem::size_of_val(data)) };
    unsafe {
        let flat = state.programs[0];
        gl.use_program(flat);
        gl.bind_vertex_array(Some(state.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.vbo));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
        let stride = (STRIDE * 4) as i32;
        for (index, size, offset) in
            [(0u32, 3, 0), (1, 4, 3), (2, 4, 7), (3, 4, 11), (4, 4, 15), (5, 3, 19)]
        {
            gl.enable_vertex_attrib_array(index);
            gl.vertex_attrib_pointer_f32(index, size, glow::FLOAT, false, stride, offset * 4);
        }
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(state.texture));
        let has = match shadow {
            Some(map) => {
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA as i32,
                    map.size as i32,
                    map.size as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(&map.texels)),
                );
                for (param, value) in [
                    (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                    (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                    (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                ] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, param, value as i32);
                }
                gl.uniform_1_f32(
                    gl.get_uniform_location(flat.unwrap(), "u_texel").as_ref(),
                    1.0 / map.size as f32,
                );
                1.0
            }
            None => 0.0,
        };
        gl.uniform_1_i32(gl.get_uniform_location(flat.unwrap(), "u_shadow").as_ref(), 0);
        gl.uniform_1_f32(gl.get_uniform_location(flat.unwrap(), "u_has_shadow").as_ref(), has);
        gl.disable(glow::BLEND);
        gl.disable(glow::CULL_FACE);
        gl.enable(glow::DEPTH_TEST);
        gl.depth_func(glow::LEQUAL);
        gl.depth_mask(true);
        gl.clear_depth_f32(1.0);
        // egui's scissor keeps this clear inside the viewport.
        gl.clear(glow::DEPTH_BUFFER_BIT);
        for batch in batches {
            let advanced = matches!(
                batch.kind,
                crate::gpu_textures::SampleKind::Array
                    | crate::gpu_textures::SampleKind::Volume
                    | crate::gpu_textures::SampleKind::Cube
            );
            let copy = advanced && !state.real_samplers;
            if copy {
                log_layer_copy_once();
            }
            let program = if copy {
                flat
            } else {
                state.programs[program_slot(batch.kind)].or(flat)
            };
            let Some(program) = program else { continue };
            gl.use_program(Some(program));
            gl.uniform_1_i32(gl.get_uniform_location(program, "u_shadow").as_ref(), 0);
            gl.uniform_1_f32(gl.get_uniform_location(program, "u_has_shadow").as_ref(), has);
            gl.uniform_1_f32(
                gl.get_uniform_location(program, "u_texel").as_ref(),
                shadow.as_ref().map(|map| 1.0 / map.size as f32).unwrap_or(1.0),
            );
            gl.uniform_1_i32(gl.get_uniform_location(program, "u_albedo").as_ref(), 1);
            let bound = if batch.path.is_empty() {
                None
            } else {
                crate::gpu_textures::bind_material(
                    gl,
                    frame.project.as_deref(),
                    &batch.path,
                    batch.layer,
                    batch.slice,
                    frame.anisotropy,
                    advanced && !copy,
                )
            };
            if let Some(tex) = bound.as_ref() {
                if batch.kind == crate::gpu_textures::SampleKind::Volume && !copy {
                    gl.uniform_1_f32(
                        gl.get_uniform_location(program, "u_depth").as_ref(),
                        tex.slices.max(1.0),
                    );
                }
            }
            gl.uniform_1_f32(
                gl.get_uniform_location(program, "u_has_albedo").as_ref(),
                if bound.is_some() { 1.0 } else { 0.0 },
            );
            gl.active_texture(glow::TEXTURE1);
            let bound_target = bound.as_ref().map(|tex| tex.target);
            match bound.as_ref() {
                Some(tex) => gl.bind_texture(tex.target, Some(tex.texture)),
                None => {
                    gl.bind_texture(glow::TEXTURE_2D, None);
                    if state.real_samplers {
                        gl.bind_texture(glow::TEXTURE_2D_ARRAY, None);
                        gl.bind_texture(glow::TEXTURE_3D, None);
                        gl.bind_texture(glow::TEXTURE_CUBE_MAP, None);
                    }
                }
            }
            gl.draw_arrays(glow::TRIANGLES, batch.first, batch.count);
            if let Some(target) = bound_target {
                gl.bind_texture(target, None);
            }
        }
        gl.disable(glow::DEPTH_TEST);
        for index in 0..6 {
            gl.disable_vertex_attrib_array(index);
        }
        gl.active_texture(glow::TEXTURE1);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.use_program(None);
    }
}

/// A paint step that draws `tris` on the graphics card inside `rect`, testing
/// `shadow` per pixel when given.
pub fn paint_callback(
    rect: Rect,
    tris: &[LitTriangle],
    shadow: Option<&seal_editor_core::shadow::ShadowMap>,
    frame: GpuFrame,
) -> egui::PaintCallback {
    if crate::gpu_viewport_wgpu::active() {
        return crate::gpu_viewport_wgpu::paint_callback(rect, tris, shadow, frame);
    }
    let (data, batches) = vertex_data(tris, rect);
    let data = Arc::new(data);
    let batches = Arc::new(batches);
    let shadow = Arc::new(
        shadow.map(|map| ShadowTexture { size: map.size, texels: pack_shadow(map.depths()) }),
    );
    let frame = Arc::new(frame);
    egui::PaintCallback {
        rect,
        callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
            let gl = painter.gl();
            let px = info.viewport_in_pixels();
            let size = scaled_size(px.width_px as u32, px.height_px as u32, frame.render_scale);
            let full = (px.width_px as u32, px.height_px as u32);
            if size == full || !draw_scaled(gl, size, |gl| draw(gl, &data, &batches, shadow.as_ref().as_ref(), &frame)) {
                draw(gl, &data, &batches, shadow.as_ref().as_ref(), &frame);
            }
        })),
    }
}

/// Pixel size of the scene image for a render scale in percent (at least 1x1).
pub fn scaled_size(width: u32, height: u32, percent: u32) -> (u32, u32) {
    let p = percent.clamp(25, 100);
    if p == 100 {
        return (width, height);
    }
    (((width * p) / 100).max(1), ((height * p) / 100).max(1))
}

struct ScaleTarget {
    fbo: glow::Framebuffer,
    color: glow::Texture,
    depth: glow::Renderbuffer,
    size: (u32, u32),
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
}

static SCALE: Mutex<Option<ScaleTarget>> = Mutex::new(None);
static SCALE_FAILED: AtomicBool = AtomicBool::new(false);

/// Draw the lit scene into a smaller image, then stretch it over the viewport with
/// linear filtering, keeping what is already behind it where the scene has no pixels.
/// Returns false when the card cannot do it (the caller draws at full size).
fn draw_scaled(gl: &glow::Context, size: (u32, u32), paint: impl FnOnce(&glow::Context)) -> bool {
    if SCALE_FAILED.load(Ordering::Relaxed) {
        return false;
    }
    let mut guard = SCALE.lock().unwrap_or_else(|err| err.into_inner());
    if guard.as_ref().is_none_or(|t| t.size != size) {
        if let Some(old) = guard.take() {
            unsafe {
                gl.delete_framebuffer(old.fbo);
                gl.delete_texture(old.color);
                gl.delete_renderbuffer(old.depth);
                gl.delete_program(old.program);
                gl.delete_vertex_array(old.vao);
                gl.delete_buffer(old.vbo);
            }
        }
        match build_scale_target(gl, size) {
            Ok(target) => *guard = Some(target),
            Err(err) => {
                eprintln!("render scale is not available on this card: {err}");
                SCALE_FAILED.store(true, Ordering::Relaxed);
                return false;
            }
        }
    }
    let Some(t) = guard.as_ref() else { return false };
    unsafe {
        let previous = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        let mut viewport = [0i32; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
        let scissor_on = gl.is_enabled(glow::SCISSOR_TEST);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(t.fbo));
        gl.viewport(0, 0, t.size.0 as i32, t.size.1 as i32);
        gl.disable(glow::SCISSOR_TEST);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear_depth_f32(1.0);
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        paint(gl);
        gl.bind_framebuffer(glow::FRAMEBUFFER, previous);
        gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
        if scissor_on {
            gl.enable(glow::SCISSOR_TEST);
        }
        gl.disable(glow::DEPTH_TEST);
        gl.enable(glow::BLEND);
        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
        gl.use_program(Some(t.program));
        gl.bind_vertex_array(Some(t.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(t.vbo));
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(t.color));
        gl.uniform_1_i32(gl.get_uniform_location(t.program, "u_scene").as_ref(), 0);
        gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        gl.disable_vertex_attrib_array(0);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.use_program(None);
    }
    true
}

fn build_scale_target(gl: &glow::Context, size: (u32, u32)) -> Result<ScaleTarget, String> {
    let version = egui_glow::ShaderVersion::get(gl);
    let new = version.is_new_shader_interface();
    let (attr, vout, fin) = if new { ("in", "out", "in") } else { ("attribute", "varying", "varying") };
    let precision = if version.is_embedded() { "precision mediump float;\n" } else { "" };
    let sample = if new { "texture" } else { "texture2D" };
    let out_decl = if new { "out vec4 out_color;\n" } else { "" };
    let out_name = if new { "out_color" } else { "gl_FragColor" };
    let vs = format!(
        "{}{precision}{attr} vec2 a_pos;\n{vout} vec2 v_uv;\nvoid main() {{ v_uv = a_pos * 0.5 + 0.5; gl_Position = vec4(a_pos, 0.0, 1.0); }}\n",
        version.version_declaration()
    );
    let fs = format!(
        "{}{precision}{fin} vec2 v_uv;\n{out_decl}uniform sampler2D u_scene;\nvoid main() {{ {out_name} = {sample}(u_scene, v_uv); }}\n",
        version.version_declaration()
    );
    unsafe {
        let color = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(color));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            size.0 as i32,
            size.1 as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        for (name, value) in [
            (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
            (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
            (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
            (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, name, value as i32);
        }
        gl.bind_texture(glow::TEXTURE_2D, None);
        let depth = gl.create_renderbuffer()?;
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
        gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, size.0 as i32, size.1 as i32);
        gl.bind_renderbuffer(glow::RENDERBUFFER, None);
        let previous = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        let fbo = gl.create_framebuffer()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color), 0);
        gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(depth));
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        gl.bind_framebuffer(glow::FRAMEBUFFER, previous);
        if status != glow::FRAMEBUFFER_COMPLETE {
            return Err(format!("framebuffer status {status:#x}"));
        }
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
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        let quad: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
        let bytes = std::slice::from_raw_parts(quad.as_ptr().cast::<u8>(), std::mem::size_of_val(&quad));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        Ok(ScaleTarget { fbo, color, depth, size, program, vao, vbo })
    }
}

/// Sky cubemap behind the scene. The grid and meshes are painted after this.
pub fn sky_callback(rect: Rect, frame: GpuFrame) -> egui::PaintCallback {
    let spread = sky_spread(rect, crate::viewport_mesh::look_through().map(|look| look.focal_px));
    if crate::gpu_viewport_wgpu::active() {
        return crate::gpu_mesh_pass::sky_callback(rect, frame);
    }
    let frame = Arc::new(frame);
    egui::PaintCallback {
        rect,
        callback: Arc::new(egui_glow::CallbackFn::new(move |_info, painter| {
            paint_sky(painter.gl(), &frame, spread);
        })),
    }
}

/// How far the sky view reaches sideways and up at the view's edges (tangent of half
/// the field of view). Through a play camera it matches the camera's lens exactly.
pub fn sky_spread(rect: Rect, focal_px: Option<f32>) -> [f32; 2] {
    let half_h = (rect.height() * 0.5).max(1.0);
    let aspect = rect.width().max(1.0) / rect.height().max(1.0);
    let up = match focal_px {
        Some(focal) => half_h / focal.max(1e-3),
        None => 0.4,
    };
    [up * aspect, up]
}

struct SkyState {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
}

static SKY: Mutex<Option<SkyState>> = Mutex::new(None);
static SKY_FAILED: AtomicBool = AtomicBool::new(false);

fn paint_sky(gl: &glow::Context, frame: &GpuFrame, spread: [f32; 2]) {
    if frame.sky.is_empty() || SKY_FAILED.load(Ordering::Relaxed) {
        return;
    }
    let Some(tex) = crate::gpu_textures::bind_sky(gl, frame.project.as_deref(), &frame.sky) else {
        return;
    };
    let mut guard = SKY.lock().unwrap_or_else(|err| err.into_inner());
    if guard.is_none() {
        match build_sky(gl) {
            Ok(state) => *guard = Some(state),
            Err(err) => {
                eprintln!("sky cubemap could not start: {err}");
                SKY_FAILED.store(true, Ordering::Relaxed);
                return;
            }
        }
    }
    let Some(state) = guard.as_ref() else { return };
    unsafe {
        gl.use_program(Some(state.program));
        gl.bind_vertex_array(Some(state.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.vbo));
        let quad: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
        let bytes = std::slice::from_raw_parts(quad.as_ptr().cast::<u8>(), std::mem::size_of_val(&quad));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(tex.target, Some(tex.texture));
        gl.uniform_1_i32(gl.get_uniform_location(state.program, "u_sky").as_ref(), 0);
        gl.uniform_1_f32(gl.get_uniform_location(state.program, "u_yaw").as_ref(), frame.yaw_deg);
        gl.uniform_1_f32(gl.get_uniform_location(state.program, "u_pitch").as_ref(), frame.pitch_deg);
        gl.uniform_2_f32(gl.get_uniform_location(state.program, "u_spread").as_ref(), spread[0], spread[1]);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::BLEND);
        gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        gl.disable_vertex_attrib_array(0);
        gl.bind_texture(tex.target, None);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.use_program(None);
    }
}

fn build_sky(gl: &glow::Context) -> Result<SkyState, String> {
    let version = egui_glow::ShaderVersion::get(gl);
    let new = version.is_new_shader_interface();
    let (attr, vout, fin) = if new { ("in", "out", "in") } else { ("attribute", "varying", "varying") };
    let precision = if version.is_embedded() { "precision mediump float;\n" } else { "" };
    let sample = if new { "texture(u_sky, dir)" } else { "textureCube(u_sky, dir)" };
    let vs = format!(
        "{}{precision}{attr} vec2 a_pos;\n{vout} vec2 v_ndc;\nvoid main() {{ v_ndc = a_pos; gl_Position = vec4(a_pos, 1.0, 1.0); }}\n",
        version.version_declaration()
    );
    let out_decl = if new { "out vec4 out_color;\n" } else { "" };
    let out_name = if new { "out_color" } else { "gl_FragColor" };
    let fs = format!(
        "{}{precision}{fin} vec2 v_ndc;\n{out_decl}uniform samplerCube u_sky;\nuniform float u_yaw;\nuniform float u_pitch;\nuniform vec2 u_spread;\n\
         void main() {{\n\
           float yaw = radians(u_yaw);\n\
           float pitch = radians(u_pitch);\n\
           vec3 forward = vec3(sin(yaw) * cos(pitch), sin(pitch), cos(yaw) * cos(pitch));\n\
           vec3 right = vec3(cos(yaw), 0.0, -sin(yaw));\n\
           vec3 up = cross(forward, right);\n\
           vec3 dir = normalize(forward + right * v_ndc.x * u_spread.x + up * v_ndc.y * u_spread.y);\n\
           {out_name} = {sample};\n\
         }}\n",
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
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        Ok(SkyState { program, vao, vbo })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, Pos2};

    #[test]
    fn vertices_map_the_viewport_to_clip_space_with_near_depth_first() {
        let rect = Rect::from_min_max(Pos2::new(100.0, 50.0), Pos2::new(300.0, 250.0));
        let tri = |depth: f32| LitTriangle {
            points: [Pos2::new(100.0, 50.0), Pos2::new(300.0, 50.0), Pos2::new(300.0, 250.0)],
            depth,
            depths: [depth; 3],
            color: Color32::from_rgb(255, 0, 0),
            dark: Color32::from_rgb(64, 0, 0),
            shadow_uvz: [[0.25, 0.5, 0.75]; 3],
            shadow_bias: 0.01,
            uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
            tex_layer: 0,
            tex_slice: 0.0,
            tex_path: String::new(),
            tex_kind: crate::gpu_textures::SampleKind::None,
            normal: [0.0, 1.0, 0.0],
            light: 1.0,
        };
        let (data, batches) = vertex_data(&[tri(2.0), tri(10.0)], rect);
        assert_eq!(batches.len(), 1);
        assert!(batches[0].path.is_empty());
        let mut flat = tri(1.0);
        flat.tex_path = "wood.png".into();
        flat.tex_kind = crate::gpu_textures::SampleKind::D2;
        let mut array = tri(1.0);
        array.tex_path = "wood.texture".into();
        array.tex_kind = crate::gpu_textures::SampleKind::Array;
        array.tex_layer = 2;
        let (_, split) = vertex_data(&[flat, array], rect);
        assert_eq!(split.len(), 2);
        assert_ne!(split[0].kind, split[1].kind);
        assert_eq!(data.len(), 2 * 3 * STRIDE);
        assert_eq!(&data[0..2], &[-1.0, 1.0], "top left corner");
        assert_eq!(&data[STRIDE * 2..STRIDE * 2 + 2], &[1.0, -1.0], "bottom right corner");
        assert!(data[2] < 0.0 && data[3 * STRIDE + 2] > 0.0, "nearer is smaller depth");
        assert_eq!(&data[3..7], &[1.0, 0.0, 0.0, 1.0]);
        assert_eq!(&data[11..15], &[0.25, 0.5, 0.75, 0.01]);
    }

    #[test]
    fn packed_shadow_depths_keep_24_bits_and_empty_texels_read_farthest() {
        let packed = pack_shadow(&[0.5, f32::INFINITY, 0.0]);
        let read = |px: &[u8]| {
            (f32::from(px[0]) * 65536.0 + f32::from(px[1]) * 256.0 + f32::from(px[2]))
                / 16_777_215.0
        };
        assert!((read(&packed[0..4]) - 0.5).abs() < 1e-6);
        assert_eq!(read(&packed[4..8]), 1.0);
        assert_eq!(read(&packed[8..12]), 0.0);
    }
}

#[cfg(test)]
mod scale_and_sky_tests {
    use super::{scaled_size, sky_spread};

    #[test]
    fn render_scale_shrinks_the_scene_image_and_the_sky_matches_the_lens() {
        assert_eq!(scaled_size(1280, 720, 100), (1280, 720));
        assert_eq!(scaled_size(1280, 720, 50), (640, 360));
        assert_eq!(scaled_size(1280, 720, 10), (320, 180)); // clamped to 25%
        assert_eq!(scaled_size(1, 1, 25), (1, 1));
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 400.0));
        // A 90 degree vertical field of view: focal length equals half the height.
        let [x, y] = sky_spread(rect, Some(200.0));
        assert!((y - 1.0).abs() < 1e-5 && (x - 2.0).abs() < 1e-5);
    }
}
