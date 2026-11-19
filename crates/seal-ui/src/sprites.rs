//! Image files drawn as flat pictures in the viewport.
//!
//! Viewport textures decode on a background worker: the frame gets a neutral
//! placeholder tile sized from the file header until pixels arrive. Callers that
//! need pixels on the same thread use [`texture_blocking`].

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use egui::{
    Color32, Mesh, Pos2, TextureFilter, TextureHandle, TextureId, TextureOptions, TextureWrapMode,
};

/// One packed picture: the page file, the placement, the page size, and sampling.
#[derive(Clone)]
struct AtlasHit {
    page: String,
    placed: seal_editor_core::atlas::Placed,
    page_w: u32,
    page_h: u32,
    nearest: bool,
}

/// Atlas pages a packaged game draws its small pictures from.
type AtlasMap = HashMap<String, AtlasHit>;

static ATLAS: std::sync::Mutex<Option<AtlasMap>> = std::sync::Mutex::new(None);

fn atlas_key(path: &str) -> String {
    path.replace('\\', "/")
}

/// The atlas entry for a picture path, matched on its project-relative tail so
/// absolute, relative and differently spelled roots all find it.
fn atlas_find(map: &AtlasMap, path: &str) -> Option<AtlasHit> {
    let path = atlas_key(path);
    if let Some(hit) = map.get(&path) {
        return Some(hit.clone());
    }
    path.match_indices('/').find_map(|(i, _)| map.get(&path[i + 1..]).cloned())
}

/// Draw pictures listed in the packaged `data` folder's atlas from its pages.
/// Returns how many pictures the atlas covers (0 when there is none).
pub fn use_atlas(data: &Path) -> usize {
    let Some(atlas) = seal_editor_core::atlas::Atlas::load(data) else {
        return 0;
    };
    let mut map = AtlasMap::new();
    for placed in &atlas.sprites {
        let (page, pw, ph, nearest) = &atlas.pages[placed.page];
        let page = data.join(page).to_string_lossy().to_string();
        map.insert(
            atlas_key(&placed.path),
            AtlasHit { page, placed: placed.clone(), page_w: *pw, page_h: *ph, nearest: *nearest },
        );
    }
    let count = map.len();
    if let Ok(mut slot) = ATLAS.lock() {
        *slot = Some(map);
    }
    count
}

/// Stop drawing from atlas pages (the editor, after a packaged game).
pub fn clear_atlas() {
    if let Ok(mut slot) = ATLAS.lock() {
        *slot = None;
    }
}

/// The texture to draw a picture with, the rectangle of it holding the
/// picture (the whole texture unless it sits on an atlas page), and the
/// picture's size in pixels.
pub fn sprite_texture(ctx: &egui::Context, path: &str) -> Option<(TextureId, [f32; 4], [f32; 2])> {
    let placed = ATLAS.lock().ok().and_then(|a| atlas_find(a.as_ref()?, path));
    if let Some(hit) = placed {
        let rect = seal_editor_core::atlas::page_uv(&hit.placed, hit.page_w, hit.page_h);
        // Logical size, so a half-scale page still draws at the picture's world size.
        let size = [hit.placed.source_w as f32, hit.placed.source_h as f32];
        if let Some(texture) = texture_filtered(ctx, &hit.page, hit.nearest) {
            return Some((texture, rect, size));
        }
    }
    let nearest = seal_editor_core::import_presets::picture_look(Path::new(path)).nearest;
    let texture = texture_filtered(ctx, path, nearest)?;
    Some((texture, [0.0, 0.0, 1.0, 1.0], texture_size(ctx, path)?))
}

/// Where a trimmed or turned sprite sits inside the quad that shows `frame`
/// (fractions of the original picture) and the page UVs of that patch.
///
/// `None` when the picture is not in an atlas, or it was packed whole. A
/// low-resolution page is whole in this sense: the quad stays the full picture
/// and the texels are simply fewer, so the world size does not change.
pub struct SpriteFit {
    pub corners: [Pos2; 4],
    pub uv: [f32; 4],
    pub uvs: [Pos2; 4],
    /// Opaque region in picture pixels, for measuring borders on the patch that was packed.
    pub content_px: [f32; 2],
    pub turned: bool,
}

fn bilinear_pos(corners: [Pos2; 4], s: f32, t: f32) -> Pos2 {
    let top = corners[0] + (corners[1] - corners[0]) * s;
    let bottom = corners[3] + (corners[2] - corners[3]) * s;
    top + (bottom - top) * t
}

/// Inset `corners` so a trimmed picture's texels stay where they were, and
/// rotate the UVs back when the page stored a quarter turn.
pub fn layout_sprite(
    placed: &seal_editor_core::atlas::Placed,
    page_w: u32,
    page_h: u32,
    corners: [Pos2; 4],
    frame: [f32; 4],
) -> Option<SpriteFit> {
    if !placed.needs_layout() {
        return None;
    }
    let content = seal_editor_core::atlas::content_frame(placed);
    let (fx0, fx1) = if frame[0] <= frame[2] { (frame[0], frame[2]) } else { (frame[2], frame[0]) };
    let (fy0, fy1) = if frame[1] <= frame[3] { (frame[1], frame[3]) } else { (frame[3], frame[1]) };
    let x0 = fx0.max(content[0]);
    let y0 = fy0.max(content[1]);
    let x1 = fx1.min(content[2]);
    let y1 = fy1.min(content[3]);
    if x1 - x0 <= 1e-5 || y1 - y0 <= 1e-5 || (frame[2] - frame[0]).abs() <= 1e-6 {
        return None;
    }
    let (u0, u1) = if frame[0] <= frame[2] { (x0, x1) } else { (x1, x0) };
    let (v0, v1) = if frame[1] <= frame[3] { (y0, y1) } else { (y1, y0) };
    let at = |u: f32, v: f32| {
        let s = (u - frame[0]) / (frame[2] - frame[0]);
        let t = (v - frame[1]) / (frame[3] - frame[1]).max(1e-6);
        bilinear_pos(corners, s, t)
    };
    let fitted = [at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1)];
    let uv_at = |u: f32, v: f32| {
        let [pu, pv] = seal_editor_core::atlas::picture_corner_uv(placed, page_w, page_h, u, v);
        Pos2::new(pu, pv)
    };
    let uvs = [uv_at(u0, v0), uv_at(u1, v0), uv_at(u1, v1), uv_at(u0, v1)];
    Some(SpriteFit {
        corners: fitted,
        uv: [uvs[0].x, uvs[0].y, uvs[2].x, uvs[2].y],
        uvs,
        content_px: [placed.content_w as f32, placed.content_h as f32],
        turned: placed.turned,
    })
}

