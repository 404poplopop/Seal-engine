//! Uploads pictures, texture arrays, volumes and cubemaps for the viewport.
//!
//! The decision of which target, which block format and how many bytes to send
//! is plain data, so tests cover it without a graphics card. The card calls
//! stay thin. Without a card the mesh keeps its flat colour.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use eframe::egui_glow;
use eframe::glow::{self, HasContext};
use seal_editor_core::{
    decode_block_to_rgba8, encode_export_rgba, picture, GpuBlockFormat, TextureAsset,
    TextureAssetKind,
};

/// Shown in the manual: the flat colour is what you get when the card is unavailable.
pub const CPU_FALLBACK: &str = "Without a graphics card the mesh keeps its flat colour. Array, volume and cubemap textures, anisotropic filtering and the sky cubemap are sampled only on the graphics card.";

/// Binding target for one texture asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuTextureTarget {
    D2,
    Array2d,
    Volume,
    Cubemap,
}

impl GpuTextureTarget {
    pub fn gl(self) -> u32 {
        match self {
            Self::D2 => glow::TEXTURE_2D,
            Self::Array2d => glow::TEXTURE_2D_ARRAY,
            Self::Volume => glow::TEXTURE_3D,
            Self::Cubemap => glow::TEXTURE_CUBE_MAP,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::D2 => "2d",
            Self::Array2d => "array",
            Self::Volume => "volume",
            Self::Cubemap => "cubemap",
        }
    }
}

/// How the mesh shader reads a bound texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SampleKind {
    None,
    D2,
    Array,
    Volume,
    Cube,
}

impl SampleKind {
    pub fn code(self) -> i32 {
        match self {
            Self::None => 0,
            Self::D2 => 1,
            Self::Array => 2,
            Self::Volume => 3,
            Self::Cube => 4,
        }
    }
}

/// What `upload_compressed` will do for a format on a given card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UploadPath {
    /// Uncompressed RGBA8 image.
    Rgba8,
    /// `glCompressedTexImage*` with the block format.
    Compressed,
    /// Block bytes are expanded to RGBA8 first.
    DecodeRgba8,
}

/// Base colour reference carried on a viewport mark. An empty path keeps the flat colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshSurface<'a> {
    pub path: Option<&'a str>,
    pub layer: u32,
    pub slice: f32,
    pub tile: [f32; 2],
    pub kind: SampleKind,
}

impl MeshSurface<'static> {
    pub const EMPTY: Self =
        Self { path: None, layer: 0, slice: 0.0, tile: [1.0, 1.0], kind: SampleKind::None };
}

pub fn target_for_kind(kind: TextureAssetKind) -> GpuTextureTarget {
    match kind {
        TextureAssetKind::Array2d => GpuTextureTarget::Array2d,
        TextureAssetKind::Volume3d => GpuTextureTarget::Volume,
        TextureAssetKind::Cubemap => GpuTextureTarget::Cubemap,
    }
}

pub fn target_for_picture() -> GpuTextureTarget {
    GpuTextureTarget::D2
}

/// Layers stored for a kind: picture count for an array, depth for a volume, six for a cubemap.
pub fn layer_count(kind: TextureAssetKind, pictures: usize, depth: u32) -> u32 {
    match kind {
        TextureAssetKind::Array2d => pictures.max(1) as u32,
        TextureAssetKind::Volume3d => depth.max(1),
        TextureAssetKind::Cubemap => 6,
    }
}

pub fn parse_gl_version(text: &str) -> (u32, u32) {
    let start = text.find(|c: char| c.is_ascii_digit()).unwrap_or(text.len());
    let mut parts = text[start..].split(|c: char| !c.is_ascii_digit());
    let major = parts.next().unwrap_or("0").parse().unwrap_or(0);
    let minor = parts.next().unwrap_or("0").parse().unwrap_or(0);
    (major, minor)
}

pub fn extension_present(extensions: &[impl AsRef<str>], token: &str) -> bool {
    let token = token.strip_prefix("GL_").unwrap_or(token);
    extensions.iter().any(|ext| {
        let ext = ext.as_ref();
        let bare = ext.strip_prefix("GL_").unwrap_or(ext);
        bare.eq_ignore_ascii_case(token)
    })
}

fn has_any(extensions: &[impl AsRef<str>], tokens: &[&str]) -> bool {
    tokens.iter().any(|token| extension_present(extensions, token))
}

/// Block formats the card can take compressed, from an extension list and a version.
pub fn format_supported(
    format: GpuBlockFormat,
    extensions: &[impl AsRef<str>],
    major: u32,
    minor: u32,
    es: bool,
) -> bool {
    match format {
        GpuBlockFormat::Rgba8 => true,
        GpuBlockFormat::Bc1 => {
            has_any(extensions, &["EXT_texture_compression_s3tc", "EXT_texture_compression_dxt1"])
        }
        GpuBlockFormat::Bc2 | GpuBlockFormat::Bc3 => {
            has_any(extensions, &["EXT_texture_compression_s3tc"])
        }
        GpuBlockFormat::Bc4 | GpuBlockFormat::Bc5 => {
            has_any(extensions, &["ARB_texture_compression_rgtc", "EXT_texture_compression_rgtc"])
        }
        GpuBlockFormat::Bc6h | GpuBlockFormat::Bc7 => {
            has_any(extensions, &["ARB_texture_compression_bptc", "EXT_texture_compression_bptc"])
        }
        GpuBlockFormat::Etc2 => {
            let gl43 = !es && (major > 4 || (major == 4 && minor >= 3));
            let es3 = es && major >= 3;
            gl43 || es3
                || has_any(
                    extensions,
                    &["ARB_ES3_compatibility", "OES_compressed_ETC2_RGB8_texture"],
                )
        }
        GpuBlockFormat::Astc4x4 => has_any(
            extensions,
            &[
                "KHR_texture_compression_astc_ldr",
                "OES_texture_compression_astc",
                "KHR_texture_compression_astc_hdr",
            ],
        ),
    }
}

pub fn upload_path(
    format: GpuBlockFormat,
    extensions: &[impl AsRef<str>],
    major: u32,
    minor: u32,
    es: bool,
) -> UploadPath {
    if !format.is_block_compressed() {
        return UploadPath::Rgba8;
    }
    if format_supported(format, extensions, major, minor, es) {
        UploadPath::Compressed
    } else {
        UploadPath::DecodeRgba8
    }
}

/// Block width, block height and bytes in one block.
pub fn block_footprint(format: GpuBlockFormat) -> (u32, u32, usize) {
    match format {
        GpuBlockFormat::Rgba8 => (1, 1, 4),
        GpuBlockFormat::Bc1 | GpuBlockFormat::Bc4 | GpuBlockFormat::Etc2 => (4, 4, 8),
        GpuBlockFormat::Bc2
        | GpuBlockFormat::Bc3
        | GpuBlockFormat::Bc5
        | GpuBlockFormat::Bc6h
        | GpuBlockFormat::Bc7
        | GpuBlockFormat::Astc4x4 => (4, 4, 16),
    }
}

