use egui::{Color32, Pos2};

use crate::widgets::GridMark;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshPrimitive {
    Cube,
    Sphere,
    Cylinder,
    Cone,
    Capsule,
    Plane,
}

pub fn mesh_for_place_kind(kind: &str) -> MeshPrimitive {
    match kind {
        "sphere" => MeshPrimitive::Sphere,
        "cylinder" => MeshPrimitive::Cylinder,
        "cone" => MeshPrimitive::Cone,
        "capsule" => MeshPrimitive::Capsule,
        "plane" => MeshPrimitive::Plane,
        _ => MeshPrimitive::Cube,
    }
}

pub struct LitTriangle {
    pub points: [Pos2; 3],
    pub depth: f32,
    /// View depth of each corner, for per-pixel depth testing.
    pub depths: [f32; 3],
    pub color: Color32,
    /// GPU drawing only: the colour with no direct light, used where the
    /// shadow map says the pixel is in shadow (equal to `color` when unshadowed).
    pub dark: Color32,
    /// GPU drawing only: each corner's shadow-map coordinates and depth.
    pub shadow_uvz: [[f32; 3]; 3],
    /// GPU drawing only: depth bias for this face's shadow test.
    pub shadow_bias: f32,
    /// Texture coordinates at each corner. Ignored when `tex_path` is empty.
    pub uv: [[f32; 2]; 3],
    /// Array layer or cubemap face.
    pub tex_layer: u32,
    /// Volume slice index.
    pub tex_slice: f32,
    /// Picture or `.texture` path. Empty keeps the flat colour.
    pub tex_path: String,
    /// Which sampler the graphics card uses for `tex_path`.
    pub tex_kind: crate::gpu_textures::SampleKind,
    /// Unit surface normal, for a cubemap lookup.
    pub normal: [f32; 3],
    /// HDR scale. 1 matches the 8-bit colour. Above 1 is a bright light or glow.
    pub light: f32,
}

/// Scale for [`LitTriangle::light`]. The 8-bit colour stays clamped for the
/// ordinary window. A bright light or glow keeps the extra range here.
pub fn scene_light_gain(intensity: f32, glow_on: bool, glow_intensity: f32) -> f32 {
    let glow = if glow_on { glow_intensity.max(0.0) } else { 1.0 };
    intensity.max(0.0).max(glow).max(1.0)
}

struct TexDraw<'a> {
    path: &'a str,
    layer: u32,
    slice: f32,
    tile: [f32; 2],
    kind: crate::gpu_textures::SampleKind,
}

/// Lit triangles plus the shadow map they were shaded with.
pub struct LitScene {
    pub tris: Vec<LitTriangle>,
    pub shadow: Option<seal_editor_core::shadow::ShadowMap>,
}

const AMBIENT: f32 = 0.28;
/// Largest face piece (world units) when shadows are on.
const SHADOW_FACE_CELL: f32 = 0.5;
/// Shadow map resolution in texels per side.
const DEFAULT_SHADOW_MAP_SIZE: usize = 384;

fn mark_bounds(mark: &GridMark<'_>) -> Option<([f32; 3], [f32; 3])> {
    let primitive = mark.mesh?;
    if mark.billboard.is_some() || mark.faint {
        return None;
    }
    let c = [mark.x, mark.y, mark.depth];
    let h = mark.size;
    let h = match primitive {
        MeshPrimitive::Sphere => {
            let r = h[0].max(h[1]).max(h[2]);
            [r, r, r]
        }
        MeshPrimitive::Capsule => {
            let r = h[0].max(h[2]);
            [r, h[1].max(0.1) + r, r]
        }
        _ => h,
    };
    Some(([c[0] - h[0], c[1] - h[1], c[2] - h[2]], [c[0] + h[0], c[1] + h[1], c[2] + h[2]]))
}

/// The light's depth view of every solid mesh, fitted around the scene.
/// `light_dir` points towards the light. Returns None for an empty scene.
pub fn shadow_map_for(
    marks: &[GridMark<'_>],
    light_dir: [f32; 3],
    map_size: usize,
) -> Option<seal_editor_core::shadow::ShadowMap> {
    let all: Vec<_> = marks.iter().filter_map(mark_bounds).collect();
    // Flat slabs (floors) only receive shadows: fitting the map around them
    // would spread its texels over the whole ground and make every shadow blocky.
    let is_slab = |(min, max): &([f32; 3], [f32; 3])| {
        let width = (max[0] - min[0]).max(max[2] - min[2]);
        max[1] - min[1] < 0.1 * width && width > 4.0
    };
    let mut boxes: Vec<_> = all.iter().copied().filter(|b| !is_slab(b)).collect();
    if boxes.is_empty() {
        boxes = all;
    }
    if boxes.is_empty() || light_dir.iter().all(|v| v.abs() < f32::EPSILON) {
        return None;
    }
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for (min, max) in &boxes {
        for k in 0..3 {
            lo[k] = lo[k].min(min[k]);
            hi[k] = hi[k].max(max[k]);
        }
    }
    let center = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let d = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let radius = ((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() / 2.0).max(0.5) * 1.05;
    let travel = [-light_dir[0], -light_dir[1], -light_dir[2]];
    let frame = seal_editor_core::shadow::LightFrame::fit(travel, center, radius);
    let size = map_size.clamp(64, 2048);
    let mut map = seal_editor_core::shadow::ShadowMap::new(size, frame);
    // The real triangles, so round shapes cast round shadows and do not shade
    // themselves with their bounding box.
    let sink = std::cell::RefCell::new(Vec::new());
    let ctx = MeshPaintCtx {
        light: light_dir,
        shadow: None,
        probes: None,
        yaw: 0.0,
        pitch: 0.0,
        origin: Pos2::ZERO,
        cell: 1.0,
        gpu: false,
        capture: Some(&sink),
        light_gain: 1.0,
    };
    emit_marks(&mut Vec::new(), &ctx, marks);
    for [a, b, c] in sink.into_inner() {
        map.draw_triangle(a, b, c);
    }
    Some(map)
}

struct MeshPaintCtx<'a> {
    light: [f32; 3],
    shadow: Option<&'a seal_editor_core::shadow::ShadowMap>,
    probes: Option<&'a seal_editor_core::probes::ProbeGrid>,
    yaw: f32,
    pitch: f32,
    origin: Pos2,
    cell: f32,
    /// Shadows are tested per pixel on the graphics card instead of per piece.
    gpu: bool,
    /// When set, triangles are only collected here in world space (for the shadow map).
    capture: Option<&'a std::cell::RefCell<Vec<[[f32; 3]; 3]>>>,
    light_gain: f32,
}

/// Lambert shading where `sun` (0 to 1) is how much of the direct light
/// reaches the surface; ambient light is never shadowed.
fn lit_color_shadowed(base: Color32, normal: [f32; 3], light: [f32; 3], sun: f32) -> Color32 {
    let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if len < f32::EPSILON {
        return base;
    }
    let nx = normal[0] / len;
    let ny = normal[1] / len;
    let nz = normal[2] / len;
    let dot = (nx * light[0] + ny * light[1] + nz * light[2]).clamp(0.0, 1.0);
    let factor = (AMBIENT + dot * sun * (1.0 - AMBIENT)).clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (f32::from(base.r()) * factor) as u8,
        (f32::from(base.g()) * factor) as u8,
        (f32::from(base.b()) * factor) as u8,
        base.a(),
    )
}