/// [`layout_sprite`] for a picture the loaded atlas knows about.
pub fn fit_sprite(path: &str, corners: [Pos2; 4], frame: [f32; 4]) -> Option<SpriteFit> {
    let hit = ATLAS.lock().ok().and_then(|atlas| atlas_find(atlas.as_ref()?, path))?;
    layout_sprite(&hit.placed, hit.page_w, hit.page_h, corners, frame)
}

/// Draw a quad with one UV per corner (a turned sprite does not share one axis-aligned rectangle).
pub fn build_quad_uvs_mesh(
    texture: TextureId,
    corners: [Pos2; 4],
    uvs: [Pos2; 4],
    color: Color32,
) -> Mesh {
    let mut mesh = Mesh::with_texture(texture);
    for (pos, uv) in corners.into_iter().zip(uvs) {
        mesh.vertices.push(egui::epaint::Vertex { pos, uv, color });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    mesh
}

pub fn paint_quad_uvs(
    painter: &egui::Painter,
    texture: TextureId,
    corners: [Pos2; 4],
    uvs: [Pos2; 4],
    color: Color32,
) {
    painter.add(egui::Shape::mesh(build_quad_uvs_mesh(texture, corners, uvs, color)));
}

/// A frame rectangle `uv` (fractions of the picture) moved into `rect`, the
/// picture's place on its texture.
pub fn uv_within(rect: [f32; 4], uv: [f32; 4]) -> [f32; 4] {
    let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
    [rect[0] + uv[0] * w, rect[1] + uv[1] * h, rect[0] + uv[2] * w, rect[1] + uv[3] * h]
}

#[derive(Clone, Default)]
struct Cache(crate::sprite_load::RuntimeSpriteCache);

/// A thumbnail: image time, sidecar time, and the texture if it decoded.
type ThumbEntry = (Option<SystemTime>, Option<SystemTime>, Option<TextureHandle>);

#[derive(Clone, Default)]
struct ThumbCache(Arc<std::sync::Mutex<HashMap<String, ThumbEntry>>>);

pub(crate) fn texture_options(
    nearest: bool,
    look: &seal_editor_core::import_presets::PictureLook,
) -> TextureOptions {
    let filter = if nearest { TextureFilter::Nearest } else { TextureFilter::Linear };
    let wrap_mode = match look.wrap {
        seal_editor_core::TextureWrap::Repeat => TextureWrapMode::Repeat,
        seal_editor_core::TextureWrap::Mirror => TextureWrapMode::MirroredRepeat,
        seal_editor_core::TextureWrap::Clamp => TextureWrapMode::ClampToEdge,
    };
    TextureOptions {
        magnification: filter,
        minification: filter,
        wrap_mode,
        mipmap_mode: look.mipmaps.then_some(filter),
    }
}

pub(crate) fn drawn_rgba(path: &Path) -> Option<(Vec<u8>, u32, u32)> {
    let loaded = seal_editor_core::picture_import::load(path).ok()?;
    let rgba =
        if seal_editor_core::picture_import::displays_linear(&loaded.look, loaded.linear_file) {
            seal_editor_core::picture::display_linear_rgba(&loaded.rgba)
        } else {
            loaded.rgba
        };
    Some((rgba, loaded.width, loaded.height))
}

fn sidecar_time(path: &str) -> Option<SystemTime> {
    std::fs::metadata(format!("{path}.import.seal")).and_then(|m| m.modified()).ok()
}

/// The texture for an image file. Sampling follows the picture's import filter
/// (nearest when the picture has no import sidecar, which is how pictures were drawn).
pub fn texture(ctx: &egui::Context, path: &str) -> Option<TextureId> {
    let nearest =
        seal_editor_core::import_presets::picture_look(std::path::Path::new(path)).nearest;
    texture_filtered(ctx, path, nearest)
}

/// Synchronous load for exporters, tests and tooling (blocks on decode).
pub fn texture_blocking(ctx: &egui::Context, path: &str, nearest: bool) -> Option<TextureId> {
    let look = seal_editor_core::import_presets::picture_look(Path::new(path));
    let options = texture_options(nearest, &look);
    let (rgba, w, h) = drawn_rgba(Path::new(path))?;
    let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
    let handle = ctx.load_texture(format!("sprite-sync:{path}"), image, options);
    Some(handle.id())
}

/// Like [`texture`], with the sampling chosen by the caller (an atlas page has one filter).
pub fn texture_filtered(ctx: &egui::Context, path: &str, nearest: bool) -> Option<TextureId> {
    let cache =
        ctx.data_mut(|d| d.get_temp_mut_or_default::<Cache>(egui::Id::new("seal-sprites")).clone());
    crate::sprite_load::finish_worker_jobs(ctx, &cache.0);
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let side = sidecar_time(path);
    let id = crate::sprite_load::queue_async_load(ctx, &cache.0, path, nearest, mtime, side)?;
    crate::sprite_load::refresh_streaming(ctx, &cache.0, path);
    Some(id)
}

/// Upload budget and pending decode count for the Statistics panel.
pub fn texture_runtime_stats() -> (u64, u64, usize) {
    let (used, budget) = seal_editor_core::texture_memory::memory_stats();
    (used, budget, crate::sprite_load::pending_decode_count())
}

pub fn sync_texture_budget_mb(mb: u32) {
    crate::sprite_load::sync_project_budget(mb);
}

pub fn preload_play_textures(ctx: &egui::Context, playing: bool, paths: &[String]) {
    crate::sprite_load::tick_play_preload(ctx, playing, paths);
}

pub fn note_sprite_screen_coverage(
    ctx: &egui::Context,
    path: &str,
    pixel_area: f32,
    base_w: u32,
    base_h: u32,
) {
    crate::sprite_load::note_sprite_coverage(path, pixel_area, base_w, base_h);
    let cache =
        ctx.data_mut(|d| d.get_temp_mut_or_default::<Cache>(egui::Id::new("seal-sprites")).clone());
    crate::sprite_load::refresh_streaming(ctx, &cache.0, path);
}

pub fn set_texture_picture_policy(path: &str, streaming: bool, budget_mb: u32) {
    crate::sprite_load::set_picture_policy(path, streaming, budget_mb);
}

/// Pixel size of a picture file, cached until the file changes.
pub fn picture_pixels(path: &str) -> Option<[f32; 2]> {
    type SizeCache = HashMap<String, (Option<SystemTime>, Option<SystemTime>, [f32; 2])>;
    static CACHE: std::sync::Mutex<Option<SizeCache>> = std::sync::Mutex::new(None);
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let side = sidecar_time(path);
    let mut guard = CACHE.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some((seen, seen_side, size)) = map.get(path) {
        if *seen == mtime && *seen_side == side {
            return Some(*size);
        }
    }
    let size = decode_image(Path::new(path)).map(|(_, w, h)| [w as f32, h as f32])?;
    map.insert(path.to_string(), (mtime, side, size));
    Some(size)
}

/// Pixel width and height of an image file already loaded by [`texture`].
pub fn texture_size(ctx: &egui::Context, path: &str) -> Option<[f32; 2]> {
    let cache =
        ctx.data_mut(|d| d.get_temp_mut_or_default::<Cache>(egui::Id::new("seal-sprites")).clone());
    let map = cache.0 .0.lock().ok()?;
    let entry = map.get(path)?;
    if let Some(handle) = &entry.handle {
        let s = handle.size();
        return Some([s[0] as f32, s[1] as f32]);
    }
    Some([entry.base_w as f32, entry.base_h as f32])
}

/// How to cut a sprite into nine pieces.
#[derive(Clone, Copy, Debug)]
pub struct NineSlice {
    /// Borders in picture pixels: left, right, top, bottom.
    pub border: [f32; 4],
    /// Size in pixels of the whole texture the rectangle is measured on.
    pub picture: [f32; 2],
    /// Sprite scale across and up; at scale 1 the picture is drawn unstretched.
    pub scale: [f32; 2],
}

/// Cut positions along one side, as fractions 0..1 of the drawn quad and of the
/// frame's texture span. `near`/`far` are the border pixels at the start and
/// end of the span, `frame_px` the frame's pixel length, `scale` the sprite scale.
pub fn nine_cuts(near: f32, far: f32, frame_px: f32, scale: f32) -> ([f32; 4], [f32; 4]) {
    seal_editor_core::sprite_slice::nine_cuts(near, far, frame_px, scale)
}

/// Build a nine-slice mesh without drawing it.
pub fn build_nine_mesh(
    texture: TextureId,
    corners: [Pos2; 4],
    r: [f32; 4],
    color: Color32,
    nine: NineSlice,
) -> Mesh {
    let [l, rt, t, b] = nine.border;
    let (first_x, last_x) = if r[2] < r[0] { (rt, l) } else { (l, rt) };
    let (first_y, last_y) = if r[3] < r[1] { (b, t) } else { (t, b) };
    let frame_w = (r[2] - r[0]).abs() * nine.picture[0];
    let frame_h = (r[3] - r[1]).abs() * nine.picture[1];
    let (px, tx) = nine_cuts(first_x, last_x, frame_w, nine.scale[0]);
    let (py, ty) = nine_cuts(first_y, last_y, frame_h, nine.scale[1]);
    let at = |fx: f32, fy: f32| {
        let top = corners[0] + (corners[1] - corners[0]) * fx;
        let bottom = corners[3] + (corners[2] - corners[3]) * fx;
        top + (bottom - top) * fy
    };
    let mut mesh = Mesh::with_texture(texture);
    for j in 0..4 {
        for i in 0..4 {
            let uv = Pos2::new(r[0] + (r[2] - r[0]) * tx[i], r[1] + (r[3] - r[1]) * ty[j]);
            mesh.vertices.push(egui::epaint::Vertex { pos: at(px[i], py[j]), uv, color });
        }
    }
    for j in 0..3u32 {
        for i in 0..3u32 {
            let k = j * 4 + i;
            mesh.indices.extend_from_slice(&[k, k + 1, k + 5, k, k + 5, k + 4]);
        }
    }
    mesh
}

/// Draw a texture rectangle as a nine-slice sprite: corners keep their size,
/// edges stretch one way and the middle stretches both ways. `corners` are
/// top-left, top-right, bottom-right, bottom-left; a swapped `r` mirrors.
pub fn paint_nine(
    painter: &egui::Painter,
    texture: TextureId,
    corners: [Pos2; 4],
    r: [f32; 4],
    color: Color32,
    nine: NineSlice,
) {
    painter.add(egui::Shape::mesh(build_nine_mesh(texture, corners, r, color, nine)));
}

pub fn build_nine_tile_mesh(
    texture: TextureId,
    corners: [Pos2; 4],
    r: [f32; 4],
    color: Color32,
    nine: NineSlice,
) -> Mesh {
    let at = |fx: f32, fy: f32| {
        let top = corners[0] + (corners[1] - corners[0]) * fx;
        let bottom = corners[3] + (corners[2] - corners[3]) * fx;
        top + (bottom - top) * fy
    };
    let mut mesh = Mesh::with_texture(texture);
    for piece in
        seal_editor_core::sprite_slice::tile_pieces(nine.border, r, nine.picture, nine.scale)
    {
        let [fx0, fy0, fx1, fy1] = piece.span;
        let quad = [at(fx0, fy1), at(fx1, fy1), at(fx1, fy0), at(fx0, fy0)];
        let uvs = [
            Pos2::new(piece.uv[0], piece.uv[1]),
            Pos2::new(piece.uv[2], piece.uv[1]),
            Pos2::new(piece.uv[2], piece.uv[3]),
            Pos2::new(piece.uv[0], piece.uv[3]),
        ];
        let base = mesh.vertices.len() as u32;
        for (pos, uv) in quad.into_iter().zip(uvs) {
            mesh.vertices.push(egui::epaint::Vertex { pos, uv, color });
        }
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    mesh
}

/// Like [`paint_nine`], but edges and the centre tile instead of stretching.
pub fn paint_nine_tile(
    painter: &egui::Painter,
    texture: TextureId,
    corners: [Pos2; 4],
    r: [f32; 4],
    color: Color32,
    nine: NineSlice,
) {
    painter.add(egui::Shape::mesh(build_nine_tile_mesh(texture, corners, r, color, nine)));
}

/// Draw `texture` stretched over four screen corners: top-left, top-right,
/// bottom-right, bottom-left.
pub fn paint_quad(painter: &egui::Painter, texture: TextureId, corners: [Pos2; 4]) {
    paint_quad_uv(painter, texture, corners, [0.0, 0.0, 1.0, 1.0], Color32::WHITE);
}

/// Like [`paint_quad`], showing only the texture rectangle `r` = u0, v0, u1,
/// v1 (one frame of a sprite sheet), multiplied by `color` (tint and opacity).
/// Swapped corners mirror the picture.
pub fn paint_quad_uv(
    painter: &egui::Painter,
    texture: TextureId,
    corners: [Pos2; 4],
    r: [f32; 4],
    color: Color32,
) {
    let mut mesh = Mesh::with_texture(texture);
    let uv = [
        Pos2::new(r[0], r[1]),
        Pos2::new(r[2], r[1]),
        Pos2::new(r[2], r[3]),
        Pos2::new(r[0], r[3]),
    ];
    for (pos, uv) in corners.into_iter().zip(uv) {
        mesh.vertices.push(egui::epaint::Vertex { pos, uv, color });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

pub fn build_tight_mesh(
    path: &str,
    texture: TextureId,
    corners: [Pos2; 4],
    uv: [f32; 4],
    color: Color32,
) -> Option<Mesh> {
    let points = tight_points(path)?;
    if points.len() < 3 {
        return None;
    }
    let at = |u: f32, v_up: f32| {
        let v = 1.0 - v_up;
        let top = Pos2::new(
            corners[0].x + (corners[1].x - corners[0].x) * u,
            corners[0].y + (corners[1].y - corners[0].y) * u,
        );
        let bottom = Pos2::new(
            corners[3].x + (corners[2].x - corners[3].x) * u,
            corners[3].y + (corners[2].y - corners[3].y) * u,
        );
        Pos2::new(top.x + (bottom.x - top.x) * v, top.y + (bottom.y - top.y) * v)
    };
    let tex = |u: f32, v_up: f32| {
        Pos2::new(uv[0] + u * (uv[2] - uv[0]), uv[1] + (1.0 - v_up) * (uv[3] - uv[1]))
    };
    let mut mesh = Mesh::with_texture(texture);
    for index in 1..points.len() - 1 {
        let tri = [points[0], points[index], points[index + 1]];
        let base = mesh.vertices.len() as u32;
        for point in tri {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: at(point[0], point[1]),
                uv: tex(point[0], point[1]),
                color,
            });
        }
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
    Some(mesh)
}

/// Draw a tight sprite mesh when the picture's import setting asks for one.
/// Returns false when the picture still uses a full rectangle.
pub fn paint_tight_mesh(
    painter: &egui::Painter,
    path: Option<&str>,
    texture: TextureId,
    corners: [Pos2; 4],
    uv: [f32; 4],
    color: Color32,
) -> bool {
    let Some(path) = path else { return false };
    let Some(mesh) = build_tight_mesh(path, texture, corners, uv, color) else {
        return false;
    };
    painter.add(egui::Shape::mesh(mesh));
    true
}

fn tight_points(path: &str) -> Option<Vec<[f32; 2]>> {
    let look = seal_editor_core::import_presets::picture_look(Path::new(path));
    if !look.mesh_tight {
        return None;
    }
    let mtime = std::fs::metadata(path).and_then(|meta| meta.modified()).ok();
    let side = sidecar_time(path);
    type OutlineCache = HashMap<String, (Option<SystemTime>, Option<SystemTime>, Vec<[f32; 2]>)>;
    static CACHE: std::sync::Mutex<Option<OutlineCache>> = std::sync::Mutex::new(None);
    let mut guard = CACHE.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some((seen, seen_side, points)) = map.get(path) {
        if *seen == mtime && *seen_side == side {
            return Some(points.clone());
        }
    }
    let (rgba, width, height) = decode_image(Path::new(path))?;
    let points = seal_editor_core::picture::tight_mesh(&rgba, width, height);
    map.insert(path.to_string(), (mtime, side, points.clone()));
    Some(points)
}

/// Whether a file name is a picture the asset browser can preview.
pub fn is_image_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if seal_editor_core::aseprite::is_source_name(&lower)
        || seal_editor_core::layered_picture::is_source_name(&lower)
    {
        return true;
    }
    let ext = lower.rsplit('.').next().unwrap_or("");
    lower.contains('.') && seal_editor_core::picture::is_extension(ext)
}

/// Decode a picture file to RGBA8, after its import Max Size.
pub fn decode_image(path: &Path) -> Option<(Vec<u8>, u32, u32)> {
    if seal_editor_core::aseprite::is_source_name(&path.to_string_lossy()) {
        return seal_editor_core::aseprite::preview(path);
    }
    if seal_editor_core::layered_picture::is_source_name(&path.to_string_lossy()) {
        return seal_editor_core::layered_picture::preview(path);
    }
    seal_editor_core::picture_import::load(path)
        .ok()
        .map(|picture| (picture.rgba, picture.width, picture.height))
}

/// Shrink RGBA8 pixels so the longer side is at most `max` (box filter).
pub fn shrink(rgba: &[u8], w: u32, h: u32, max: u32) -> (Vec<u8>, u32, u32) {
    let scale = (w.max(h) as f32 / max as f32).max(1.0);
    let (nw, nh) =
        (((w as f32 / scale).round() as u32).max(1), ((h as f32 / scale).round() as u32).max(1));
    if (nw, nh) == (w, h) {
        return (rgba.to_vec(), w, h);
    }
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1));
        for x in 0..nw {
            let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1));
            let mut sum = [0u32; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let i = ((sy * w + sx) * 4) as usize;
                    for k in 0..4 {
                        sum[k] += u32::from(rgba[i + k]);
                    }
                }
            }
            let n = (y1 - y0) * (x1 - x0);
            out.extend(sum.iter().map(|v| (v / n) as u8));
        }
    }
    (out, nw, nh)
}