pub fn level_byte_size(format: GpuBlockFormat, width: u32, height: u32) -> usize {
    let (bw, bh, bytes) = block_footprint(format);
    let blocks_x = width.max(1).div_ceil(bw);
    let blocks_y = height.max(1).div_ceil(bh);
    blocks_x as usize * blocks_y as usize * bytes
}

/// Bytes for `layers` images across `mips` levels (mip 0 is `width` by `height`).
pub fn upload_byte_size(
    format: GpuBlockFormat,
    width: u32,
    height: u32,
    layers: u32,
    mips: u32,
) -> usize {
    let mut total = 0usize;
    let mut w = width.max(1);
    let mut h = height.max(1);
    let levels = mips.max(1);
    let layers = layers.max(1) as usize;
    for level in 0..levels {
        total += level_byte_size(format, w, h) * layers;
        if w == 1 && h == 1 {
            let rest = (levels - level - 1) as usize;
            total += level_byte_size(format, 1, 1) * layers * rest;
            break;
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    total
}

pub fn compressed_internal_format(format: GpuBlockFormat) -> Option<u32> {
    Some(match format {
        GpuBlockFormat::Rgba8 => return None,
        GpuBlockFormat::Bc1 => 0x83F1,
        GpuBlockFormat::Bc2 => 0x83F2,
        GpuBlockFormat::Bc3 => 0x83F3,
        GpuBlockFormat::Bc4 => 0x8DBB,
        GpuBlockFormat::Bc5 => 0x8DBD,
        GpuBlockFormat::Bc6h => 0x8E8F,
        GpuBlockFormat::Bc7 => 0x8E8C,
        GpuBlockFormat::Etc2 => 0x9274,
        GpuBlockFormat::Astc4x4 => 0x93B0,
    })
}

pub fn anisotropy_level(requested: u32, max_supported: f32, extension: bool) -> f32 {
    if !extension {
        return 1.0;
    }
    let max = if max_supported.is_finite() && max_supported >= 1.0 { max_supported } else { 1.0 };
    (requested.clamp(1, 16) as f32).min(max).max(1.0)
}

pub fn tile_uv(uv: [f32; 2], tile: [f32; 2]) -> [f32; 2] {
    [uv[0] * tile[0], uv[1] * tile[1]]
}

/// Unit-square corners of one box or plane face, then scaled by `tile`.
pub fn quad_uvs(tile: [f32; 2]) -> [[f32; 2]; 4] {
    [
        tile_uv([0.0, 0.0], tile),
        tile_uv([1.0, 0.0], tile),
        tile_uv([1.0, 1.0], tile),
        tile_uv([0.0, 1.0], tile),
    ]
}

/// Longitude / latitude on a sphere, cylinder side or cone (`seg` around, `ring` down).
pub fn lat_long_uv(seg: u32, segments: u32, ring: u32, rings: u32, tile: [f32; 2]) -> [f32; 2] {
    tile_uv([seg as f32 / segments.max(1) as f32, ring as f32 / rings.max(1) as f32], tile)
}

/// Disc coordinate for a cylinder cap or cone base, `x` and `z` relative to the centre.
pub fn planar_xz_uv(x: f32, z: f32, radius: f32, tile: [f32; 2]) -> [f32; 2] {
    let r = radius.abs().max(1.0e-6);
    tile_uv([(x / r + 1.0) * 0.5, (z / r + 1.0) * 0.5], tile)
}

/// One corner set per primitive, used by tests and by the mesh builder.
pub fn primitive_corner_uvs(primitive: &str, tile: [f32; 2]) -> [[f32; 2]; 4] {
    match primitive {
        "sphere" => [
            lat_long_uv(0, 12, 0, 8, tile),
            lat_long_uv(1, 12, 0, 8, tile),
            lat_long_uv(1, 12, 1, 8, tile),
            lat_long_uv(0, 12, 1, 8, tile),
        ],
        "cylinder" | "cone" | "capsule" => [
            lat_long_uv(0, 16, 0, 1, tile),
            lat_long_uv(1, 16, 0, 1, tile),
            lat_long_uv(1, 16, 1, 1, tile),
            lat_long_uv(0, 16, 1, 1, tile),
        ],
        _ => quad_uvs(tile),
    }
}

/// Sampler a picture or a texture asset uses on a card with array, volume and cubemap samplers.
pub fn sample_kind_for_texture(kind: TextureAssetKind) -> SampleKind {
    match kind {
        TextureAssetKind::Array2d => SampleKind::Array,
        TextureAssetKind::Volume3d => SampleKind::Volume,
        TextureAssetKind::Cubemap => SampleKind::Cube,
    }
}

pub fn sample_kind_for_picture() -> SampleKind {
    SampleKind::D2
}

/// Volume texture coordinate. `slice` is the slice index and `depth` is the slice count.
/// The half-texel shift sits in the middle of that slice. A later animation can move `slice`.
pub fn volume_w(slice: f32, depth: u32) -> f32 {
    (slice + 0.5) / (depth.max(1) as f32)
}

/// Shading language 3 and GLES 3 can sample arrays, volumes and cubemaps.
/// Version 1.x and GLES 2 cannot, so those textures use a flat copy of one layer.
pub fn real_texture_samplers(shading_language: &str) -> bool {
    let start =
        shading_language.find(|c: char| c.is_ascii_digit()).unwrap_or(shading_language.len());
    let ver = shading_language[start..].split([' ', '\0']).next().unwrap_or("0");
    let major: u32 = ver.split('.').next().unwrap_or("0").parse().unwrap_or(0);
    // Version 1.x and GLES 2 stay on a flat copy. Language 3 and GLES 3 sample the real objects.
    major >= 3
}

/// Vertex and fragment source for one mesh sampler. Tests read this text; the card compiles the same strings.
pub fn mesh_shader_source(kind: SampleKind, new_interface: bool) -> (String, String) {
    let (attr, vout, fin) =
        if new_interface { ("in", "out", "in") } else { ("attribute", "varying", "varying") };
    let version = if new_interface { "#version 140\n" } else { "#version 120\n" };
    let sample_2d = if new_interface { "texture" } else { "texture2D" };
    let vertex = format!(
        "{version}{attr} vec3 a_pos;\n{attr} vec4 a_color;\n{attr} vec4 a_dark;\n{attr} vec4 a_shadow;\n{attr} vec4 a_tex;\n{attr} vec3 a_normal;\n\
         {vout} vec4 v_color;\n{vout} vec4 v_dark;\n{vout} vec4 v_shadow;\n{vout} vec4 v_tex;\n{vout} vec3 v_normal;\n\
         void main() {{ v_color = a_color; v_dark = a_dark; v_shadow = a_shadow; v_tex = a_tex; v_normal = a_normal; gl_Position = vec4(a_pos, 1.0); }}\n"
    );
    let (albedo_uniform, albedo_sample) = match kind {
        SampleKind::Array => {
            ("uniform sampler2DArray u_albedo;\n", "texture(u_albedo, vec3(v_tex.xy, v_tex.z))")
        }
        SampleKind::Volume => (
            "uniform sampler3D u_albedo;\nuniform float u_depth;\n",
            "texture(u_albedo, vec3(v_tex.xy, (v_tex.w + 0.5) / u_depth))",
        ),
        SampleKind::Cube => (
            "uniform samplerCube u_albedo;\n",
            if new_interface {
                "texture(u_albedo, normalize(v_normal))"
            } else {
                "textureCube(u_albedo, normalize(v_normal))"
            },
        ),
        SampleKind::None | SampleKind::D2 => (
            "uniform sampler2D u_albedo;\n",
            if new_interface {
                "texture(u_albedo, v_tex.xy)"
            } else {
                "texture2D(u_albedo, v_tex.xy)"
            },
        ),
    };
    let out_decl = if new_interface { "out vec4 out_color;\n" } else { "" };
    let out_name = if new_interface { "out_color" } else { "gl_FragColor" };
    let fragment = format!(
        "{version}{fin} vec4 v_color;\n{fin} vec4 v_dark;\n{fin} vec4 v_shadow;\n{fin} vec4 v_tex;\n{fin} vec3 v_normal;\n{out_decl}\
         uniform sampler2D u_shadow;\n{albedo_uniform}uniform float u_texel;\nuniform float u_has_shadow;\nuniform float u_has_albedo;\n\
         float nearest(vec2 uv) {{ vec3 c = {sample_2d}(u_shadow, uv).rgb; return dot(c, vec3(65536.0, 256.0, 1.0)) * 255.0 / 16777215.0; }}\n\
         void main() {{\n\
           float sun = 1.0;\n\
           if (u_has_shadow > 0.5 && v_shadow.w > 0.0) {{\n\
             vec2 grid = v_shadow.xy / u_texel - 0.5;\n\
             vec2 cell = floor(grid);\n\
             vec2 f = grid - cell;\n\
             float lit = 0.0;\n\
             for (int j = 0; j < 4; j++) {{ for (int i = 0; i < 4; i++) {{\n\
               float wx = i == 0 ? 1.0 - f.x : (i == 3 ? f.x : 1.0);\n\
               float wy = j == 0 ? 1.0 - f.y : (j == 3 ? f.y : 1.0);\n\
               vec2 uv = (cell + vec2(float(i) - 1.0, float(j) - 1.0) + 0.5) * u_texel;\n\
               float hit = (uv.x < 0.0 || uv.y < 0.0 || uv.x >= 1.0 || uv.y >= 1.0 || v_shadow.z - v_shadow.w <= nearest(uv)) ? 1.0 : 0.0;\n\
               lit += wx * wy * hit;\n\
             }} }}\n\
             sun = lit / 9.0;\n\
           }}\n\
           vec4 lit = mix(v_dark, v_color, sun);\n\
           vec4 albedo = vec4(1.0);\n\
           if (u_has_albedo > 0.5) {{ albedo = {albedo_sample}; }}\n\
           {out_name} = vec4(lit.rgb * albedo.rgb, lit.a * albedo.a);\n\
         }}\n"
    );
    (vertex, fragment)
}

/// Texture texel times the lit object colour (both straight, 0 to 1 in the shader).
pub fn modulate(texel: [u8; 4], lit: [f32; 4]) -> [f32; 4] {
    [
        f32::from(texel[0]) / 255.0 * lit[0],
        f32::from(texel[1]) / 255.0 * lit[1],
        f32::from(texel[2]) / 255.0 * lit[2],
        f32::from(texel[3]) / 255.0 * lit[3],
    ]
}

fn gl_info_parts(gl: &glow::Context) -> (Vec<String>, u32, u32, bool) {
    let extensions: Vec<String> = gl.supported_extensions().iter().cloned().collect();
    let version = gl.version();
    (extensions, version.major, version.minor, version.is_embedded)
}

fn decode_layers(
    format: GpuBlockFormat,
    width: u32,
    height: u32,
    layers: u32,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    let one = level_byte_size(format, width, height);
    let layers = layers.max(1) as usize;
    if data.len() < one * layers {
        return Err(format!(
            "block texture is {} bytes, {layers} layers of {width}x{height} {} need {}",
            data.len(),
            format.name(),
            one * layers
        ));
    }
    let mut out = Vec::with_capacity((width.max(1) * height.max(1) * 4) as usize * layers);
    for layer in 0..layers {
        let slice = &data[layer * one..(layer + 1) * one];
        match decode_block_to_rgba8(format, width.max(1), height.max(1), slice) {
            Ok(px) => out.extend(px),
            Err(_) => out.extend(stand_in_rgba(width, height, slice)),
        }
    }
    Ok(out)
}

/// RGBA8 stand-in when this format has no software decoder. Size matches the image.
fn stand_in_rgba(width: u32, height: u32, block: &[u8]) -> Vec<u8> {
    let (r, g, b) = match block {
        [a, b, c, ..] => (*a, *b, *c),
        [a, b] => (*a, *b, 0),
        [a] => (*a, *a, *a),
        _ => (128, 128, 128),
    };
    let n = (width.max(1) * height.max(1)) as usize;
    let mut out = Vec::with_capacity(n * 4);
    for _ in 0..n {
        out.extend_from_slice(&[r, g, b, 255]);
    }
    out
}

/// Upload `data` into the bound texture. Block formats use a compressed image call
/// when the card lists the matching extension; otherwise the bytes are decoded to RGBA8.
pub fn upload_compressed(
    gl: &glow::Context,
    target: u32,
    format: GpuBlockFormat,
    width: u32,
    height: u32,
    layers: u32,
    data: &[u8],
) -> Result<(), String> {
    let (extensions, major, minor, es) = gl_info_parts(gl);
    let path = upload_path(format, &extensions, major, minor, es);
    let decoded;
    let (format, bytes) = if path == UploadPath::DecodeRgba8 {
        decoded = decode_layers(format, width, height, layers, data)?;
        (GpuBlockFormat::Rgba8, decoded.as_slice())
    } else if format.is_block_compressed() {
        let need = level_byte_size(format, width, height) * layers.max(1) as usize;
        if data.len() < need {
            return Err(format!("compressed texture is {} bytes, need {need}", data.len()));
        }
        (format, &data[..need])
    } else {
        let need = level_byte_size(GpuBlockFormat::Rgba8, width, height) * layers.max(1) as usize;
        if data.len() < need {
            return Err(format!("rgba texture is {} bytes, need {need}", data.len()));
        }
        (GpuBlockFormat::Rgba8, &data[..need])
    };
    unsafe { upload_bytes(gl, target, format, width, height, layers, bytes) }
}

unsafe fn upload_bytes(
    gl: &glow::Context,
    target: u32,
    format: GpuBlockFormat,
    width: u32,
    height: u32,
    layers: u32,
    data: &[u8],
) -> Result<(), String> {
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    let w = width.max(1) as i32;
    let h = height.max(1) as i32;
    let layers = layers.max(1);
    if format.is_block_compressed() {
        let internal =
            compressed_internal_format(format).ok_or("rgba is not a block format")? as i32;
        if target == glow::TEXTURE_CUBE_MAP {
            let faces = 6usize;
            if layers < 6 {
                return Err("a cubemap needs 6 faces".into());
            }
            let one = data.len() / faces;
            for face in 0..faces {
                gl.compressed_tex_image_2d(
                    glow::TEXTURE_CUBE_MAP_POSITIVE_X + face as u32,
                    0,
                    internal,
                    w,
                    h,
                    0,
                    one as i32,
                    &data[face * one..(face + 1) * one],
                );
            }
        } else if target == glow::TEXTURE_2D_ARRAY || target == glow::TEXTURE_3D {
            gl.compressed_tex_image_3d(
                target,
                0,
                internal,
                w,
                h,
                layers as i32,
                0,
                data.len() as i32,
                data,
            );
        } else {
            gl.compressed_tex_image_2d(target, 0, internal, w, h, 0, data.len() as i32, data);
        }
    } else if target == glow::TEXTURE_CUBE_MAP {
        if layers < 6 {
            return Err("a cubemap needs 6 faces".into());
        }
        let one = (w * h * 4) as usize;
        if data.len() < one * 6 {
            return Err("cubemap rgba is short".into());
        }
        for face in 0..6 {
            gl.tex_image_2d(
                glow::TEXTURE_CUBE_MAP_POSITIVE_X + face as u32,
                0,
                glow::RGBA as i32,
                w,
                h,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&data[face * one..(face + 1) * one])),
            );
        }
    } else if target == glow::TEXTURE_2D_ARRAY || target == glow::TEXTURE_3D {
        gl.tex_image_3d(
            target,
            0,
            glow::RGBA as i32,
            w,
            h,
            layers as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(data)),
        );
    } else {
        gl.tex_image_2d(
            target,
            0,
            glow::RGBA as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(data)),
        );
    }
    Ok(())
}