/// Adds probe fill light (per channel, 1 = full base colour) on top of the key-lit colour.
fn add_fill(lit: Color32, base: Color32, fill: [f32; 3]) -> Color32 {
    let ch =
        |l: u8, b: u8, f: f32| (f32::from(l) + f32::from(b) * f.clamp(0.0, 1.0)).min(255.0) as u8;
    Color32::from_rgba_premultiplied(
        ch(lit.r(), base.r(), fill[0]),
        ch(lit.g(), base.g(), fill[1]),
        ch(lit.b(), base.b(), fill[2]),
        lit.a(),
    )
}

/// Rays per probe for the viewport's bounced light.
const GI_RAYS: usize = 48;

/// Probes holding the fill lights plus light from the sun and fill lights bounced
/// off every solid mesh (with shadows). Baked once and reused until the scene,
/// the sun or the fill lights change.
pub fn gi_probe_grid_for(
    marks: &[GridMark<'_>],
    fill: &[seal_editor_core::probes::Light],
    light_dir: [f32; 3],
) -> Option<seal_editor_core::probes::ProbeGrid> {
    use seal_editor_core::gi::{GiScene, Occluder};
    use seal_editor_core::probes::{Light, ProbeGrid};
    use std::hash::{Hash, Hasher};
    let occluders: Vec<Occluder> = marks
        .iter()
        .filter_map(|mark| {
            let (min, max) = mark_bounds(mark)?;
            let albedo = mark.tint.map(|c| f32::from(c) / 255.0);
            Some(Occluder { min, max, albedo })
        })
        .collect();
    if occluders.is_empty() {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for o in &occluders {
        for v in o.min.iter().chain(&o.max).chain(&o.albedo) {
            v.to_bits().hash(&mut hasher);
        }
    }
    for v in light_dir {
        v.to_bits().hash(&mut hasher);
    }
    format!("{fill:?}").hash(&mut hasher);
    let key = hasher.finish();
    thread_local! {
        static CACHE: std::cell::RefCell<Option<(u64, ProbeGrid)>> = const { std::cell::RefCell::new(None) };
    }
    if let Some(hit) =
        CACHE.with(|c| c.borrow().as_ref().filter(|(k, _)| *k == key).map(|(_, g)| clone_grid(g)))
    {
        return Some(hit);
    }
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for o in &occluders {
        for k in 0..3 {
            lo[k] = lo[k].min(o.min[k]);
            hi[k] = hi[k].max(o.max[k]);
        }
    }
    let extent = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(0.5);
    let spacing = (extent / 7.0).max(2.0);
    let dims = [0, 1, 2].map(|k| (((hi[k] - lo[k]) / spacing).ceil() as usize + 1).clamp(1, 8));
    let mut lights = fill.to_vec();
    lights.push(Light::Directional { toward: light_dir, color: [1.0, 1.0, 1.0] });
    let scene = GiScene { occluders, lights, sky: [0.0; 3], direct: false };
    let mut grid = scene.bake_grid(lo, spacing, dims, GI_RAYS);
    if !fill.is_empty() {
        let direct = ProbeGrid::bake(lo, spacing, dims, fill);
        for (sh, d) in grid.probes.iter_mut().zip(&direct.probes) {
            for k in 0..4 {
                for c in 0..3 {
                    sh[k][c] += d[k][c];
                }
            }
        }
    }
    CACHE.with(|c| *c.borrow_mut() = Some((key, clone_grid(&grid))));
    Some(grid)
}

fn clone_grid(g: &seal_editor_core::probes::ProbeGrid) -> seal_editor_core::probes::ProbeGrid {
    seal_editor_core::probes::ProbeGrid {
        origin: g.origin,
        spacing: g.spacing,
        dims: g.dims,
        probes: g.probes.clone(),
    }
}

/// Irradiance probes over the meshes' bounds (one every ~2 units, at most 8 per
/// axis), baked from `fill`. None when there is nothing to light or no fill light.
pub fn probe_grid_for(
    marks: &[GridMark<'_>],
    fill: &[seal_editor_core::probes::Light],
) -> Option<seal_editor_core::probes::ProbeGrid> {
    if fill.is_empty() {
        return None;
    }
    let boxes: Vec<_> = marks.iter().filter_map(mark_bounds).collect();
    if boxes.is_empty() {
        return None;
    }
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for (min, max) in &boxes {
        for k in 0..3 {
            lo[k] = lo[k].min(min[k]);
            hi[k] = hi[k].max(max[k]);
        }
    }
    let extent = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(0.5);
    let spacing = (extent / 7.0).max(2.0);
    let dims = [0, 1, 2].map(|k| (((hi[k] - lo[k]) / spacing).ceil() as usize + 1).clamp(1, 8));
    Some(seal_editor_core::probes::ProbeGrid::bake(lo, spacing, dims, fill))
}

/// Corners of a mark's solid box in the scene, or None when it has no 3D shape.
fn mark_box(mark: &GridMark<'_>) -> Option<([f32; 3], [f32; 3])> {
    if mark.billboard.is_some() || mark.sprite.is_some() {
        return None;
    }
    let primitive = mark.mesh?;
    let c = [mark.x, mark.y, mark.depth];
    let h = mark.size;
    let h = match primitive {
        MeshPrimitive::Sphere => {
            let r = h[0].max(h[1]).max(h[2]);
            [r, r, r]
        }
        MeshPrimitive::Capsule => {
            let r = h[0].max(h[2]);
            [r, h[1].max(0.1) + r, r]
        }
        _ => h,
    };
    Some(([c[0] - h[0], c[1] - h[1], c[2] - h[2]], [c[0] + h[0], c[1] + h[1], c[2] + h[2]]))
}

/// Screen outline of a mark's 3D shape in the angled view: the convex hull of its
/// projected box corners, so selection outlines and clicks follow what is drawn.
pub fn mark_hull(
    mark: &GridMark<'_>,
    yaw_deg: f32,
    pitch_deg: f32,
    origin: Pos2,
    cell: f32,
) -> Option<Vec<Pos2>> {
    let (lo, hi) = mark_box(mark)?;
    let mut points = Vec::with_capacity(8);
    for x in [lo[0], hi[0]] {
        for y in [lo[1], hi[1]] {
            for z in [lo[2], hi[2]] {
                points.push(project_point([x, y, z], yaw_deg, pitch_deg, origin, cell).0);
            }
        }
    }
    Some(convex_hull(points))
}

/// Convex hull in counter-clockwise screen order (monotone chain).
pub fn convex_hull(mut points: Vec<Pos2>) -> Vec<Pos2> {
    points.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
    });
    points.dedup_by(|a, b| (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4);
    if points.len() < 3 {
        return points;
    }
    let cross = |o: Pos2, a: Pos2, b: Pos2| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut hull: Vec<Pos2> = Vec::with_capacity(points.len() * 2);
    for &p in &points {
        while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(p);
    }
    // The upper chain may not pop back into the finished lower chain.
    let lower = hull.len() + 1;
    for &p in points.iter().rev().skip(1) {
        while hull.len() >= lower && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(p);
    }
    hull.pop();
    hull
}

/// True when `p` is inside the convex polygon `hull`, or within `margin` pixels of it.
pub fn hull_contains(hull: &[Pos2], p: Pos2, margin: f32) -> bool {
    if hull.len() < 3 {
        return hull.iter().any(|q| q.distance(p) <= margin);
    }
    let n = hull.len();
    // Orientation-independent: p must be on the same side of every edge.
    let mut sign = 0.0f32;
    for i in 0..n {
        let a = hull[i];
        let b = hull[(i + 1) % n];
        let edge = b - a;
        let len = edge.length().max(f32::EPSILON);
        let side = (edge.x * (p.y - a.y) - edge.y * (p.x - a.x)) / len;
        if side.abs() <= margin {
            continue;
        }
        if sign == 0.0 {
            sign = side.signum();
        } else if side.signum() != sign {
            return false;
        }
    }
    true
}

/// Distance of a point along the angled view's depth; smaller is nearer the viewer.
pub fn view_depth(p: [f32; 3], yaw_deg: f32, pitch_deg: f32) -> f32 {
    // Linear, so an eye only shifts every depth by the same amount: order is unchanged.
    project_point_angled(p, yaw_deg, pitch_deg, Pos2::ZERO, 1.0).1
}

/// Project a scene point through the viewport camera onto screen space.
pub fn project_scene_point(
    p: [f32; 3],
    yaw_deg: f32,
    pitch_deg: f32,
    origin: Pos2,
    cell: f32,
) -> Pos2 {
    project_point(p, yaw_deg, pitch_deg, origin, cell).0
}

/// A play camera the viewport looks through: perspective from `eye`, with
/// `focal_px` pixels per unit of sideways offset at one unit of depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LookThrough {
    pub eye: [f32; 3],
    pub focal_px: f32,
}

/// Points nearer the eye than this are cut away (never divided by).
pub const LOOK_NEAR: f32 = 0.05;

thread_local! {
    static LOOK: std::cell::Cell<Option<LookThrough>> = const { std::cell::Cell::new(None) };
}

/// Set while painting a viewport that looks through a play camera; clear it after.
pub fn set_look_through(look: Option<LookThrough>) {
    LOOK.with(|cell| cell.set(look));
}

pub fn look_through() -> Option<LookThrough> {
    LOOK.with(|cell| cell.get())
}

/// Scene point to screen through the viewport camera; a perspective view from the
/// play camera while one is set (see `set_look_through`). Depth grows away from the viewer.
pub(crate) fn project_point(
    p: [f32; 3],
    yaw_deg: f32,
    pitch_deg: f32,
    origin: Pos2,
    cell: f32,
) -> (Pos2, f32) {
    let Some(look) = look_through() else {
        return project_point_angled(p, yaw_deg, pitch_deg, origin, cell);
    };
    let rel = [p[0] - look.eye[0], p[1] - look.eye[1], p[2] - look.eye[2]];
    let (q, depth) = project_point_angled(rel, yaw_deg, pitch_deg, Pos2::ZERO, 1.0);
    let scale = look.focal_px / depth.max(LOOK_NEAR);
    (Pos2::new(origin.x + q.x * scale, origin.y + q.y * scale), depth)
}

/// The angled (parallel) projection; also used for directions, which have no eye.
pub(crate) fn project_point_angled(
    p: [f32; 3],
    yaw_deg: f32,
    pitch_deg: f32,
    origin: Pos2,
    cell: f32,
) -> (Pos2, f32) {
    let yaw = yaw_deg.to_radians();
    let pitch = pitch_deg.to_radians();
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let x1 = p[0] * cy - p[2] * sy;
    let z1 = p[0] * sy + p[2] * cy;
    let y1 = p[1] * cp - z1 * sp;
    let z2 = p[1] * sp + z1 * cp;
    let screen = Pos2::new(origin.x + x1 * cell, origin.y - y1 * cell);
    (screen, z2)
}

/// How much direct light reaches a surface point, with a normal offset and a
/// slope-scaled depth bias so a lit face never shadows itself (no acne).
pub(crate) fn surface_sun(
    map: &seal_editor_core::shadow::ShadowMap,
    p: [f32; 3],
    n: [f32; 3],
    facing: f32,
) -> f32 {
    let texel = map.frame.radius * 2.0 / map.size as f32;
    let facing = facing.clamp(0.05, 1.0);
    let slope = ((1.0 - facing * facing).sqrt() / facing).min(8.0);
    let lift = texel;
    let q = [p[0] + n[0] * lift, p[1] + n[1] * lift, p[2] + n[2] * lift];
    let bias = (1.0 + 1.5 * slope) / map.size as f32;
    map.lit(q, bias, 1)
}

/// All lit triangles (already ordered back to front) as one plain mesh.
pub fn lit_mesh(tris: &[LitTriangle]) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    for tri in tris {
        let base = mesh.vertices.len() as u32;
        for p in tri.points {
            mesh.colored_vertex(p, tri.color);
        }
        mesh.add_triangle(base, base + 1, base + 2);
    }
    mesh
}