/// One decoded thumbnail coming back from the background worker.
struct ThumbDone {
    path: String,
    mtime: Option<SystemTime>,
    side: Option<SystemTime>,
    pixels: Option<(Vec<u8>, u32, u32)>,
}

/// Thumbnails decode on one background thread so a folder of large pictures never stalls
/// the frame; the tile shows its placeholder until the picture arrives.
struct ThumbWorker {
    jobs: std::sync::mpsc::Sender<(String, Option<SystemTime>, Option<SystemTime>)>,
    done: std::sync::mpsc::Receiver<ThumbDone>,
    pending: std::collections::HashSet<String>,
}

fn thumb_worker() -> &'static std::sync::Mutex<ThumbWorker> {
    static WORKER: std::sync::OnceLock<std::sync::Mutex<ThumbWorker>> = std::sync::OnceLock::new();
    WORKER.get_or_init(|| {
        let (job_tx, job_rx) =
            std::sync::mpsc::channel::<(String, Option<SystemTime>, Option<SystemTime>)>();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<ThumbDone>();
        std::thread::Builder::new()
            .name("seal-thumbnails".into())
            .spawn(move || {
                for (path, mtime, side) in job_rx {
                    let pixels =
                        drawn_rgba(Path::new(&path)).map(|(rgba, w, h)| shrink(&rgba, w, h, 128));
                    if done_tx.send(ThumbDone { path, mtime, side, pixels }).is_err() {
                        break;
                    }
                }
            })
            .expect("thumbnail worker thread");
        std::sync::Mutex::new(ThumbWorker {
            jobs: job_tx,
            done: done_rx,
            pending: std::collections::HashSet::new(),
        })
    })
}

