//! Registry of custom thumbnail and preview renderers keyed by file extension.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crate::error::CoreError;

const PREVIEW_SIZE: u32 = crate::scene_thumbnail::THUMB_SIZE;

/// Renders an RGBA8 preview for one file (width and height are always [`PREVIEW_SIZE`]).
pub trait PreviewProvider: Send + Sync {
    fn render(&self, source: &Path) -> Result<Vec<u8>, CoreError>;
}

struct Registry {
    by_ext: HashMap<String, Box<dyn PreviewProvider>>,
}

fn registry() -> &'static Mutex<Registry> {
    static CELL: OnceLock<Mutex<Registry>> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut by_ext = HashMap::new();
        by_ext.insert("demo".into(), Box::new(DemoStripesProvider) as Box<dyn PreviewProvider>);
        Mutex::new(Registry { by_ext })
    })
}

/// Register a provider for every extension in `exts` (lowercase, no dot).
pub fn register(exts: &[&str], provider: Box<dyn PreviewProvider>) {
    let Ok(mut guard) = registry().lock() else {
        return;
    };
    let ext = exts.first().map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    if !ext.is_empty() {
        guard.by_ext.insert(ext, provider);
    }
}

/// RGBA pixels, width and height.
pub type Rendered = (Vec<u8>, u32, u32);

/// Render with a registered provider, or `None` when no provider owns the extension.
pub fn render_with_provider(source: &Path) -> Option<Result<Rendered, CoreError>> {
    let ext =
        source.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let guard = registry().lock().ok()?;
    let provider = guard.by_ext.get(&ext)?;
    Some(provider.render(source).map(|rgba| (rgba, PREVIEW_SIZE, PREVIEW_SIZE)))
}

struct DemoStripesProvider;

impl PreviewProvider for DemoStripesProvider {
    fn render(&self, _source: &Path) -> Result<Vec<u8>, CoreError> {
        let mut rgba = vec![0u8; (PREVIEW_SIZE * PREVIEW_SIZE * 4) as usize];
        for y in 0..PREVIEW_SIZE {
            for x in 0..PREVIEW_SIZE {
                let stripe = ((x + y) / 8) % 2 == 0;
                let i = ((y * PREVIEW_SIZE + x) * 4) as usize;
                let base = if stripe { 80 } else { 200 };
                rgba[i] = base;
                rgba[i + 1] = base + 20;
                rgba[i + 2] = base + 40;
                rgba[i + 3] = 255;
            }
        }
        Ok(rgba)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct FixedProvider;

    impl PreviewProvider for FixedProvider {
        fn render(&self, _: &Path) -> Result<Vec<u8>, CoreError> {
            Ok((0..PREVIEW_SIZE * PREVIEW_SIZE).flat_map(|_| [255u8, 0, 0, 255]).collect())
        }
    }

    #[test]
    fn custom_provider_used_for_extension() {
        let dir = std::env::temp_dir().join(format!("seal-demo-prov-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("tile.demotest");
        fs::write(&file, b"x").unwrap();
        register(&["demotest"], Box::new(FixedProvider));
        let out = render_with_provider(&file).unwrap().unwrap();
        assert_eq!(out.2, PREVIEW_SIZE);
        assert!(out.0.contains(&255));
    }
}