/// Triangle corners and their UVs after clipping.
type ClippedTri = ([[f32; 3]; 3], [[f32; 2]; 3]);

/// The part of a triangle at or beyond the near distance, as one or two triangles
/// (same winding). `depths` are the corners' view depths; at least one is in front.
fn clip_near(corners: [[f32; 3]; 3], depths: [f32; 3], uvs: [[f32; 2]; 3]) -> Vec<ClippedTri> {
    let mut poly: Vec<([f32; 3], [f32; 2])> = Vec::with_capacity(4);
    for i in 0..3 {
        let (p, dp, uv) = (corners[i], depths[i], uvs[i]);
        let (q, dq, uq) = (corners[(i + 1) % 3], depths[(i + 1) % 3], uvs[(i + 1) % 3]);
        if dp >= LOOK_NEAR {
            poly.push((p, uv));
        }
        if (dp >= LOOK_NEAR) != (dq >= LOOK_NEAR) {
            let t = ((LOOK_NEAR * 1.01 - dp) / (dq - dp)).clamp(0.0, 1.0);
            poly.push((
                [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t, p[2] + (q[2] - p[2]) * t],
                [uv[0] + (uq[0] - uv[0]) * t, uv[1] + (uq[1] - uv[1]) * t],
            ));
        }
    }
    // Cut corners sit just past the near distance, so the pieces are never cut again.
    (1..poly.len().saturating_sub(1))
        .map(|k| ([poly[0].0, poly[k].0, poly[k + 1].0], [poly[0].1, poly[k].1, poly[k + 1].1]))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn push_tri(
    out: &mut Vec<LitTriangle>,
    ctx: &MeshPaintCtx,
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    normal: [f32; 3],
    base: Color32,
    uv: [[f32; 2]; 3],
    surface: &TexDraw<'_>,
) {
    if let Some(sink) = ctx.capture {
        sink.borrow_mut().push([a, b, c]);
        return;
    }
    if let Some(look) = look_through() {
        // Seen from an eye, a face is a back face when the eye is behind its plane.
        let to_eye = [look.eye[0] - a[0], look.eye[1] - a[1], look.eye[2] - a[2]];
        if to_eye[0] * normal[0] + to_eye[1] * normal[1] + to_eye[2] * normal[2] <= 0.0 {
            return;
        }
        let depth_of = |p: [f32; 3]| {
            let rel = [p[0] - look.eye[0], p[1] - look.eye[1], p[2] - look.eye[2]];
            view_depth(rel, ctx.yaw, ctx.pitch)
        };
        let corners = [a, b, c];
        let depths = corners.map(depth_of);
        if depths.iter().all(|d| *d < LOOK_NEAR) {
            return;
        }
        if depths.iter().any(|d| *d < LOOK_NEAR) {
            for (tri, uv) in clip_near(corners, depths, uv) {
                push_tri(out, ctx, tri[0], tri[1], tri[2], normal, base, uv, surface);
            }
            return;
        }
    } else if view_depth(normal, ctx.yaw, ctx.pitch) > 1e-4 {
        // Back faces are hidden: without this a thin box's underside sorts in between
        // its top pieces and shows through as dark triangles.
        return;
    }
    let (pa, za) = project_point(a, ctx.yaw, ctx.pitch, ctx.origin, ctx.cell);
    let (pb, zb) = project_point(b, ctx.yaw, ctx.pitch, ctx.origin, ctx.cell);
    let (pc, zc) = project_point(c, ctx.yaw, ctx.pitch, ctx.origin, ctx.cell);
    // Through an eye, depth must vary like 1/z to interpolate straight across the screen.
    let persp = |z: f32| if look_through().is_some() { -1.0 / z.max(LOOK_NEAR) } else { z };
    let (za, zb, zc) = (persp(za), persp(zb), persp(zc));
    let depth = (za + zb + zc) / 3.0;
    if ctx.gpu {
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
            .sqrt()
            .max(f32::EPSILON);
        let n = [normal[0] / len, normal[1] / len, normal[2] / len];
        let centroid =
            [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0];
        let fill = |col: Color32| match ctx.probes {
            Some(grid) => add_fill(col, base, grid.sample(centroid, n)),
            None => col,
        };
        let color = fill(lit_color_shadowed(base, normal, ctx.light, 1.0));
        let facing = n[0] * ctx.light[0] + n[1] * ctx.light[1] + n[2] * ctx.light[2];
        let (dark, shadow_uvz, shadow_bias) = match ctx.shadow {
            Some(map) if facing > 0.0 => {
                let texel = map.frame.radius * 2.0 / map.size as f32;
                let f = facing.clamp(0.05, 1.0);
                let slope = ((1.0 - f * f).sqrt() / f).min(8.0);
                let uvz = [a, b, c].map(|p| {
                    map.frame.project([
                        p[0] + n[0] * texel,
                        p[1] + n[1] * texel,
                        p[2] + n[2] * texel,
                    ])
                });
                (
                    fill(lit_color_shadowed(base, normal, ctx.light, 0.0)),
                    uvz,
                    (1.0 + 1.5 * slope) / map.size as f32,
                )
            }
            _ => (color, [[0.0; 3]; 3], 0.0),
        };
        out.push(LitTriangle {
            points: [pa, pb, pc],
            depth,
            depths: [za, zb, zc],
            color,
            dark,
            shadow_uvz,
            shadow_bias,
            uv,
            tex_layer: surface.layer,
            tex_slice: surface.slice,
            tex_path: surface.path.to_string(),
            tex_kind: surface.kind,
            normal: n,
            light: ctx.light_gain.max(1.0),
        });
        return;
    }
    let sun = match ctx.shadow {
        Some(map) => {
            let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
                .sqrt()
                .max(f32::EPSILON);
            let n = [normal[0] / len, normal[1] / len, normal[2] / len];
            let facing = n[0] * ctx.light[0] + n[1] * ctx.light[1] + n[2] * ctx.light[2];
            if facing <= 0.0 {
                // Faces turned away from the light get no direct light anyway.
                1.0
            } else {
                let p = [
                    (a[0] + b[0] + c[0]) / 3.0,
                    (a[1] + b[1] + c[1]) / 3.0,
                    (a[2] + b[2] + c[2]) / 3.0,
                ];
                surface_sun(map, p, n, facing)
            }
        }
        None => 1.0,
    };
    let mut color = lit_color_shadowed(base, normal, ctx.light, sun);
    if let Some(grid) = ctx.probes {
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
            .sqrt()
            .max(f32::EPSILON);
        let n = [normal[0] / len, normal[1] / len, normal[2] / len];
        let p =
            [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0];
        color = add_fill(color, base, grid.sample(p, n));
    }
    out.push(LitTriangle {
        points: [pa, pb, pc],
        depth,
        depths: [za, zb, zc],
        color,
        dark: color,
        shadow_uvz: [[0.0; 3]; 3],
        shadow_bias: 0.0,
        uv,
        tex_layer: surface.layer,
        tex_slice: surface.slice,
        tex_path: surface.path.to_string(),
        tex_kind: surface.kind,
        normal: {
            let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
                .sqrt()
                .max(f32::EPSILON);
            [normal[0] / len, normal[1] / len, normal[2] / len]
        },
        light: 1.0,
    });
}

fn box_faces(
    out: &mut Vec<LitTriangle>,
    ctx: &MeshPaintCtx,
    center: [f32; 3],
    half: [f32; 3],
    base: Color32,
    surface: &TexDraw<'_>,
) {
    let [cx, cy, cz] = center;
    let [hx, hy, hz] = half;
    let faces: [([[f32; 3]; 4], [f32; 3]); 6] = [
        (
            [
                [cx - hx, cy - hy, cz + hz],
                [cx + hx, cy - hy, cz + hz],
                [cx + hx, cy + hy, cz + hz],
                [cx - hx, cy + hy, cz + hz],
            ],
            [0.0, 0.0, 1.0],
        ),
        (
            [
                [cx + hx, cy - hy, cz - hz],
                [cx - hx, cy - hy, cz - hz],
                [cx - hx, cy + hy, cz - hz],
                [cx + hx, cy + hy, cz - hz],
            ],
            [0.0, 0.0, -1.0],
        ),
        (
            [
                [cx - hx, cy + hy, cz - hz],
                [cx + hx, cy + hy, cz - hz],
                [cx + hx, cy + hy, cz + hz],
                [cx - hx, cy + hy, cz + hz],
            ],
            [0.0, 1.0, 0.0],
        ),
        (
            [
                [cx - hx, cy - hy, cz + hz],
                [cx + hx, cy - hy, cz + hz],
                [cx + hx, cy - hy, cz - hz],
                [cx - hx, cy - hy, cz - hz],
            ],
            [0.0, -1.0, 0.0],
        ),
        (
            [
                [cx + hx, cy - hy, cz + hz],
                [cx + hx, cy - hy, cz - hz],
                [cx + hx, cy + hy, cz - hz],
                [cx + hx, cy + hy, cz + hz],
            ],
            [1.0, 0.0, 0.0],
        ),
        (
            [
                [cx - hx, cy - hy, cz - hz],
                [cx - hx, cy - hy, cz + hz],
                [cx - hx, cy + hy, cz + hz],
                [cx - hx, cy + hy, cz - hz],
            ],
            [-1.0, 0.0, 0.0],
        ),
    ];
    for (quad, normal) in faces {
        // With shadows on, split big faces so a shadow can fall across part of one.
        // Each direction is split on its own, so thin faces stay a single strip.
        let edge = |i: usize, j: usize| {
            let d = [quad[j][0] - quad[i][0], quad[j][1] - quad[i][1], quad[j][2] - quad[i][2]];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        };
        let split = |len: f32| {
            if ctx.shadow.is_some() {
                ((len / SHADOW_FACE_CELL).ceil() as usize).clamp(1, 24)
            } else {
                1
            }
        };
        let (steps_u, steps_v) = (split(edge(0, 1)), split(edge(1, 2)));
        let at = |u: f32, v: f32| -> [f32; 3] {
            let mut p = [0.0; 3];
            for (k, item) in p.iter_mut().enumerate() {
                let bottom = quad[0][k] + (quad[1][k] - quad[0][k]) * u;
                let top = quad[3][k] + (quad[2][k] - quad[3][k]) * u;
                *item = bottom + (top - bottom) * v;
            }
            p
        };
        let (nu, nv) = (steps_u as f32, steps_v as f32);
        for i in 0..steps_u {
            for j in 0..steps_v {
                let (u0, u1, v0, v1) =
                    (i as f32 / nu, (i + 1) as f32 / nu, j as f32 / nv, (j + 1) as f32 / nv);
                let (p0, p1, p2, p3) = (at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1));
                let uv = |u: f32, v: f32| crate::gpu_textures::tile_uv([u, v], surface.tile);
                refine_piece(
                    out,
                    ctx,
                    [p0, p1, p2, p3],
                    [uv(u0, v0), uv(u1, v0), uv(u1, v1), uv(u0, v1)],
                    normal,
                    base,
                    0,
                    surface,
                );
            }
        }
    }
}