/// A small preview of an image file for the asset browser and its size in
/// pixels, loaded once per path until the file changes. `None` while it is still decoding.
pub fn thumbnail(ctx: &egui::Context, path: &str) -> Option<(TextureId, egui::Vec2)> {
    let cache = ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<ThumbCache>(egui::Id::new("seal-thumbnails")).clone()
    });
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let side = sidecar_time(path);
    let mut map = cache.0.lock().ok()?;
    let mut worker = thumb_worker().lock().ok()?;
    // Turn finished decodes into textures (textures are made on this thread).
    while let Ok(done) = worker.done.try_recv() {
        worker.pending.remove(&done.path);
        let look = seal_editor_core::import_presets::picture_look(Path::new(&done.path));
        let handle = done.pixels.map(|(small, sw, sh)| {
            let image =
                egui::ColorImage::from_rgba_unmultiplied([sw as usize, sh as usize], &small);
            ctx.load_texture(
                format!("thumb:{}", done.path),
                image,
                texture_options(look.nearest, &look),
            )
        });
        map.insert(done.path, (done.mtime, done.side, handle));
        ctx.request_repaint();
    }
    if let Some((seen, seen_side, handle)) = map.get(path) {
        if *seen == mtime && *seen_side == side {
            return handle.as_ref().map(|h| (h.id(), h.size_vec2()));
        }
    }
    if worker.pending.insert(path.to_string()) {
        let _ = worker.jobs.send((path.to_string(), mtime, side));
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(50));
    None
}

