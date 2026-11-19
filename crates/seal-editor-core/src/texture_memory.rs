//! Project texture memory budget and CPU-side mip streaming planner (shared with the viewport).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex};

use seal_render::texture_stream::{StreamTextureId, TextureMemoryBudget, TextureStreamPlanner};

pub const TEXTURE_MEMORY_BUDGET_MB: &str = "rendering.texture_memory_budget_mb";
pub const DEFAULT_TEXTURE_MEMORY_BUDGET_MB: u32 = 512;

static BUDGET_MB: AtomicU32 = AtomicU32::new(DEFAULT_TEXTURE_MEMORY_BUDGET_MB);

static PLANNER: LazyLock<Mutex<TextureStreamPlanner>> =
    LazyLock::new(|| Mutex::new(TextureStreamPlanner::default()));

pub fn sync_budget_mb(mb: u32) {
    BUDGET_MB.store(mb.max(1), Ordering::Relaxed);
}

pub fn budget_mb() -> u32 {
    BUDGET_MB.load(Ordering::Relaxed)
}

pub fn budget_bytes() -> TextureMemoryBudget {
    TextureMemoryBudget::from_mb(budget_mb())
}

pub fn stream_id_for_path(path: &str) -> StreamTextureId {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    StreamTextureId(hasher.finish())
}

pub fn with_planner<R>(f: impl FnOnce(&mut TextureStreamPlanner) -> R) -> R {
    let mut guard = PLANNER.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

pub fn register_full_res(path: &str, mip0_bytes: u64, max_mip: u32) {
    let id = stream_id_for_path(path);
    with_planner(|p| p.register(id, mip0_bytes, max_mip));
}

pub fn register_streaming(path: &str, mip0_bytes: u64, max_mip: u32) {
    let id = stream_id_for_path(path);
    with_planner(|p| p.register_streaming(id, mip0_bytes, max_mip));
}

pub fn touch_drawn(path: &str) {
    let id = stream_id_for_path(path);
    with_planner(|p| p.touch(id));
}

pub fn raise_for_visible(path: &str, pixel_coverage: f32, base_pixels: u32) -> Option<u32> {
    let id = stream_id_for_path(path);
    with_planner(|p| {
        p.touch(id);
        p.raise_mip_for_visible(budget_bytes(), id, pixel_coverage, base_pixels)
    })
}

pub fn finest_loaded_mip(path: &str) -> Option<u32> {
    let id = stream_id_for_path(path);
    with_planner(|p| p.finest_loaded(id))
}

pub fn memory_stats() -> (u64, u64) {
    with_planner(|p| (p.used_bytes(), budget_bytes().max_bytes))
}

pub fn log_picture_load_failed(path: &str, detail: &str) {
    seal_log::warn!("picture load failed: {path}: {detail}");
}
/// A texture problem the viewport hit (shown in the Message Log).
pub fn log_texture_warning(detail: &str) {
    seal_log::warn!("{detail}");
}