/// Draws one flat piece of a face. Where a shadow edge crosses the piece it is
/// split in four (down to about a tenth of a unit), so shadows keep their shape
/// instead of darkening whole pieces.
fn refine_piece(
    out: &mut Vec<LitTriangle>,
    ctx: &MeshPaintCtx,
    q: [[f32; 3]; 4],
    uv: [[f32; 2]; 4],
    normal: [f32; 3],
    base: Color32,
    level: u32,
    surface: &TexDraw<'_>,
) {
    let mid =
        |a: [f32; 3], b: [f32; 3]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0];
    let mid_uv = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let center = mid(mid(q[0], q[1]), mid(q[2], q[3]));
    if let Some(map) = ctx.shadow {
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
            .sqrt()
            .max(f32::EPSILON);
        let n = [normal[0] / len, normal[1] / len, normal[2] / len];
        let facing = n[0] * ctx.light[0] + n[1] * ctx.light[1] + n[2] * ctx.light[2];
        let span = {
            let d = [q[2][0] - q[0][0], q[2][1] - q[0][1], q[2][2] - q[0][2]];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        };
        if !ctx.gpu && facing > 0.0 && level < 5 && span > 0.15 {
            // Sample a little inside each corner so neighbouring faces don't leak in.
            let inset = |c: [f32; 3]| mid(mid(c, center), c);
            let suns: Vec<f32> = [inset(q[0]), inset(q[1]), inset(q[2]), inset(q[3]), center]
                .iter()
                .map(|&p| surface_sun(map, p, n, facing))
                .collect();
            let lo = suns.iter().copied().fold(f32::INFINITY, f32::min);
            let hi = suns.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            if hi - lo > 0.05 {
                let (e01, e12, e23, e30) =
                    (mid(q[0], q[1]), mid(q[1], q[2]), mid(q[2], q[3]), mid(q[3], q[0]));
                let (u01, u12, u23, u30) = (
                    mid_uv(uv[0], uv[1]),
                    mid_uv(uv[1], uv[2]),
                    mid_uv(uv[2], uv[3]),
                    mid_uv(uv[3], uv[0]),
                );
                let uc = mid_uv(mid_uv(uv[0], uv[1]), mid_uv(uv[2], uv[3]));
                for (sub, suv) in [
                    ([q[0], e01, center, e30], [uv[0], u01, uc, u30]),
                    ([e01, q[1], e12, center], [u01, uv[1], u12, uc]),
                    ([center, e12, q[2], e23], [uc, u12, uv[2], u23]),
                    ([e30, center, e23, q[3]], [u30, uc, u23, uv[3]]),
                ] {
                    refine_piece(out, ctx, sub, suv, normal, base, level + 1, surface);
                }
                return;
            }
        }
    }
    push_tri(out, ctx, q[0], q[1], q[2], normal, base, [uv[0], uv[1], uv[2]], surface);
    push_tri(out, ctx, q[0], q[2], q[3], normal, base, [uv[0], uv[2], uv[3]], surface);
}