/// Draw a thumbnail fitted inside `area`, keeping its proportions.
pub fn paint_fitted(
    painter: &egui::Painter,
    texture: TextureId,
    size: egui::Vec2,
    area: egui::Rect,
) {
    let scale = (area.width() / size.x).min(area.height() / size.y);
    let rect = egui::Rect::from_center_size(area.center(), size * scale);
    painter.image(
        texture,
        rect,
        egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
}

/// Decode a picture file to RGBA8 without applying import settings.
pub fn decode(path: &Path) -> Option<(Vec<u8>, u32, u32)> {
    seal_editor_core::picture::decode_path(path)
        .ok()
        .map(|picture| (picture.rgba, picture.width, picture.height))
}

/// Write RGBA8 pixels as a PNG file.
pub fn encode(path: &Path, rgba: &[u8], width: u32, height: u32) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    writer.write_image_data(rgba).map_err(std::io::Error::other)?;
    Ok(())
}

#[cfg(test)]
mod thumb_tests {
    use super::*;

    #[test]
    fn pictures_are_recognised_by_extension() {
        assert!(is_image_name("Hero.PNG"));
        assert!(is_image_name("photo.jpeg"));
        assert!(is_image_name("a.jpg"));
        assert!(is_image_name("a.bmp"));
        assert!(is_image_name("a.gif"));
        assert!(is_image_name("a.tga"));
        assert!(is_image_name("a.webp"));
        assert!(is_image_name("a.hdr"));
        assert!(is_image_name("a.exr"));
        assert!(is_image_name("a.svg"));
        assert!(is_image_name("a.ktx2"));
        assert!(is_image_name("a.dds"));
        assert!(is_image_name("hero.aseprite"));
        assert!(is_image_name("hero.ase"));
        assert!(!is_image_name("scene.seal"));
    }

