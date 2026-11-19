//! Background texture load state: placeholder first, full image when ready.

use std::sync::{Arc, Mutex};

const PLACEHOLDER: [u8; 4] = [40, 40, 48, 255];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureLoadState {
    Placeholder,
    Ready,
}

#[derive(Clone, Debug)]
struct Inner {
    state: TextureLoadState,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct SharedTextureLoad {
    inner: Arc<Mutex<Inner>>,
}

impl SharedTextureLoad {
    pub fn placeholder() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                state: TextureLoadState::Placeholder,
                width: 1,
                height: 1,
                rgba: PLACEHOLDER.to_vec(),
            })),
        }
    }

    pub fn state(&self) -> TextureLoadState {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).state
    }

    pub fn pixels(&self) -> (u32, u32, Vec<u8>) {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        (g.width, g.height, g.rgba.clone())
    }

    pub fn finish_load(&self, width: u32, height: u32, rgba: Vec<u8>) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.state = TextureLoadState::Ready;
        g.width = width;
        g.height = height;
        g.rgba = rgba;
    }
}

/// Poll-free helper for tests and backends: run a decode closure off-thread, apply later.
pub struct BackgroundTextureSlot {
    pub load: SharedTextureLoad,
}

impl BackgroundTextureSlot {
    pub fn start_placeholder() -> Self {
        Self { load: SharedTextureLoad::placeholder() }
    }

    /// Apply decoded pixels (called from worker or test).
    pub fn apply_decoded(&self, width: u32, height: u32, rgba: Vec<u8>) {
        self.load.finish_load(width, height, rgba);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_decode_returns_placeholder_without_blocking() {
        let slot = BackgroundTextureSlot::start_placeholder();
        let load = slot.load.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(80));
            slot.apply_decoded(4, 4, vec![200; 64]);
        });
        assert_eq!(load.state(), TextureLoadState::Placeholder);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while load.state() != TextureLoadState::Ready && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(load.state(), TextureLoadState::Ready);
    }

    #[test]
    fn loader_returns_placeholder_then_texture() {
        let slot = BackgroundTextureSlot::start_placeholder();
        assert_eq!(slot.load.state(), TextureLoadState::Placeholder);
        let (_, _, px) = slot.load.pixels();
        assert_eq!(px, PLACEHOLDER.to_vec());
        slot.apply_decoded(2, 2, vec![255; 16]);
        assert_eq!(slot.load.state(), TextureLoadState::Ready);
        let (w, h, px) = slot.load.pixels();
        assert_eq!((w, h), (2, 2));
        assert_eq!(px.len(), 16);
    }
}
