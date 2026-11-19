//! Background sprite decoding and mip streaming for viewport pictures.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::SystemTime;

use egui::{ColorImage, Context, TextureHandle, TextureId, TextureOptions};

use crate::sprites::{drawn_rgba, shrink, texture_options};

const PLACEHOLDER_RGBA: [u8; 4] = [40, 40, 48, 128];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryState {
    Placeholder,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Default)]
struct PicturePolicy {
    streaming: bool,
    #[allow(dead_code)]
    budget_mb: u32,
}

static POLICIES: LazyLock<std::sync::Mutex<HashMap<String, PicturePolicy>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

pub fn set_picture_policy(path: &str, streaming: bool, budget_mb: u32) {
    if let Ok(mut map) = POLICIES.lock() {
        map.insert(path.to_string(), PicturePolicy { streaming, budget_mb: budget_mb.max(1) });
    }
}

fn policy(path: &str) -> PicturePolicy {
    POLICIES.lock().ok().and_then(|m| m.get(path).copied()).unwrap_or(PicturePolicy {
        streaming: true,
        budget_mb: seal_editor_core::texture_memory::budget_mb(),
    })
}

pub fn sync_project_budget(mb: u32) {
    seal_editor_core::texture_memory::sync_budget_mb(mb);
}

fn max_mip_levels(w: u32, h: u32) -> u32 {
    let side = w.max(h).max(1);
    32u32.saturating_sub(side.ilog2())
}

fn mip_downscale(rgba: &[u8], w: u32, h: u32, finest: u32) -> (Vec<u8>, u32, u32) {
    if finest == 0 {
        return (rgba.to_vec(), w, h);
    }
    let mut data = rgba.to_vec();
    let mut cw = w;
    let mut ch = h;
    for _ in 0..finest {
        let (next, nw, nh) = shrink(&data, cw, ch, (cw.max(ch) / 2).max(1));
        data = next;
        cw = nw;
        ch = nh;
    }
    (data, cw, ch)
}

struct SpriteDone {
    path: String,
    mtime: Option<SystemTime>,
    side: Option<SystemTime>,
    nearest: bool,
    pixels: Option<(Vec<u8>, u32, u32)>,
    err: Option<String>,
}

struct SpriteWorker {
    jobs: std::sync::mpsc::Sender<(String, Option<SystemTime>, Option<SystemTime>, bool)>,
    done: std::sync::mpsc::Receiver<SpriteDone>,
    pending: HashSet<String>,
}

fn sprite_worker() -> &'static std::sync::Mutex<SpriteWorker> {
    static WORKER: std::sync::OnceLock<std::sync::Mutex<SpriteWorker>> = std::sync::OnceLock::new();
    WORKER.get_or_init(|| {
        let (job_tx, job_rx) =
            std::sync::mpsc::channel::<(String, Option<SystemTime>, Option<SystemTime>, bool)>();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<SpriteDone>();
        std::thread::Builder::new()
            .name("seal-sprite-tex".into())
            .spawn(move || {
                for (path, mtime, side, nearest) in job_rx {
                    let decoded = drawn_rgba(Path::new(&path));
                    let (pixels, err) = match decoded {
                        Some((rgba, w, h)) => (Some((rgba, w, h)), None),
                        None => (None, Some("decode failed".into())),
                    };
                    if done_tx.send(SpriteDone { path, mtime, side, nearest, pixels, err }).is_err()
                    {
                        break;
                    }
                }
            })
            .expect("sprite texture worker");
        std::sync::Mutex::new(SpriteWorker { jobs: job_tx, done: done_rx, pending: HashSet::new() })
    })
}

fn placeholder_image(w: u32, h: u32) -> ColorImage {
    let w = w.max(1) as usize;
    let h = h.max(1) as usize;
    let px: Vec<u8> = std::iter::repeat_n(PLACEHOLDER_RGBA, w * h).flatten().collect();
    ColorImage::from_rgba_unmultiplied([w, h], &px)
}

#[derive(Clone, Default)]
pub struct RuntimeSpriteCache(pub Arc<std::sync::Mutex<HashMap<String, SpriteRuntimeEntry>>>);

#[derive(Clone)]
pub struct SpriteRuntimeEntry {
    pub mtime: Option<SystemTime>,
    pub side: Option<SystemTime>,
    pub nearest: bool,
    pub handle: Option<TextureHandle>,
    pub state: EntryState,
    pub base_w: u32,
    pub base_h: u32,
    pub uploaded_mip: u32,
    pub full_rgba: Option<Arc<(Vec<u8>, u32, u32)>>,
}

pub fn pending_decode_count() -> usize {
    sprite_worker().lock().ok().map(|w| w.pending.len()).unwrap_or(0)
}

pub fn tick_play_preload(ctx: &Context, playing: bool, paths: &[String]) {
    static WAS_PLAYING: AtomicBool = AtomicBool::new(false);
    let was = WAS_PLAYING.swap(playing, Ordering::Relaxed);
    if playing && !was {
        for path in paths {
            let nearest = seal_editor_core::import_presets::picture_look(Path::new(path)).nearest;
            let _ = super::sprites::texture_filtered(ctx, path, nearest);
        }
    }
}