    #[test]
    fn shrinking_keeps_proportions_and_averages_colour() {
        let rgba: Vec<u8> = (0..256 * 64)
            .flat_map(|i| if i % 2 == 0 { [255, 0, 0, 255] } else { [0, 0, 255, 255] })
            .collect();
        let (small, w, h) = shrink(&rgba, 256, 64, 128);
        assert_eq!((w, h), (128, 32));
        assert_eq!(small.len(), 128 * 32 * 4);
        assert!((i32::from(small[0]) - 127).abs() <= 1 && (i32::from(small[2]) - 127).abs() <= 1);
    }

    #[test]
    fn png_and_jpeg_files_both_decode() {
        let dir = std::env::temp_dir().join(format!("seal-thumb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("dot.png");
        encode(&png, &[10, 20, 30, 255, 40, 50, 60, 255], 2, 1).unwrap();
        assert_eq!(decode_image(&png).map(|(_, w, h)| (w, h)), Some((2, 1)));
        // A 1x1 baseline JPEG.
        let jpg = dir.join("dot.jpg");
        let bytes: &[u8] = &[
            0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06, 0x05, 0x08,
            0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B, 0x0B, 0x0C,
            0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20,
            0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29, 0x2C, 0x30,
            0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32, 0x3C, 0x2E, 0x33, 0x34,
            0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00,
            0xFF, 0xC4, 0x00, 0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0xFF, 0xC4, 0x00, 0x14,
            0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00,
            0xD2, 0xCF, 0x20, 0xFF, 0xD9,
        ];
        std::fs::write(&jpg, bytes).unwrap();
        assert_eq!(decode_image(&jpg).map(|(_, w, h)| (w, h)), Some((1, 1)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn thumbnails_decode_in_the_background_then_appear() {
        let dir = std::env::temp_dir().join(format!("seal-thumb-bg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("dot.png");
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = png::Encoder::new(file, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&[255u8; 16]).unwrap();
        let ctx = egui::Context::default();
        let path = path.to_string_lossy().to_string();
        assert!(
            super::thumbnail(&ctx, &path).is_none(),
            "the first request only queues the decode"
        );
        let mut found = None;
        for _ in 0..200 {
            found = super::thumbnail(&ctx, &path);
            if found.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = std::fs::remove_dir_all(&dir);
        let (_, size) = found.expect("thumbnail arrives from the worker");
        assert_eq!(size, egui::vec2(2.0, 2.0));
    }

    #[test]
    fn png_round_trip() {
        let dir = std::env::temp_dir().join(format!("seal-sprite-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.png");
        let px: Vec<u8> = (0..4 * 3 * 2).map(|i| (i * 11) as u8).collect();
        super::encode(&path, &px, 3, 2).unwrap();
        assert_eq!(super::decode(&path), Some((px, 3, 2)));
        assert_eq!(super::decode(&dir.join("missing.png")), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod nine_tests {
    use super::nine_cuts;

    #[test]
    fn borders_keep_their_size_when_the_sprite_grows() {
        // A 32 px frame with 8 px borders drawn at scale 4: each border takes
        // 8/32/4 = 1/16 of the quad, but a quarter of the texture.
        let (pos, tex) = nine_cuts(8.0, 8.0, 32.0, 4.0);
        assert_eq!(pos, [0.0, 0.0625, 0.9375, 1.0]);
        assert_eq!(tex, [0.0, 0.25, 0.75, 1.0]);
    }

    #[test]
    fn shrunk_sprites_share_the_space_between_their_borders() {
        let (pos, _) = nine_cuts(8.0, 24.0, 32.0, 0.5);
        assert_eq!(pos, [0.0, 0.25, 0.25, 1.0]);
    }

    #[test]
    fn oversized_borders_are_clamped_to_the_frame() {
        let (_, tex) = nine_cuts(40.0, 40.0, 32.0, 1.0);
        assert_eq!(tex, [0.0, 1.0, 1.0, 1.0]);
    }
}

#[cfg(test)]
mod atlas_tests {
    use super::{layout_sprite, uv_within};

    #[test]
    fn frames_land_inside_the_pictures_atlas_rectangle() {
        let rect = [0.5, 0.25, 0.75, 0.5];
        assert_eq!(uv_within(rect, [0.0, 0.0, 1.0, 1.0]), rect);
        // The right half of a two-frame sheet, mirrored.
        assert_eq!(uv_within(rect, [1.0, 0.0, 0.5, 1.0]), [0.75, 0.25, 0.625, 0.5]);
    }

    #[test]
    fn a_trimmed_sprite_keeps_its_place_in_the_quad() {
        use seal_editor_core::atlas::Placed;
        let placed = Placed {
            path: "sprites/pad.png".into(),
            page: 0,
            x: 1,
            y: 2,
            w: 4,
            h: 4,
            source_w: 16,
            source_h: 16,
            origin_x: 6,
            origin_y: 5,
            content_w: 4,
            content_h: 4,
            turned: false,
        };
        let corners = [
            egui::pos2(0.0, 0.0),
            egui::pos2(16.0, 0.0),
            egui::pos2(16.0, 16.0),
            egui::pos2(0.0, 16.0),
        ];
        let fit = layout_sprite(&placed, 32, 32, corners, [0.0, 0.0, 1.0, 1.0]).unwrap();
        assert!(!fit.turned);
        assert_eq!(fit.corners[0], egui::pos2(6.0, 5.0));
        assert_eq!(fit.corners[2], egui::pos2(10.0, 9.0));
        assert_eq!(fit.uv, [1.0 / 32.0, 2.0 / 32.0, 5.0 / 32.0, 6.0 / 32.0]);
    }

    #[test]
    fn a_half_size_page_keeps_the_full_quad() {
        use seal_editor_core::atlas::Placed;
        let placed = Placed {
            path: "sprites/half.png".into(),
            page: 0,
            x: 0,
            y: 0,
            w: 4,
            h: 2,
            source_w: 8,
            source_h: 4,
            origin_x: 0,
            origin_y: 0,
            content_w: 8,
            content_h: 4,
            turned: false,
        };
        let corners = [
            egui::pos2(0.0, 0.0),
            egui::pos2(8.0, 0.0),
            egui::pos2(8.0, 4.0),
            egui::pos2(0.0, 4.0),
        ];
        assert!(layout_sprite(&placed, 4, 2, corners, [0.0, 0.0, 1.0, 1.0]).is_none());
        let uv = seal_editor_core::atlas::page_uv(&placed, 4, 2);
        assert_eq!(uv, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(((uv[2] - uv[0]) * 4.0).round() as u32, 4);
        assert_eq!([placed.source_w, placed.source_h], [8, 4]);
    }
}

/// Decoded normal-map pictures, kept so lighting can sample them every frame.
type NormalPixels = Arc<(Vec<u8>, u32, u32)>;
static NORMALS: std::sync::Mutex<Option<HashMap<String, Option<NormalPixels>>>> =
    std::sync::Mutex::new(None);

/// The decoded RGBA pixels of a normal map, or None when it does not load.
pub fn normal_pixels(path: &str) -> Option<NormalPixels> {
    let mut guard = NORMALS.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    map.entry(path.to_string())
        .or_insert_with(|| decode_image(Path::new(path)).map(Arc::new))
        .clone()
}

/// The surface normal at texture coordinate `uv` (0 to 1, v down) of a
/// decoded normal map. Normal maps store green as "up", which is +y here.
pub fn sample_normal(pixels: &(Vec<u8>, u32, u32), uv: [f32; 2]) -> [f32; 3] {
    let (data, w, h) = (&pixels.0, pixels.1.max(1), pixels.2.max(1));
    let x = ((uv[0].clamp(0.0, 1.0) * w as f32) as u32).min(w - 1);
    let y = ((uv[1].clamp(0.0, 1.0) * h as f32) as u32).min(h - 1);
    let i = ((y * w + x) * 4) as usize;
    if i + 2 >= data.len() {
        return [0.0, 0.0, 1.0];
    }
    seal_editor_core::light2d::decode_normal([data[i], data[i + 1], data[i + 2]])
}

/// Steps per side when a sprite is drawn as a grid so 2D light can vary across it.
pub const LIT_STEPS: usize = 12;

/// Draws a textured quad split into a grid whose vertex colours come from
/// `light_at(s, t)`, where s and t run 0 to 1 from the top-left corner.
pub fn paint_quad_lit(
    painter: &egui::Painter,
    texture: TextureId,
    corners: [Pos2; 4],
    r: [f32; 4],
    color: Color32,
    light_at: impl Fn(f32, f32) -> [f32; 3],
) {
    let uvs = [
        Pos2::new(r[0], r[1]),
        Pos2::new(r[2], r[1]),
        Pos2::new(r[2], r[3]),
        Pos2::new(r[0], r[3]),
    ];
    paint_quad_lit_uvs(painter, texture, corners, uvs, color, light_at);
}

/// Like [`paint_quad_lit`], with one texture coordinate per corner.
pub fn build_lit_quad_mesh(
    texture: TextureId,
    corners: [Pos2; 4],
    uvs: [Pos2; 4],
    color: Color32,
    light_at: impl Fn(f32, f32) -> [f32; 3],
) -> Mesh {
    let n = LIT_STEPS;
    let mut mesh = Mesh::with_texture(texture);
    let lerp = |a: Pos2, b: Pos2, t: f32| a + (b - a) * t;
    let base = egui::Rgba::from(color);
    for j in 0..=n {
        let t = j as f32 / n as f32;
        let left = lerp(corners[0], corners[3], t);
        let right = lerp(corners[1], corners[2], t);
        let uv_left = lerp(uvs[0], uvs[3], t);
        let uv_right = lerp(uvs[1], uvs[2], t);
        for i in 0..=n {
            let s = i as f32 / n as f32;
            let pos = lerp(left, right, s);
            let uv = lerp(uv_left, uv_right, s);
            let l = light_at(s, t);
            let lit = egui::Rgba::from_rgba_premultiplied(
                (base.r() * l[0]).min(base.a()),
                (base.g() * l[1]).min(base.a()),
                (base.b() * l[2]).min(base.a()),
                base.a(),
            );
            mesh.vertices.push(egui::epaint::Vertex { pos, uv, color: lit.into() });
        }
    }
    let row = (n + 1) as u32;
    for j in 0..n as u32 {
        for i in 0..n as u32 {
            let a = j * row + i;
            mesh.indices.extend_from_slice(&[a, a + 1, a + row + 1, a, a + row + 1, a + row]);
        }
    }
    mesh
}

pub fn paint_quad_lit_uvs(
    painter: &egui::Painter,
    texture: TextureId,
    corners: [Pos2; 4],
    uvs: [Pos2; 4],
    color: Color32,
    light_at: impl Fn(f32, f32) -> [f32; 3],
) {
    painter.add(egui::Shape::mesh(build_lit_quad_mesh(texture, corners, uvs, color, light_at)));
}

/// Pixels of a picture for a material to work on: the file itself, or its
/// rectangle cut from an atlas page in a packaged game.
fn material_source(path: &str) -> Option<(Vec<u8>, u32, u32)> {
    if let Some(found) = decode_image(Path::new(path)) {
        return Some(found);
    }
    let hit = ATLAS.lock().ok().and_then(|a| atlas_find(a.as_ref()?, path))?;
    let (pixels, pw, ph) = decode_image(Path::new(&hit.page))?;
    let (w, h) = (hit.placed.source_w.max(1), hit.placed.source_h.max(1));
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    for y in 0..h {
        for x in 0..w {
            let Some((px, py)) = seal_editor_core::atlas::page_texel(&hit.placed, x, y) else {
                continue;
            };
            if px >= pw || py >= ph {
                continue;
            }
            let start = ((py * pw + px) * 4) as usize;
            let dest = ((y * w + x) * 4) as usize;
            if let Some(texel) = pixels.get(start..start + 4) {
                out[dest..dest + 4].copy_from_slice(texel);
            }
        }
    }
    Some((out, w, h))
}

/// A sprite's shaded picture and the time it was shaded for, by picture and material.
struct Shaded {
    source: Arc<(Vec<u8>, u32, u32)>,
    program: Option<seal_editor_core::material2d::Program>,
    handle: TextureHandle,
    time: f64,
}

static SHADED: std::sync::Mutex<Option<HashMap<String, Option<Shaded>>>> =
    std::sync::Mutex::new(None);

/// The texture of `path` drawn through `material`, redrawn about 30 times a second
/// while the material moves. `None` when the picture or the material cannot be used.
pub fn material_texture(
    ctx: &egui::Context,
    path: &str,
    material: &seal_editor_core::material2d::Material2d,
) -> Option<TextureId> {
    use seal_editor_core::material2d::{animated, load_program, shade, Picture};
    if material.effect.is_empty() {
        return None;
    }
    let shader_time = if material.shader.is_empty() {
        None
    } else {
        std::fs::metadata(&material.shader).and_then(|m| m.modified()).ok()
    };
    let look = seal_editor_core::import_presets::picture_look(Path::new(path));
    let nearest = look.nearest;
    let key = format!(
        "{path}|{:?}|{:?}|{:?}|{nearest}",
        material,
        std::fs::metadata(path).and_then(|m| m.modified()).ok(),
        shader_time
    );
    let now = ctx.input(|i| i.time);
    let moving = animated(material);
    let time = if moving { (now * 30.0).floor() / 30.0 } else { 0.0 };
    let mut guard = SHADED.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    if !map.contains_key(&key) {
        // Old versions of this picture and material are dropped with their textures.
        map.retain(|k, _| !k.starts_with(&format!("{path}|")));
        let made = material_source(path).and_then(|(pixels, w, h)| {
            let program = if material.effect == "custom" {
                Some(load_program(Path::new(&material.shader)).ok()?)
            } else {
                None
            };
            let picture = Picture { pixels: &pixels, width: w, height: h };
            let out = shade(&picture, material, program.as_ref(), time as f32);
            let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &out);
            let handle =
                ctx.load_texture(format!("material:{key}"), image, texture_options(nearest, &look));
            Some(Shaded { source: Arc::new((pixels, w, h)), program, handle, time })
        });
        map.insert(key.clone(), made);
    }
    let entry = map.get_mut(&key)?.as_mut()?;
    if moving {
        if entry.time != time {
            let (pixels, w, h) = &*entry.source;
            let picture = Picture { pixels, width: *w, height: *h };
            let out = shade(&picture, material, entry.program.as_ref(), time as f32);
            let image = egui::ColorImage::from_rgba_unmultiplied([*w as usize, *h as usize], &out);
            entry.handle.set(image, texture_options(nearest, &look));
            entry.time = time;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
    Some(entry.handle.id())
}