fn sphere_tris(
    out: &mut Vec<LitTriangle>,
    ctx: &MeshPaintCtx,
    center: [f32; 3],
    radius: f32,
    base: Color32,
    surface: &TexDraw<'_>,
) {
    let rings = 8;
    let segments = 12;
    for ring in 0..rings {
        let v0 = std::f32::consts::FRAC_PI_2 - std::f32::consts::PI * ring as f32 / rings as f32;
        let v1 =
            std::f32::consts::FRAC_PI_2 - std::f32::consts::PI * (ring + 1) as f32 / rings as f32;
        for seg in 0..segments {
            let u0 = 2.0 * std::f32::consts::PI * seg as f32 / segments as f32;
            let u1 = 2.0 * std::f32::consts::PI * (seg + 1) as f32 / segments as f32;
            let p = |u: f32, v: f32| -> [f32; 3] {
                let cv = v.cos();
                let sv = v.sin();
                let cu = u.cos();
                let su = u.sin();
                [
                    center[0] + radius * cv * cu,
                    center[1] + radius * sv,
                    center[2] + radius * cv * su,
                ]
            };
            let n = |u: f32, v: f32| -> [f32; 3] {
                let cv = v.cos();
                let sv = v.sin();
                let cu = u.cos();
                let su = u.sin();
                [cv * cu, sv, cv * su]
            };
            let a = p(u0, v0);
            let b = p(u1, v0);
            let c = p(u1, v1);
            let d = p(u0, v1);
            let n0 = n((u0 + u1) * 0.5, v0);
            let n1 = n(u1, (v0 + v1) * 0.5);
            let uv = |seg: u32, ring: u32| {
                crate::gpu_textures::lat_long_uv(seg, segments, ring, rings, surface.tile)
            };
            push_tri(
                out,
                ctx,
                a,
                b,
                c,
                n0,
                base,
                [uv(seg, ring), uv(seg + 1, ring), uv(seg + 1, ring + 1)],
                surface,
            );
            push_tri(
                out,
                ctx,
                a,
                c,
                d,
                n1,
                base,
                [uv(seg, ring), uv(seg + 1, ring + 1), uv(seg, ring + 1)],
                surface,
            );
        }
    }
}