pub fn apply_anisotropy(gl: &glow::Context, target: u32, requested: u32) {
    let extensions: Vec<String> = gl.supported_extensions().iter().cloned().collect();
    let present = extension_present(&extensions, "EXT_texture_filter_anisotropic");
    if !present {
        return;
    }
    let max = unsafe { gl.get_parameter_f32(glow::MAX_TEXTURE_MAX_ANISOTROPY_EXT) };
    let level = anisotropy_level(requested, max, true);
    unsafe { gl.tex_parameter_f32(target, glow::TEXTURE_MAX_ANISOTROPY_EXT, level) };
}

fn linear_params(gl: &glow::Context, target: u32, anisotropy: u32, repeat: bool, mip: bool) {
    let min = if mip { glow::LINEAR_MIPMAP_LINEAR } else { glow::LINEAR };
    let wrap = if repeat { glow::REPEAT } else { glow::CLAMP_TO_EDGE };
    unsafe {
        gl.tex_parameter_i32(target, glow::TEXTURE_MIN_FILTER, min as i32);
        gl.tex_parameter_i32(target, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(target, glow::TEXTURE_WRAP_S, wrap as i32);
        gl.tex_parameter_i32(target, glow::TEXTURE_WRAP_T, wrap as i32);
        if target == glow::TEXTURE_3D || target == glow::TEXTURE_CUBE_MAP {
            gl.tex_parameter_i32(target, glow::TEXTURE_WRAP_R, wrap as i32);
        }
        if mip {
            gl.generate_mipmap(target);
        }
    }
    apply_anisotropy(gl, target, anisotropy);
}

struct Loaded {
    kind: SampleKind,
    format: GpuBlockFormat,
    width: u32,
    height: u32,
    layers: u32,
    rgba: Vec<u8>,
    #[allow(dead_code)]
    mtime: Option<SystemTime>,
    /// Level-0 blocks of every layer in `format`, from the `.layerN.ktx2` files that
    /// Encode for export wrote (when all are present and newer than their pictures).
    blocks: Option<Vec<u8>>,
}

/// Blocks for every layer from `<stem>.layerN.ktx2` beside the texture asset, when each
/// one exists, is in `format`, matches the size and is not older than its picture.
fn encoded_layers(
    asset_file: &Path,
    pictures: &[PathBuf],
    format: GpuBlockFormat,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    let stem = asset_file.file_stem()?.to_str()?;
    let dir = asset_file.parent()?;
    let mut out = Vec::new();
    for (layer, picture) in pictures.iter().enumerate() {
        let file = dir.join(format!("{stem}.layer{layer}.ktx2"));
        if stamp(&file)? < stamp(picture)? {
            return None;
        }
        let image = seal_editor_core::read_ktx2(&std::fs::read(&file).ok()?).ok()?;
        if image.format != format || image.width != width || image.height != height {
            return None;
        }
        out.extend(image.levels.first()?);
    }
    Some(out)
}

fn stamp(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn resolve(root: Option<&Path>, path: &str) -> PathBuf {
    let raw = Path::new(path);
    match root {
        Some(root) if raw.is_relative() => root.join(raw),
        _ => raw.to_path_buf(),
    }
}

fn load_picture(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let pic = picture::decode_path(path).map_err(|err| err.message)?;
    if pic.rgba.len() != (pic.width * pic.height * 4) as usize {
        return Err(format!("damaged picture {}", path.display()));
    }
    Ok((pic.width.max(1), pic.height.max(1), pic.rgba))
}

fn load_material(root: Option<&Path>, rel: &str) -> Result<Loaded, String> {
    let full = resolve(root, rel);
    if !full.is_file() {
        return Err(format!("missing texture {rel}"));
    }
    let mtime = stamp(&full);
    if rel.rsplit('.').next().is_some_and(|ext| ext.eq_ignore_ascii_case("texture")) {
        let text = std::fs::read_to_string(&full).map_err(|err| err.to_string())?;
        let asset = TextureAsset::parse(&text).map_err(|err| err.to_string())?;
        if asset.pictures.is_empty() {
            return Err(format!("texture {rel} has no pictures"));
        }
        let mut rgba = Vec::new();
        let mut width = asset.width.max(1);
        let mut height = asset.height.max(1);
        let mut files = Vec::new();
        for (index, picture_path) in asset.pictures.iter().enumerate() {
            if picture_path.is_empty() {
                return Err(format!("texture {rel} picture {} is empty", index + 1));
            }
            let file = resolve(root, picture_path);
            files.push(file.clone());
            let (w, h, pixels) = load_picture(&file)?;
            if index == 0 {
                width = w;
                height = h;
            } else if w != width || h != height {
                return Err(format!(
                    "texture {rel} picture {} is {w}x{h}, expected {width}x{height}",
                    index + 1
                ));
            }
            rgba.extend(pixels);
        }
        let kind = match asset.kind {
            TextureAssetKind::Array2d => SampleKind::Array,
            TextureAssetKind::Volume3d => SampleKind::Volume,
            TextureAssetKind::Cubemap => SampleKind::Cube,
        };
        let layers = layer_count(asset.kind, asset.pictures.len(), asset.depth);
        if asset.kind == TextureAssetKind::Cubemap && asset.pictures.len() < 6 {
            return Err(format!("cubemap {rel} needs 6 pictures"));
        }
        let blocks = (asset.gpu_format != GpuBlockFormat::Rgba8)
            .then(|| encoded_layers(&full, &files, asset.gpu_format, width, height))
            .flatten();
        Ok(Loaded { kind, format: asset.gpu_format, width, height, layers, rgba, mtime, blocks })
    } else {
        let (width, height, rgba) = load_picture(&full)?;
        Ok(Loaded {
            kind: SampleKind::D2,
            format: GpuBlockFormat::Rgba8,
            width,
            height,
            layers: 1,
            rgba,
            mtime,
            blocks: None,
        })
    }
}

fn pack_for_card(
    loaded: &Loaded,
    extensions: &[String],
    major: u32,
    minor: u32,
    es: bool,
) -> (GpuBlockFormat, Vec<u8>) {
    let path = upload_path(loaded.format, extensions, major, minor, es);
    if path == UploadPath::Compressed && loaded.format != GpuBlockFormat::Rgba8 {
        let want = level_byte_size(loaded.format, loaded.width, loaded.height)
            * loaded.layers.max(1) as usize;
        // Prefer the blocks Encode for export wrote; otherwise encode once here (fast quality).
        if let Some(blocks) = loaded.blocks.as_ref().filter(|b| b.len() == want) {
            return (loaded.format, blocks.clone());
        }
        let stride = (loaded.width * loaded.height * 4) as usize;
        let mut out = Vec::new();
        for layer in 0..loaded.layers.max(1) {
            let start = layer as usize * stride;
            let Some(pixels) = loaded.rgba.get(start..start + stride) else {
                break;
            };
            match seal_editor_core::encode_block_rgba(
                loaded.format,
                seal_editor_core::EncodeQuality::Fast,
                loaded.width,
                loaded.height,
                pixels,
            ) {
                Ok(blocks) => out.extend(blocks),
                Err(_) => break,
            }
        }
        if out.len() == want {
            return (loaded.format, out);
        }
    }
    (GpuBlockFormat::Rgba8, loaded.rgba.clone())
}

struct Slot {
    key: String,
    texture: glow::Texture,
    target: u32,
    kind: SampleKind,
    slices: f32,
}

fn cache() -> &'static Mutex<Vec<Slot>> {
    static CACHE: Mutex<Vec<Slot>> = Mutex::new(Vec::new());
    &CACHE
}

fn slot_key(rel: &str, layer: u32, slice: i32, view: &str) -> String {
    format!("{rel}|{layer}|{slice}|{view}")
}

/// Bindable texture for a mesh or the sky. `view` is `full` or `layer` (one 2D slice).
pub struct BoundTexture {
    pub texture: glow::Texture,
    pub target: u32,
    pub kind: SampleKind,
    pub slices: f32,
}

pub fn bind_material(
    gl: &glow::Context,
    root: Option<&Path>,
    rel: &str,
    layer: u32,
    slice: f32,
    anisotropy: u32,
    arrays: bool,
) -> Option<BoundTexture> {
    let full_key = slot_key(rel, 0, 0, "full");
    let layer_key = slot_key(rel, layer, slice.to_bits() as i32, "layer");
    {
        let guard = cache().lock().unwrap_or_else(|err| err.into_inner());
        if let Some(found) = guard.iter().find(|slot| {
            if arrays {
                slot.key == full_key
            } else {
                slot.key == layer_key || (slot.key == full_key && slot.kind == SampleKind::D2)
            }
        }) {
            return Some(BoundTexture {
                texture: found.texture,
                target: found.target,
                kind: found.kind,
                slices: found.slices,
            });
        }
    }
    let loaded = match load_material(root, rel) {
        Ok(loaded) => loaded,
        Err(err) => {
            warn_once(rel, &err);
            return None;
        }
    };
    // `arrays` keeps a cubemap, array or volume on its real target. Otherwise the
    // chosen layer, slice or face is a 2D image, after the real target is uploaded.
    let view = if loaded.kind == SampleKind::D2
        || (arrays
            && matches!(loaded.kind, SampleKind::Array | SampleKind::Volume | SampleKind::Cube))
    {
        "full"
    } else {
        "layer"
    };
    let key = slot_key(
        rel,
        if view == "full" { 0 } else { layer },
        if view == "full" { 0 } else { slice.to_bits() as i32 },
        view,
    );
    let mut guard = cache().lock().unwrap_or_else(|err| err.into_inner());
    if view == "layer" && guard.iter().all(|slot| slot.key != slot_key(rel, 0, 0, "full")) {
        drop(guard);
        let _ = upload_slot(gl, rel, &loaded, 0, 0.0, anisotropy, "full");
        guard = cache().lock().unwrap_or_else(|err| err.into_inner());
    }
    if let Some(found) = guard.iter().find(|slot| slot.key == key) {
        return Some(BoundTexture {
            texture: found.texture,
            target: found.target,
            kind: found.kind,
            slices: found.slices,
        });
    }
    drop(guard);
    upload_slot(gl, rel, &loaded, layer, slice, anisotropy, view)
}

fn upload_slot(
    gl: &glow::Context,
    rel: &str,
    loaded: &Loaded,
    layer: u32,
    slice: f32,
    anisotropy: u32,
    view: &str,
) -> Option<BoundTexture> {
    let key = slot_key(
        rel,
        if view == "full" { 0 } else { layer },
        if view == "full" { 0 } else { slice.to_bits() as i32 },
        view,
    );
    let mut guard = cache().lock().unwrap_or_else(|err| err.into_inner());
    if let Some(found) = guard.iter().find(|slot| slot.key == key) {
        return Some(BoundTexture {
            texture: found.texture,
            target: found.target,
            kind: found.kind,
            slices: found.slices,
        });
    }
    let (extensions, major, minor, es) = gl_info_parts(gl);
    let (format, bytes) = if view == "layer" {
        let index = match loaded.kind {
            SampleKind::Volume => slice.max(0.0) as u32,
            _ => layer,
        };
        let stride = (loaded.width * loaded.height * 4) as usize;
        let start = index as usize * stride;
        let end = (start + stride).min(loaded.rgba.len());
        if start >= loaded.rgba.len() || end - start != stride {
            return None;
        }
        (GpuBlockFormat::Rgba8, loaded.rgba[start..end].to_vec())
    } else {
        pack_for_card(loaded, &extensions, major, minor, es)
    };
    let target = if view == "layer" {
        glow::TEXTURE_2D
    } else {
        match loaded.kind {
            SampleKind::Array => glow::TEXTURE_2D_ARRAY,
            SampleKind::Volume => glow::TEXTURE_3D,
            SampleKind::Cube => glow::TEXTURE_CUBE_MAP,
            _ => glow::TEXTURE_2D,
        }
    };
    let layers = if view == "layer" { 1 } else { loaded.layers };
    let texture = unsafe { gl.create_texture().ok()? };
    unsafe { gl.bind_texture(target, Some(texture)) };
    if let Err(err) =
        upload_compressed(gl, target, format, loaded.width, loaded.height, layers, &bytes)
    {
        warn_once(rel, &format!("upload failed: {err}"));
        unsafe {
            gl.bind_texture(target, None);
            gl.delete_texture(texture);
        }
        return None;
    }
    let mip = !format.is_block_compressed();
    let repeat = !matches!(loaded.kind, SampleKind::Cube);
    linear_params(gl, target, anisotropy, repeat, mip);
    unsafe { gl.bind_texture(target, None) };
    let kind = if view == "layer" { SampleKind::D2 } else { loaded.kind };
    let slices = loaded.layers.max(1) as f32;
    guard.push(Slot { key, texture, target, kind, slices });
    Some(BoundTexture { texture, target, kind, slices })
}

pub fn bind_sky(gl: &glow::Context, root: Option<&Path>, rel: &str) -> Option<BoundTexture> {
    // The cached texture is reused every frame; pictures are only read when it is missing.
    let bound = bind_material(gl, root, rel, 0, 0.0, 1, true)?;
    if bound.kind != SampleKind::Cube {
        warn_once(rel, "the sky needs a cubemap texture");
        return None;
    }
    Some(bound)
}

/// One log line per texture and problem, so a broken file does not flood the log.
fn warn_once(rel: &str, why: &str) {
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let key = format!("{rel}: {why}");
    let mut seen = SEEN.lock().unwrap_or_else(|err| err.into_inner());
    if !seen.contains(&key) {
        seal_editor_core::texture_memory::log_texture_warning(&format!("texture {key}"));
        seen.push(key);
    }
}

/// One selected layer of a `.texture`, as RGBA8, for the Asset Editor card preview.
pub fn preview_layer_rgba(
    asset: &TextureAsset,
    pictures: &[Vec<u8>],
    layer: u32,
) -> Result<Vec<u8>, String> {
    let index = layer as usize;
    let pixels = pictures.get(index).ok_or_else(|| format!("no picture {index}"))?;
    let width = asset.width.max(1);
    let height = asset.height.max(1);
    // Prefer the picture's own length when the asset size is still the placeholder.
    if pixels.len() % 4 != 0 || pixels.is_empty() {
        return Err("damaged picture bytes".into());
    }
    let _ = (width, height);
    Ok(pixels.clone())
}

struct PreviewState {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    texture: glow::Texture,
}

static PREVIEW: Mutex<Option<PreviewState>> = Mutex::new(None);

/// Draw `rgba` through an uploaded texture so the Asset Editor shows the card result.
pub fn load_cpu_texture(
    root: Option<&Path>,
    rel: &str,
) -> Result<crate::gpu_wgpu_choice::CpuTexture, String> {
    let loaded = load_material(root, rel)?;
    Ok(crate::gpu_wgpu_choice::CpuTexture {
        kind: loaded.kind,
        format: loaded.format,
        width: loaded.width,
        height: loaded.height,
        layers: loaded.layers,
        rgba: loaded.rgba,
    })
}

pub fn preview_callback(
    rect: egui::Rect,
    rgba: std::sync::Arc<Vec<u8>>,
    width: u32,
    height: u32,
    format: GpuBlockFormat,
) -> egui::PaintCallback {
    if crate::gpu_viewport_wgpu::active() {
        return crate::gpu_mesh_pass::preview_callback(rect, rgba, width, height, format);
    }
    egui::PaintCallback {
        rect,
        callback: std::sync::Arc::new(egui_glow::CallbackFn::new(move |_info, painter| {
            paint_preview(painter.gl(), &rgba, width, height, format);
        })),
    }
}

fn paint_preview(gl: &glow::Context, rgba: &[u8], width: u32, height: u32, format: GpuBlockFormat) {
    let mut guard = PREVIEW.lock().unwrap_or_else(|err| err.into_inner());
    if guard.is_none() {
        if let Ok(state) = build_preview(gl) {
            *guard = Some(state);
        }
    }
    let Some(state) = guard.as_ref() else { return };
    let need = (width.max(1) * height.max(1) * 4) as usize;
    if rgba.len() < need {
        return;
    }
    let (extensions, major, minor, es) = gl_info_parts(gl);
    let (upload_format, bytes) = if upload_path(format, &extensions, major, minor, es)
        == UploadPath::Compressed
        && format == GpuBlockFormat::Bc1
    {
        (
            GpuBlockFormat::Bc1,
            encode_export_rgba(
                GpuBlockFormat::Bc1,
                &["texture_compression_bc"],
                width,
                height,
                &rgba[..need],
            ),
        )
    } else {
        (GpuBlockFormat::Rgba8, rgba[..need].to_vec())
    };
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(state.texture));
        let _ = upload_compressed(gl, glow::TEXTURE_2D, upload_format, width, height, 1, &bytes);
        linear_params(gl, glow::TEXTURE_2D, 1, false, false);
        gl.use_program(Some(state.program));
        gl.bind_vertex_array(Some(state.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.vbo));
        let quad: [f32; 16] =
            [-1.0, -1.0, 0.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0];
        let bytes =
            std::slice::from_raw_parts(quad.as_ptr().cast::<u8>(), std::mem::size_of_val(&quad));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 16, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 16, 8);
        gl.uniform_1_i32(gl.get_uniform_location(state.program, "u_tex").as_ref(), 0);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::BLEND);
        gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        gl.disable_vertex_attrib_array(0);
        gl.disable_vertex_attrib_array(1);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.use_program(None);
    }
}

