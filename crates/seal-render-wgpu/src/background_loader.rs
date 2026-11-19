//! Decode texture pixels on background worker threads.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use seal_jobs::{JobPool, Priority};
use seal_render::texture_load::{BackgroundTextureSlot, SharedTextureLoad, TextureLoadState};

pub struct PendingBackgroundLoad {
    pub slot: BackgroundTextureSlot,
    pub path: PathBuf,
}

#[derive(Default)]
pub struct BackgroundTextureLoader {
    pool: Option<Arc<JobPool>>,
    pending: Vec<PendingBackgroundLoad>,
}

impl BackgroundTextureLoader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pool(pool: Arc<JobPool>) -> Self {
        Self { pool: Some(pool), pending: Vec::new() }
    }

    /// Returns immediately with a placeholder; decoding runs on a worker thread.
    pub fn queue_path(&mut self, path: impl AsRef<Path>) -> SharedTextureLoad {
        let slot = BackgroundTextureSlot::start_placeholder();
        let load = slot.load.clone();
        let path = path.as_ref().to_path_buf();
        let read_path = path.clone();
        let decode = move || {
            if let Ok(bytes) = std::fs::read(&read_path) {
                if let Ok(img) = image::load_from_memory(&bytes) {
                    let rgba = img.to_rgba8();
                    let (w, h) = (rgba.width(), rgba.height());
                    slot.apply_decoded(w, h, rgba.into_raw());
                    return;
                }
            }
            slot.apply_decoded(1, 1, vec![40, 40, 48, 255]);
        };
        if let Some(pool) = &self.pool {
            pool.spawn(Priority::Background, decode);
        } else {
            decode();
        }
        self.pending.push(PendingBackgroundLoad {
            slot: BackgroundTextureSlot { load: load.clone() },
            path,
        });
        load
    }

    pub fn poll_ready(&mut self) -> usize {
        let n = self
            .pending
            .iter()
            .filter(|p| p.slot.load.state() == TextureLoadState::Ready)
            .count();
        self.pending.retain(|p| p.slot.load.state() != TextureLoadState::Ready);
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn background_returns_placeholder_first() {
        let pool = Arc::new(JobPool::new(1));
        let mut loader = BackgroundTextureLoader::with_pool(pool);
        let missing = loader.queue_path("/nonexistent/picture.png");
        assert_eq!(missing.state(), TextureLoadState::Placeholder);
    }

    #[test]
    fn background_finishes_png_on_pool() {
        let dir = std::env::temp_dir().join("seal-tex");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("px.png");
        {
            let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]));
            let mut f = std::fs::File::create(&path).unwrap();
            let mut buf = Vec::new();
            image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();
            f.write_all(&buf).unwrap();
        }
        let pool = Arc::new(JobPool::new(1));
        let mut loader = BackgroundTextureLoader::with_pool(pool);
        let load = loader.queue_path(&path);
        assert_eq!(load.state(), TextureLoadState::Placeholder);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while load.state() != TextureLoadState::Ready && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(load.state(), TextureLoadState::Ready);
        let (w, h, _) = load.pixels();
        assert_eq!((w, h), (2, 2));
        let _ = std::fs::remove_dir_all(&dir);
    }
}