fn cylinder_tris(
    out: &mut Vec<LitTriangle>,
    ctx: &MeshPaintCtx,
    center: [f32; 3],
    radius: f32,
    half_h: f32,
    base: Color32,
    surface: &TexDraw<'_>,
) {
    let segments = 16;
    for seg in 0..segments {
        let u0 = 2.0 * std::f32::consts::PI * seg as f32 / segments as f32;
        let u1 = 2.0 * std::f32::consts::PI * (seg + 1) as f32 / segments as f32;
        let p0 = [center[0] + radius * u0.cos(), center[1], center[2] + radius * u0.sin()];
        let p1 = [center[0] + radius * u1.cos(), center[1], center[2] + radius * u1.sin()];
        let p2 = [p1[0], center[1] + half_h, p1[2]];
        let p3 = [p0[0], center[1] + half_h, p0[2]];
        let p4 = [p0[0], center[1] - half_h, p0[2]];
        let p5 = [p1[0], center[1] - half_h, p1[2]];
        let mid = (u0 + u1) * 0.5;
        let side_n = [mid.cos(), 0.0, mid.sin()];
        let side = |seg: u32, v: f32| {
            crate::gpu_textures::tile_uv([seg as f32 / segments as f32, v], surface.tile)
        };
        let cap = |p: [f32; 3]| {
            crate::gpu_textures::planar_xz_uv(
                p[0] - center[0],
                p[2] - center[2],
                radius,
                surface.tile,
            )
        };
        push_tri(
            out,
            ctx,
            p0,
            p1,
            p2,
            side_n,
            base,
            [side(seg as u32, 0.5), side(seg as u32 + 1, 0.5), side(seg as u32 + 1, 1.0)],
            surface,
        );
        push_tri(
            out,
            ctx,
            p0,
            p2,
            p3,
            side_n,
            base,
            [side(seg as u32, 0.5), side(seg as u32 + 1, 1.0), side(seg as u32, 1.0)],
            surface,
        );
        push_tri(
            out,
            ctx,
            p4,
            p5,
            p1,
            side_n,
            base,
            [side(seg as u32, 0.0), side(seg as u32 + 1, 0.0), side(seg as u32 + 1, 0.5)],
            surface,
        );
        push_tri(
            out,
            ctx,
            p4,
            p1,
            p0,
            side_n,
            base,
            [side(seg as u32, 0.0), side(seg as u32 + 1, 0.5), side(seg as u32, 0.5)],
            surface,
        );
        let top = [center[0], center[1] + half_h, center[2]];
        let bottom = [center[0], center[1] - half_h, center[2]];
        push_tri(
            out,
            ctx,
            top,
            p3,
            p2,
            [0.0, 1.0, 0.0],
            base,
            [cap(top), cap(p3), cap(p2)],
            surface,
        );
        push_tri(
            out,
            ctx,
            bottom,
            p5,
            p4,
            [0.0, -1.0, 0.0],
            base,
            [cap(bottom), cap(p5), cap(p4)],
            surface,
        );
    }
}

fn cone_tris(
    out: &mut Vec<LitTriangle>,
    ctx: &MeshPaintCtx,
    center: [f32; 3],
    radius: f32,
    height: f32,
    base: Color32,
    surface: &TexDraw<'_>,
) {
    let apex = [center[0], center[1] + height, center[2]];
    let segments = 16;
    for seg in 0..segments {
        let u0 = 2.0 * std::f32::consts::PI * seg as f32 / segments as f32;
        let u1 = 2.0 * std::f32::consts::PI * (seg + 1) as f32 / segments as f32;
        let b0 = [center[0] + radius * u0.cos(), center[1], center[2] + radius * u0.sin()];
        let b1 = [center[0] + radius * u1.cos(), center[1], center[2] + radius * u1.sin()];
        let mid = (u0 + u1) * 0.5;
        let side = [mid.cos(), height / (radius + height), mid.sin()];
        let uv = |seg: u32, v: f32| {
            crate::gpu_textures::tile_uv([seg as f32 / segments as f32, v], surface.tile)
        };
        let apex_uv = uv(seg as u32, 1.0);
        push_tri(
            out,
            ctx,
            b0,
            b1,
            apex,
            side,
            base,
            [uv(seg as u32, 0.0), uv(seg as u32 + 1, 0.0), apex_uv],
            surface,
        );
        let hub = [center[0], center[1], center[2]];
        let cap = |p: [f32; 3]| {
            crate::gpu_textures::planar_xz_uv(
                p[0] - center[0],
                p[2] - center[2],
                radius,
                surface.tile,
            )
        };
        push_tri(
            out,
            ctx,
            b0,
            b1,
            hub,
            [0.0, -1.0, 0.0],
            base,
            [cap(b0), cap(b1), cap(hub)],
            surface,
        );
    }
}