pub fn note_sprite_coverage(path: &str, pixel_area: f32, base_w: u32, base_h: u32) {
    let pol = policy(path);
    if !pol.streaming {
        seal_editor_core::texture_memory::touch_drawn(path);
        return;
    }
    seal_editor_core::texture_memory::raise_for_visible(
        path,
        pixel_area,
        base_w.saturating_mul(base_h),
    );
}

fn apply_mip_to_handle(
    entry: &mut SpriteRuntimeEntry,
    path: &str,
    opts: TextureOptions,
    ctx: &Context,
) {
    if entry.state != EntryState::Ready {
        return;
    }
    let Some(full) = entry.full_rgba.as_ref() else { return };
    let (rgba, w, h) = &**full;
    let pol = policy(path);
    let finest = if pol.streaming {
        seal_editor_core::texture_memory::finest_loaded_mip(path).unwrap_or(max_mip_levels(*w, *h))
    } else {
        0
    };
    if finest == entry.uploaded_mip && entry.state == EntryState::Ready {
        return;
    }
    let (pixels, uw, uh) = mip_downscale(rgba, *w, *h, finest);
    let image = ColorImage::from_rgba_unmultiplied([uw as usize, uh as usize], &pixels);
    match &mut entry.handle {
        Some(handle) => handle.set(image, opts),
        None => {
            entry.handle = Some(ctx.load_texture(format!("sprite:{path}"), image, opts));
        }
    }
    entry.uploaded_mip = finest;
    entry.base_w = *w;
    entry.base_h = *h;
}

pub fn finish_worker_jobs(ctx: &Context, cache: &RuntimeSpriteCache) {
    let Ok(mut worker) = sprite_worker().lock() else { return };
    let Ok(mut map) = cache.0.lock() else { return };
    while let Ok(done) = worker.done.try_recv() {
        worker.pending.remove(&done.path);
        let look = seal_editor_core::import_presets::picture_look(Path::new(&done.path));
        let opts = texture_options(done.nearest, &look);
        let pol = policy(&done.path);
        let entry = map.entry(done.path.clone()).or_insert_with(|| SpriteRuntimeEntry {
            mtime: done.mtime,
            side: done.side,
            nearest: done.nearest,
            handle: None,
            state: EntryState::Placeholder,
            base_w: 1,
            base_h: 1,
            uploaded_mip: u32::MAX,
            full_rgba: None,
        });
        entry.mtime = done.mtime;
        entry.side = done.side;
        entry.nearest = done.nearest;
        if let Some((rgba, w, h)) = done.pixels {
            entry.base_w = w;
            entry.base_h = h;
            entry.full_rgba = Some(Arc::new((rgba.clone(), w, h)));
            let mip0 = u64::from(w) * u64::from(h) * 4;
            let max_mip = max_mip_levels(w, h);
            if pol.streaming {
                seal_editor_core::texture_memory::register_streaming(&done.path, mip0, max_mip);
            } else {
                seal_editor_core::texture_memory::register_full_res(&done.path, mip0, max_mip);
            }
            apply_mip_to_handle(entry, &done.path, opts, ctx);
            entry.state = EntryState::Ready;
        } else {
            let msg = done.err.unwrap_or_else(|| "unknown".into());
            seal_editor_core::texture_memory::log_picture_load_failed(&done.path, &msg);
            entry.state = EntryState::Failed;
            if entry.handle.is_none() {
                entry.handle = Some(ctx.load_texture(
                    format!("sprite:{}", done.path),
                    placeholder_image(1, 1),
                    opts,
                ));
            }
        }
        ctx.request_repaint();
    }
}

pub fn queue_async_load(
    ctx: &Context,
    cache: &RuntimeSpriteCache,
    path: &str,
    nearest: bool,
    mtime: Option<SystemTime>,
    side: Option<SystemTime>,
) -> Option<TextureId> {
    let look = seal_editor_core::import_presets::picture_look(Path::new(path));
    let options = texture_options(nearest, &look);
    let mut map = cache.0.lock().ok()?;
    if let Some(entry) = map.get(path) {
        if entry.mtime == mtime && entry.side == side && entry.nearest == nearest {
            return entry.handle.as_ref().map(|h| h.id());
        }
    }
    let (pw, ph) =
        seal_editor_core::picture_import::file_dimensions(Path::new(path)).unwrap_or((1, 1));
    let image = placeholder_image(pw, ph);
    let handle = ctx.load_texture(format!("sprite:{path}"), image, options);
    let id = handle.id();
    map.insert(
        path.to_string(),
        SpriteRuntimeEntry {
            mtime,
            side,
            nearest,
            handle: Some(handle),
            state: EntryState::Placeholder,
            base_w: pw,
            base_h: ph,
            uploaded_mip: u32::MAX,
            full_rgba: None,
        },
    );
    let mut worker = sprite_worker().lock().ok()?;
    if worker.pending.insert(path.to_string()) {
        let _ = worker.jobs.send((path.to_string(), mtime, side, nearest));
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(16));
    Some(id)
}

pub fn refresh_streaming(ctx: &Context, cache: &RuntimeSpriteCache, path: &str) {
    let Ok(mut map) = cache.0.lock() else { return };
    let Some(mut entry) = map.get(path).cloned() else { return };
    if entry.state != EntryState::Ready {
        return;
    }
    let look = seal_editor_core::import_presets::picture_look(Path::new(path));
    let opts = texture_options(entry.nearest, &look);
    apply_mip_to_handle(&mut entry, path, opts, ctx);
    map.insert(path.to_string(), entry);
}