fn build_preview(gl: &glow::Context) -> Result<PreviewState, String> {
    let version = egui_glow::ShaderVersion::get(gl);
    let new = version.is_new_shader_interface();
    let (attr, vout, fin) =
        if new { ("in", "out", "in") } else { ("attribute", "varying", "varying") };
    let precision = if version.is_embedded() { "precision mediump float;\n" } else { "" };
    let tex = if new { "texture" } else { "texture2D" };
    let vs = format!(
        "{}{precision}{attr} vec2 a_pos;\n{attr} vec2 a_uv;\n{vout} vec2 v_uv;\nvoid main() {{ v_uv = a_uv; gl_Position = vec4(a_pos, 0.0, 1.0); }}\n",
        version.version_declaration()
    );
    let out_decl = if new { "out vec4 out_color;\n" } else { "" };
    let out_name = if new { "out_color" } else { "gl_FragColor" };
    let fs = format!(
        "{}{precision}{fin} vec2 v_uv;\n{out_decl}uniform sampler2D u_tex;\nvoid main() {{ {out_name} = {tex}(u_tex, v_uv); }}\n",
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
        gl.bind_attrib_location(program, 1, "a_uv");
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
        let texture = gl.create_texture()?;
        Ok(PreviewState { program, vao, vbo, texture })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ext(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| format!("GL_{s}")).collect()
    }

    #[test]
    fn targets_and_layer_counts_follow_the_asset_kind() {
        assert_eq!(target_for_picture(), GpuTextureTarget::D2);
        assert_eq!(target_for_kind(TextureAssetKind::Array2d).gl(), glow::TEXTURE_2D_ARRAY);
        assert_eq!(target_for_kind(TextureAssetKind::Volume3d).gl(), glow::TEXTURE_3D);
        assert_eq!(target_for_kind(TextureAssetKind::Cubemap).gl(), glow::TEXTURE_CUBE_MAP);
        assert_eq!(layer_count(TextureAssetKind::Array2d, 4, 1), 4);
        assert_eq!(layer_count(TextureAssetKind::Volume3d, 2, 8), 8);
        assert_eq!(layer_count(TextureAssetKind::Cubemap, 6, 1), 6);
        assert_eq!(layer_count(TextureAssetKind::Array2d, 0, 1), 1);
    }

    #[test]
    fn compressed_upload_follows_the_extension_list_and_version() {
        let none: [&str; 0] = [];
        assert_eq!(upload_path(GpuBlockFormat::Rgba8, &none, 4, 6, false), UploadPath::Rgba8);
        assert_eq!(upload_path(GpuBlockFormat::Bc7, &none, 4, 6, false), UploadPath::DecodeRgba8);
        let s3tc = ext(&["EXT_texture_compression_s3tc"]);
        assert_eq!(upload_path(GpuBlockFormat::Bc1, &s3tc, 3, 3, false), UploadPath::Compressed);
        assert_eq!(upload_path(GpuBlockFormat::Bc3, &s3tc, 3, 3, false), UploadPath::Compressed);
        assert_eq!(upload_path(GpuBlockFormat::Bc7, &s3tc, 3, 3, false), UploadPath::DecodeRgba8);
        let dxt1 = ext(&["EXT_texture_compression_dxt1"]);
        assert_eq!(upload_path(GpuBlockFormat::Bc1, &dxt1, 3, 3, false), UploadPath::Compressed);
        assert_eq!(upload_path(GpuBlockFormat::Bc3, &dxt1, 3, 3, false), UploadPath::DecodeRgba8);
        let rgtc = ext(&["ARB_texture_compression_rgtc"]);
        assert_eq!(upload_path(GpuBlockFormat::Bc5, &rgtc, 3, 3, false), UploadPath::Compressed);
        assert_eq!(upload_path(GpuBlockFormat::Bc1, &rgtc, 3, 3, false), UploadPath::DecodeRgba8);
        let bptc = ext(&["ARB_texture_compression_bptc"]);
        assert_eq!(upload_path(GpuBlockFormat::Bc6h, &bptc, 3, 3, false), UploadPath::Compressed);
        assert_eq!(upload_path(GpuBlockFormat::Bc7, &bptc, 3, 3, false), UploadPath::Compressed);
        assert_eq!(upload_path(GpuBlockFormat::Etc2, &none, 4, 2, false), UploadPath::DecodeRgba8);
        assert_eq!(upload_path(GpuBlockFormat::Etc2, &none, 4, 3, false), UploadPath::Compressed);
        assert_eq!(upload_path(GpuBlockFormat::Etc2, &none, 3, 0, true), UploadPath::Compressed);
        let es3 = ext(&["ARB_ES3_compatibility"]);
        assert_eq!(upload_path(GpuBlockFormat::Etc2, &es3, 3, 3, false), UploadPath::Compressed);
        let astc = ext(&["KHR_texture_compression_astc_ldr"]);
        assert_eq!(
            upload_path(GpuBlockFormat::Astc4x4, &astc, 3, 3, false),
            UploadPath::Compressed
        );
        assert_eq!(
            upload_path(GpuBlockFormat::Astc4x4, &none, 4, 6, false),
            UploadPath::DecodeRgba8
        );
    }

    #[test]
    fn upload_byte_sizes_follow_format_layers_and_mips() {
        assert_eq!(level_byte_size(GpuBlockFormat::Rgba8, 2, 2), 16);
        assert_eq!(level_byte_size(GpuBlockFormat::Bc1, 4, 4), 8);
        assert_eq!(level_byte_size(GpuBlockFormat::Bc1, 5, 4), 16);
        assert_eq!(level_byte_size(GpuBlockFormat::Bc7, 4, 4), 16);
        assert_eq!(level_byte_size(GpuBlockFormat::Etc2, 4, 4), 8);
        assert_eq!(level_byte_size(GpuBlockFormat::Astc4x4, 4, 4), 16);
        assert_eq!(upload_byte_size(GpuBlockFormat::Bc1, 8, 8, 1, 1), 32);
        assert_eq!(upload_byte_size(GpuBlockFormat::Bc1, 8, 8, 1, 2), 32 + 8);
        assert_eq!(upload_byte_size(GpuBlockFormat::Rgba8, 4, 4, 6, 1), 4 * 4 * 4 * 6);
        assert_eq!(upload_byte_size(GpuBlockFormat::Bc3, 4, 4, 4, 1), 16 * 4);
        assert_eq!(compressed_internal_format(GpuBlockFormat::Rgba8), None);
        assert_eq!(compressed_internal_format(GpuBlockFormat::Bc7), Some(0x8E8C));
        assert!(decode_layers(GpuBlockFormat::Bc1, 4, 4, 1, &[0; 4]).is_err());
    }

    #[test]
    fn anisotropy_clamps_to_the_card_and_uvs_tile_per_primitive() {
        assert_eq!(anisotropy_level(8, 16.0, false), 1.0);
        assert_eq!(anisotropy_level(8, 4.0, true), 4.0);
        assert_eq!(anisotropy_level(0, 16.0, true), 1.0);
        assert_eq!(anisotropy_level(32, 16.0, true), 16.0);
        assert_eq!(quad_uvs([1.0, 1.0]), [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        assert_eq!(quad_uvs([2.0, 3.0])[2], [2.0, 3.0]);
        let sphere = primitive_corner_uvs("sphere", [1.0, 1.0]);
        assert_eq!(sphere[0], [0.0, 0.0]);
        assert!((sphere[1][0] - 1.0 / 12.0).abs() < 1e-5);
        let cone = primitive_corner_uvs("cone", [2.0, 1.0]);
        assert_eq!(cone[3][1], 1.0);
        assert_eq!(cone[1][0], 2.0 / 16.0);
        assert_eq!(planar_xz_uv(0.0, 0.0, 2.0, [1.0, 1.0]), [0.5, 0.5]);
        let white = modulate([255, 255, 255, 255], [0.25, 0.5, 1.0, 1.0]);
        assert!((white[0] - 0.25).abs() < 1e-5 && (white[2] - 1.0).abs() < 1e-5);
        let black = modulate([0, 0, 0, 255], [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(black[0], 0.0);
        assert!(CPU_FALLBACK.contains("flat colour"));
        assert_eq!(parse_gl_version("4.6.0"), (4, 6));
        assert_eq!(parse_gl_version("OpenGL ES 3.2"), (3, 2));
    }

    #[test]
    fn encoded_layer_files_are_used_only_when_fresh_and_matching() {
        let dir = std::env::temp_dir().join(format!("seal-ktx-layers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let picture = dir.join("a.png");
        std::fs::write(&picture, b"png").unwrap();
        let asset = dir.join("sky.texture");
        let blocks = vec![7u8; 8];
        let ktx = seal_render_ktx(GpuBlockFormat::Bc1, 4, 4, &blocks);
        let sidecar = dir.join("sky.layer0.ktx2");
        std::fs::write(&sidecar, &ktx).unwrap();
        let pictures = vec![picture.clone()];
        assert_eq!(encoded_layers(&asset, &pictures, GpuBlockFormat::Bc1, 4, 4), Some(blocks));
        // Wrong format or size: not used.
        assert!(encoded_layers(&asset, &pictures, GpuBlockFormat::Bc7, 4, 4).is_none());
        assert!(encoded_layers(&asset, &pictures, GpuBlockFormat::Bc1, 8, 8).is_none());
        // A picture edited after encoding makes the file stale.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        std::fs::File::options().write(true).open(&picture).unwrap().set_modified(later).unwrap();
        assert!(encoded_layers(&asset, &pictures, GpuBlockFormat::Bc1, 4, 4).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn seal_render_ktx(format: GpuBlockFormat, w: u32, h: u32, blocks: &[u8]) -> Vec<u8> {
        seal_editor_core::write_ktx2(format, true, w, h, &[blocks.to_vec()]).unwrap()
    }

    #[test]
    fn a_damaged_preview_layer_is_rejected() {
        let asset = TextureAsset::default();
        assert!(preview_layer_rgba(&asset, &[Vec::new()], 0).is_err());
        assert!(preview_layer_rgba(&asset, &[], 0).is_err());
        let ok = preview_layer_rgba(&asset, &[vec![1, 2, 3, 4]], 0).unwrap();
        assert_eq!(ok, vec![1, 2, 3, 4]);
    }

    #[test]
    fn each_texture_kind_picks_its_sampler_and_the_shader_text_names_it() {
        assert_eq!(sample_kind_for_picture(), SampleKind::D2);
        assert_eq!(sample_kind_for_texture(TextureAssetKind::Array2d), SampleKind::Array);
        assert_eq!(sample_kind_for_texture(TextureAssetKind::Volume3d), SampleKind::Volume);
        assert_eq!(sample_kind_for_texture(TextureAssetKind::Cubemap), SampleKind::Cube);
        let (_vs, flat) = mesh_shader_source(SampleKind::D2, true);
        assert!(flat.contains("uniform sampler2D u_albedo"));
        assert!(flat.contains("texture(u_albedo, v_tex.xy)"));
        assert!(
            !flat.contains("sampler2DArray")
                && !flat.contains("sampler3D")
                && !flat.contains("samplerCube")
        );
        let (_vs, array) = mesh_shader_source(SampleKind::Array, true);
        assert!(array.contains("uniform sampler2DArray u_albedo"));
        assert!(array.contains("vec3(v_tex.xy, v_tex.z)"));
        assert!(!array.contains("sampler3D") && !array.contains("samplerCube"));
        let (_vs, volume) = mesh_shader_source(SampleKind::Volume, true);
        assert!(volume.contains("uniform sampler3D u_albedo"));
        assert!(volume.contains("uniform float u_depth"));
        assert!(volume.contains("(v_tex.w + 0.5) / u_depth"));
        let (_vs, cube) = mesh_shader_source(SampleKind::Cube, true);
        assert!(cube.contains("uniform samplerCube u_albedo"));
        assert!(cube.contains("normalize(v_normal)"));
        assert!(!cube.contains("sampler2DArray") && !cube.contains("sampler3D"));
        for kind in [SampleKind::D2, SampleKind::Array, SampleKind::Volume, SampleKind::Cube] {
            let (vs, fs) = mesh_shader_source(kind, true);
            assert!(vs.contains("void main") && fs.contains("void main"), "{kind:?}");
            assert!(vs.contains("a_normal") && fs.contains("u_albedo"), "{kind:?}");
        }
    }

    #[test]
    fn volume_w_sits_in_the_middle_of_the_slice_and_old_shading_languages_copy_a_layer() {
        assert!((volume_w(0.0, 4) - 0.125).abs() < 1e-6);
        assert!((volume_w(3.0, 4) - 0.875).abs() < 1e-6);
        assert!((volume_w(0.0, 1) - 0.5).abs() < 1e-6);
        assert!(!real_texture_samplers("1.20"));
        assert!(!real_texture_samplers("1.50"));
        assert!(!real_texture_samplers("OpenGL ES GLSL ES 1.00"));
        assert!(real_texture_samplers("3.30"));
        assert!(real_texture_samplers("4.60"));
        assert!(real_texture_samplers("OpenGL ES GLSL ES 3.00"));
    }
}