fn capsule_tris(
    out: &mut Vec<LitTriangle>,
    ctx: &MeshPaintCtx,
    center: [f32; 3],
    radius: f32,
    half_h: f32,
    base: Color32,
    surface: &TexDraw<'_>,
) {
    cylinder_tris(out, ctx, center, radius, half_h, base, surface);
    let top = [center[0], center[1] + half_h, center[2]];
    let bottom = [center[0], center[1] - half_h, center[2]];
    sphere_tris(out, ctx, top, radius, base, surface);
    sphere_tris(out, ctx, bottom, radius, base, surface);
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn collect_lit_triangles(
    marks: &[GridMark<'_>],
    yaw_deg: f32,
    pitch_deg: f32,
    origin: Pos2,
    cell: f32,
    light_dir: [f32; 3],
    shadows: bool,
    fill: &[seal_editor_core::probes::Light],
) -> Vec<LitTriangle> {
    collect_lit_scene(
        marks,
        yaw_deg,
        pitch_deg,
        origin,
        cell,
        light_dir,
        shadows,
        DEFAULT_SHADOW_MAP_SIZE,
        fill,
        false,
    )
    .tris
}

/// Like `collect_lit_triangles`; with `gpu` the shadows are left to the
/// graphics card, which tests the returned shadow map per pixel.
#[allow(clippy::too_many_arguments)]
pub fn collect_lit_scene(
    marks: &[GridMark<'_>],
    yaw_deg: f32,
    pitch_deg: f32,
    origin: Pos2,
    cell: f32,
    light_dir: [f32; 3],
    shadows: bool,
    shadow_map_size: usize,
    fill: &[seal_editor_core::probes::Light],
    gpu: bool,
) -> LitScene {
    collect_lit_scene_gi(
        marks,
        yaw_deg,
        pitch_deg,
        origin,
        cell,
        light_dir,
        shadows,
        shadow_map_size,
        fill,
        gpu,
        false,
        1.0,
    )
}

/// Like `collect_lit_scene`; with `gi` the probes also carry light bounced off the scene.
#[allow(clippy::too_many_arguments)]
pub fn collect_lit_scene_gi(
    marks: &[GridMark<'_>],
    yaw_deg: f32,
    pitch_deg: f32,
    origin: Pos2,
    cell: f32,
    light_dir: [f32; 3],
    shadows: bool,
    shadow_map_size: usize,
    fill: &[seal_editor_core::probes::Light],
    gpu: bool,
    gi: bool,
    brightness: f32,
) -> LitScene {
    let mut out = Vec::new();
    let map = if shadows { shadow_map_for(marks, light_dir, shadow_map_size) } else { None };
    let grid =
        if gi { gi_probe_grid_for(marks, fill, light_dir) } else { probe_grid_for(marks, fill) };
    let ctx = MeshPaintCtx {
        light: light_dir,
        shadow: map.as_ref(),
        probes: grid.as_ref(),
        yaw: yaw_deg,
        pitch: pitch_deg,
        origin,
        cell,
        gpu,
        capture: None,
        light_gain: brightness.max(1.0),
    };
    emit_marks(&mut out, &ctx, marks);
    out.sort_by(|left, right| {
        right.depth.partial_cmp(&left.depth).unwrap_or(std::cmp::Ordering::Equal)
    });
    LitScene { tris: out, shadow: map }
}

/// Every solid mark's triangles, through `push_tri`.
fn emit_marks(out: &mut Vec<LitTriangle>, ctx: &MeshPaintCtx, marks: &[GridMark<'_>]) {
    for mark in marks {
        let Some(primitive) = mark.mesh else { continue };
        if mark.billboard.is_some() || mark.faint {
            continue;
        }
        let base = Color32::from_rgb(mark.tint[0], mark.tint[1], mark.tint[2]);
        let center = [mark.x, mark.y, mark.depth];
        let half = mark.size;
        let surface = TexDraw {
            path: mark.surface.path.unwrap_or(""),
            layer: mark.surface.layer,
            slice: mark.surface.slice,
            tile: mark.surface.tile,
            kind: if mark.surface.path.is_none() {
                crate::gpu_textures::SampleKind::None
            } else {
                mark.surface.kind
            },
        };
        match primitive {
            MeshPrimitive::Cube | MeshPrimitive::Plane => {
                box_faces(out, ctx, center, half, base, &surface);
            }
            MeshPrimitive::Sphere => {
                let radius = half[0].max(half[1]).max(half[2]);
                sphere_tris(out, ctx, center, radius, base, &surface);
            }
            MeshPrimitive::Cylinder => {
                cylinder_tris(out, ctx, center, half[0].max(half[2]), half[1], base, &surface);
            }
            MeshPrimitive::Cone => {
                cone_tris(out, ctx, center, half[0].max(half[2]), half[1] * 2.0, base, &surface);
            }
            MeshPrimitive::Capsule => {
                capsule_tris(
                    out,
                    ctx,
                    center,
                    half[0].max(half[2]),
                    half[1].max(0.1),
                    base,
                    &surface,
                );
            }
        }
    }
}

/// Recolour lit mesh triangles for viewport buffer views.
pub fn apply_buffer_view(tris: &mut [LitTriangle], shade: &str) {
    match shade {
        "base_colour" => {
            for tri in tris {
                tri.dark = tri.color;
            }
        }
        "depth" => {
            let min = tris.iter().map(|t| t.depth).fold(f32::INFINITY, f32::min);
            let max = tris.iter().map(|t| t.depth).fold(f32::NEG_INFINITY, f32::max);
            let span = (max - min).max(1e-3);
            for tri in tris {
                let g = ((tri.depth - min) / span).clamp(0.0, 1.0);
                let byte = (g * 255.0) as u8;
                tri.color = Color32::from_rgb(byte, byte, byte);
                tri.dark = tri.color;
            }
        }
        "normals" => {
            for tri in tris {
                let n = face_normal(tri);
                tri.color = Color32::from_rgb(
                    ((n[0] * 0.5 + 0.5) * 255.0) as u8,
                    ((n[1] * 0.5 + 0.5) * 255.0) as u8,
                    ((n[2] * 0.5 + 0.5) * 255.0) as u8,
                );
                tri.dark = tri.color;
            }
        }
        "overdraw" => {
            for tri in tris {
                tri.color = Color32::from_rgb(70, 110, 220);
                tri.dark = tri.color;
            }
        }
        _ => {}
    }
}

fn face_normal(tri: &LitTriangle) -> [f32; 3] {
    let a = tri.points[0];
    let b = tri.points[1];
    let c = tri.points[2];
    let ab = [b.x - a.x, b.y - a.y, tri.depths[1] - tri.depths[0]];
    let ac = [c.x - a.x, c.y - a.y, tri.depths[2] - tri.depths[0]];
    let n = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
    [n[0] / len, n[1] / len, n[2] / len]
}

#[cfg(test)]
mod look_through_tests {
    use super::*;

    #[test]
    fn a_play_camera_sees_in_perspective_and_cuts_what_is_behind_it() {
        set_look_through(Some(LookThrough { eye: [0.0, 1.0, 0.0], focal_px: 100.0 }));
        // Yaw 0 / pitch 0 looks along +z: a point ahead lands on the centre, and an
        // offset point twice as far away lands half as far from the centre.
        let centre = Pos2::new(400.0, 300.0);
        let (ahead, depth) = project_point([0.0, 1.0, 5.0], 0.0, 0.0, centre, 20.0);
        assert!((ahead - centre).length() < 1e-3 && (depth - 5.0).abs() < 1e-4);
        let near = project_point([1.0, 1.0, 2.0], 0.0, 0.0, centre, 20.0).0;
        let far = project_point([1.0, 1.0, 4.0], 0.0, 0.0, centre, 20.0).0;
        assert!(((near.x - centre.x) - 2.0 * (far.x - centre.x)).abs() < 1e-3);
        set_look_through(None);
        // A triangle reaching behind the eye keeps only its part in front.
        let corners = [[0.0, 0.0, -1.0], [1.0, 0.0, 3.0], [-1.0, 0.0, 3.0]];
        let pieces = clip_near(corners, corners.map(|p| p[2]), [[0.0, 0.0]; 3]);
        assert!(!pieces.is_empty());
        assert!(pieces.iter().flat_map(|(tri, _)| tri).all(|p| p[2] >= LOOK_NEAR));
    }
}

#[cfg(test)]
mod shadow_tests {
    use super::*;

    fn solid(x: f32, y: f32, z: f32, size: [f32; 3]) -> GridMark<'static> {
        GridMark {
            x,
            y,
            half: [size[0], size[1]],
            round: false,
            billboard: None,
            selected: false,
            faint: false,
            label: "",
            tint: [200, 200, 200],
            angle: 0.0,
            depth: z,
            size,
            mesh: Some(MeshPrimitive::Cube),
            pickable: true,
            sprite: None,
            normal_map: None,
            layer: 0,
            uv: [0.0, 0.0, 1.0, 1.0],
            order: 0,
            sprite_tint: [255; 4],
            slice: [0.0; 4],
            slice_mode: "stretch",
            slice_size: [0.0; 2],
            pivot: [0.5, 0.5],
            shape2d: None,
            sprite_shape: None,
            material: None,
            mask_mode: 0,
            mask_kind: 0,
            surface: crate::gpu_textures::MeshSurface::EMPTY,
        }
    }

    #[test]
    fn a_textured_cube_keeps_flat_colour_on_the_cpu_and_stores_uvs_for_the_card() {
        let mut mark = solid(0.0, 0.0, 0.0, [1.0, 1.0, 1.0]);
        mark.surface = crate::gpu_textures::MeshSurface {
            path: Some("art/brick.png"),
            layer: 2,
            slice: 1.0,
            tile: [2.0, 3.0],
            kind: crate::gpu_textures::SampleKind::D2,
        };
        let flat = collect_lit_scene(
            &[solid(0.0, 0.0, 0.0, [1.0, 1.0, 1.0])],
            20.0,
            -30.0,
            egui::Pos2::new(100.0, 100.0),
            40.0,
            [0.2, 1.0, 0.3],
            false,
            64,
            &[],
            false,
        );
        let cpu = collect_lit_scene(
            std::slice::from_ref(&mark),
            20.0,
            -30.0,
            egui::Pos2::new(100.0, 100.0),
            40.0,
            [0.2, 1.0, 0.3],
            false,
            64,
            &[],
            false,
        );
        assert_eq!(cpu.tris[0].color, flat.tris[0].color);
        let gpu = collect_lit_scene(
            std::slice::from_ref(&mark),
            20.0,
            -30.0,
            egui::Pos2::new(100.0, 100.0),
            40.0,
            [0.2, 1.0, 0.3],
            false,
            64,
            &[],
            true,
        );
        assert!(gpu.tris.iter().any(|tri| tri.tex_path == "art/brick.png" && tri.tex_layer == 2));
        assert!(gpu.tris.iter().any(|tri| tri.uv.iter().any(|uv| uv[0] > 1.0 || uv[1] > 1.0)));
    }

    fn scene() -> Vec<GridMark<'static>> {
        vec![solid(0.0, 0.0, 0.0, [5.0, 0.05, 5.0]), solid(0.0, 2.0, 0.0, [0.5, 0.5, 0.5])]
    }

    #[test]
    fn a_box_over_the_ground_casts_a_shadow_under_itself_only() {
        let map = shadow_map_for(&scene(), [0.0, 1.0, 0.0], DEFAULT_SHADOW_MAP_SIZE).expect("map");
        let bias = 1.5 / map.size as f32;
        let ground = |x: f32, z: f32| map.lit([x, 0.1, z], bias, 1);
        assert_eq!(ground(0.0, 0.0), 0.0, "straight under the box is dark");
        assert_eq!(ground(3.0, 3.0), 1.0, "open ground is lit");
        assert_eq!(map.lit([0.0, 2.6, 0.0], bias, 1), 1.0, "the box top is lit");
        // Light from the side moves the shadow away from the side the light comes from.
        let slanted =
            shadow_map_for(&scene(), [1.0, 1.0, 0.0], DEFAULT_SHADOW_MAP_SIZE).expect("map");
        let b = 1.5 / slanted.size as f32;
        assert!(slanted.lit([-2.0, 0.1, 0.0], b, 1) < 0.5, "shadow falls on the far side");
        assert_eq!(slanted.lit([2.0, 0.1, 0.0], b, 1), 1.0, "near side stays lit");
    }

    #[test]
    fn an_empty_scene_has_no_shadow_map() {
        assert!(shadow_map_for(&[], [0.0, 1.0, 0.0], DEFAULT_SHADOW_MAP_SIZE).is_none());
    }

    #[test]
    fn shadows_darken_part_of_the_lit_ground_and_can_be_switched_off() {
        let marks = scene();
        let dark = |tris: &[LitTriangle]| {
            let ambient = (200.0 * (AMBIENT + 0.02)) as u8;
            // Dark screen area, so splitting faces into more pieces does not count.
            tris.iter()
                .filter(|t| t.color.r() <= ambient)
                .map(|t| {
                    let [a, b, c] = t.points;
                    ((b - a).x * (c - a).y - (b - a).y * (c - a).x).abs() / 2.0
                })
                .sum::<f32>()
        };
        let off = collect_lit_triangles(
            &marks,
            0.0,
            -30.0,
            Pos2::ZERO,
            10.0,
            [0.0, 1.0, 0.0],
            false,
            &[],
        );
        let on =
            collect_lit_triangles(&marks, 0.0, -30.0, Pos2::ZERO, 10.0, [0.0, 1.0, 0.0], true, &[]);
        assert!(on.len() > off.len(), "big faces are split so shadows can cover part of them");
        // Only the ground top under the box gets darker: with the light straight above,
        // the brightest ground pieces stay as bright as without shadows.
        let brightest = |tris: &[LitTriangle]| tris.iter().map(|t| t.color.r()).max().unwrap();
        assert_eq!(brightest(&on), brightest(&off));
        let new_dark = dark(&on) - dark(&off);
        assert!(new_dark > 1.0, "some ground is now in shadow");
    }

    #[test]
    fn a_red_fill_light_tints_nearby_faces_and_nothing_without_one() {
        let marks = scene();
        let red = [seal_editor_core::probes::Light::Point {
            pos: [0.0, 1.0, -3.0],
            color: [1.0, 0.0, 0.0],
            range: 6.0,
        }];
        let plain = collect_lit_triangles(
            &marks,
            0.0,
            -30.0,
            Pos2::ZERO,
            10.0,
            [0.0, 1.0, 0.0],
            false,
            &[],
        );
        let lit = collect_lit_triangles(
            &marks,
            0.0,
            -30.0,
            Pos2::ZERO,
            10.0,
            [0.0, 1.0, 0.0],
            false,
            &red,
        );
        assert_eq!(plain.len(), lit.len());
        let reddest = |t: &LitTriangle| i32::from(t.color.r()) - i32::from(t.color.g());
        assert!(plain.iter().all(|t| reddest(t) == 0), "grey stays grey without fill");
        assert!(lit.iter().any(|t| reddest(t) > 20), "faces near the red light turn red");
        assert!(lit.iter().zip(&plain).all(|(a, b)| a.color.g() == b.color.g()), "green untouched");
        assert!(probe_grid_for(&marks, &[]).is_none());
    }

    #[test]
    fn open_ground_under_slanted_light_is_evenly_lit_without_acne() {
        let ground = vec![solid(0.0, 0.0, 0.0, [10.0, 0.025, 10.0])];
        for light in [[0.3, 1.0, 0.2], [1.0, 0.6, 0.4], [-0.8, 0.5, 0.9]] {
            let tris =
                collect_lit_triangles(&ground, 30.0, -35.0, Pos2::ZERO, 10.0, light, true, &[]);
            let off =
                collect_lit_triangles(&ground, 30.0, -35.0, Pos2::ZERO, 10.0, light, false, &[]);
            let shade = |tris: &[LitTriangle]| -> std::collections::BTreeSet<u8> {
                tris.iter().map(|t| t.color.r()).collect()
            };
            // Nothing blocks the light, so shadows on must not darken any piece.
            assert_eq!(shade(&tris), shade(&off), "light {light:?}: self-shadowing");
        }
    }

    #[test]
    fn faces_turned_away_from_the_camera_are_not_drawn() {
        let cube = vec![solid(0.0, 0.0, 0.0, [1.0, 1.0, 1.0])];
        let tris = collect_lit_triangles(
            &cube,
            30.0,
            -35.0,
            Pos2::ZERO,
            10.0,
            [0.0, 1.0, 0.0],
            false,
            &[],
        );
        // A cube seen at an angle shows three faces of two triangles each.
        assert_eq!(tris.len(), 6);
    }

    #[test]
    fn the_angled_outline_hugs_the_drawn_box() {
        let mark = solid(2.0, 0.5, -1.0, [0.5, 0.5, 0.5]);
        let tris = collect_lit_triangles(
            std::slice::from_ref(&mark),
            30.0,
            -35.0,
            Pos2::new(400.0, 300.0),
            40.0,
            [0.0, 1.0, 0.0],
            false,
            &[],
        );
        let hull = mark_hull(&mark, 30.0, -35.0, Pos2::new(400.0, 300.0), 40.0).expect("hull");
        for t in &tris {
            for p in t.points {
                assert!(
                    hull_contains(&hull, p, 0.5),
                    "drawn corner {p:?} lies outside the outline"
                );
            }
        }
        let drawn =
            egui::Rect::from_points(&tris.iter().flat_map(|t| t.points).collect::<Vec<_>>());
        let outline = egui::Rect::from_points(&hull);
        assert!(
            (drawn.min - outline.min).length() < 0.5 && (drawn.max - outline.max).length() < 0.5
        );
        assert!(!hull_contains(&hull, Pos2::new(400.0, 300.0) + egui::vec2(-200.0, 0.0), 0.0));
    }

    #[test]
    fn all_lit_triangles_become_one_plain_mesh() {
        let cube = vec![solid(0.0, 0.0, 0.0, [1.0, 1.0, 1.0])];
        let tris = collect_lit_triangles(
            &cube,
            30.0,
            -35.0,
            Pos2::ZERO,
            10.0,
            [0.0, 1.0, 0.0],
            false,
            &[],
        );
        let mesh = lit_mesh(&tris);
        assert_eq!(mesh.vertices.len(), tris.len() * 3);
        assert_eq!(mesh.indices.len(), tris.len() * 3);
    }
}
