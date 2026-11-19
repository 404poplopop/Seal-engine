//! Mip streaming under a memory budget (CPU-side planner; GPU applies loaded mips).

use std::collections::HashMap;

/// How many bytes of resident texture data may be kept at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureMemoryBudget {
    pub max_bytes: u64,
}

impl Default for TextureMemoryBudget {
    fn default() -> Self {
        Self { max_bytes: 128 * 1024 * 1024 }
    }
}

impl TextureMemoryBudget {
    pub fn from_mb(mb: u32) -> Self {
        Self { max_bytes: u64::from(mb) * 1024 * 1024 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StreamTextureId(pub u64);

#[derive(Clone, Debug)]
struct ResidentMip {
    bytes: u64,
    /// Finest mip level currently resident (0 = full resolution).
    finest_loaded: u32,
    max_mip: u32,
    /// Last draw ordering; lower is evicted first under pressure.
    last_draw: u64,
}

#[derive(Clone, Debug, Default)]
pub struct TextureStreamPlanner {
    resident: HashMap<StreamTextureId, ResidentMip>,
    used_bytes: u64,
    draw_gen: u64,
}

impl TextureStreamPlanner {
    pub fn used_bytes(&self) -> u64 {
        self.used_bytes
    }

    /// Register a texture with mip 0 already resident (non-streaming path).
    pub fn register(&mut self, id: StreamTextureId, mip0_bytes: u64, max_mip: u32) {
        self.resident.insert(
            id,
            ResidentMip { bytes: mip0_bytes, finest_loaded: 0, max_mip, last_draw: self.draw_gen },
        );
        self.draw_gen = self.draw_gen.saturating_add(1);
        self.used_bytes += mip0_bytes;
    }

    /// Register for streaming: only the coarsest mip is loaded at first.
    pub fn register_streaming(&mut self, id: StreamTextureId, mip0_bytes: u64, max_mip: u32) {
        let coarse = Self::mip_bytes(mip0_bytes, max_mip);
        self.resident.insert(
            id,
            ResidentMip {
                bytes: mip0_bytes,
                finest_loaded: max_mip,
                max_mip,
                last_draw: self.draw_gen,
            },
        );
        self.draw_gen = self.draw_gen.saturating_add(1);
        self.used_bytes += coarse;
    }

    pub fn finest_loaded(&self, id: StreamTextureId) -> Option<u32> {
        self.resident.get(&id).map(|e| e.finest_loaded)
    }

    pub fn touch(&mut self, id: StreamTextureId) {
        if let Some(entry) = self.resident.get_mut(&id) {
            entry.last_draw = self.draw_gen;
            self.draw_gen = self.draw_gen.saturating_add(1);
        }
    }

    /// Bytes for one mip level if mip0 covers the full image.
    fn mip_bytes(mip0: u64, mip: u32) -> u64 {
        mip0 >> (mip * 2).min(62)
    }

    /// Desired mip from how large the texture appears on screen (pixel coverage).
    pub fn desired_mip(pixel_coverage: f32, base_pixels: u32, max_mip: u32) -> u32 {
        if pixel_coverage <= 0.0 || base_pixels == 0 {
            return max_mip;
        }
        let ratio = pixel_coverage / base_pixels as f32;
        if ratio >= 0.5 {
            0
        } else if ratio >= 0.25 {
            1.min(max_mip)
        } else {
            max_mip
        }
    }

    /// Load finer mips when the texture covers more screen area; returns the new finest mip.
    pub fn raise_mip_for_visible(
        &mut self,
        budget: TextureMemoryBudget,
        id: StreamTextureId,
        pixel_coverage: f32,
        base_pixels: u32,
    ) -> Option<u32> {
        let want = {
            let entry = self.resident.get(&id)?;
            let want = Self::desired_mip(pixel_coverage, base_pixels, entry.max_mip);
            if want >= entry.finest_loaded {
                return None;
            }
            want
        };
        let mip0 = self.resident.get(&id)?.bytes;
        let finest = self.resident.get(&id)?.finest_loaded;
        let extra: u64 = (want..finest).map(|m| Self::mip_bytes(mip0, m)).sum();
        if self.used_bytes + extra > budget.max_bytes {
            self.evict_to_fit(budget, extra);
        }
        if self.used_bytes + extra > budget.max_bytes {
            return None;
        }
        let entry = self.resident.get_mut(&id)?;
        for m in want..entry.finest_loaded {
            self.used_bytes += Self::mip_bytes(mip0, m);
        }
        entry.finest_loaded = want;
        Some(want)
    }

    fn evict_to_fit(&mut self, budget: TextureMemoryBudget, need: u64) {
        let mut ids: Vec<_> = self.resident.keys().copied().collect();
        ids.sort_by_key(|id| self.resident.get(id).map(|e| e.last_draw).unwrap_or(0));
        for id in ids {
            if self.used_bytes + need <= budget.max_bytes {
                break;
            }
            if let Some(entry) = self.resident.get_mut(&id) {
                while entry.finest_loaded < entry.max_mip
                    && self.used_bytes + need > budget.max_bytes
                {
                    self.used_bytes = self
                        .used_bytes
                        .saturating_sub(Self::mip_bytes(entry.bytes, entry.finest_loaded));
                    entry.finest_loaded += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_stays_under_budget() {
        let mut planner = TextureStreamPlanner::default();
        let budget = TextureMemoryBudget::from_mb(1);
        planner.register(StreamTextureId(1), 64 * 64 * 4, 2);
        planner.register(StreamTextureId(2), 64 * 64 * 4, 2);
        assert!(planner.used_bytes() <= budget.max_bytes);
    }

    #[test]
    fn large_on_screen_keeps_mip0() {
        let mip = TextureStreamPlanner::desired_mip(400_000.0, 512 * 512, 4);
        assert_eq!(mip, 0);
        let mip_small = TextureStreamPlanner::desired_mip(100.0, 512 * 512, 4);
        assert!(mip_small >= 1);
    }

    #[test]
    fn many_textures_stay_under_budget() {
        let mut planner = TextureStreamPlanner::default();
        let budget = TextureMemoryBudget::from_mb(2);
        let mip0 = 256u64 * 256 * 4;
        for i in 0..20u64 {
            planner.register_streaming(StreamTextureId(i), mip0, 3);
        }
        assert!(planner.used_bytes() <= budget.max_bytes);
    }

    #[test]
    fn streaming_off_keeps_full_mip() {
        let mut planner = TextureStreamPlanner::default();
        let mip0 = 128u64 * 128 * 4;
        planner.register(StreamTextureId(1), mip0, 2);
        assert_eq!(planner.finest_loaded(StreamTextureId(1)), Some(0));
    }

    #[test]
    fn streaming_raises_mip_when_large_on_screen() {
        let mut planner = TextureStreamPlanner::default();
        let budget = TextureMemoryBudget::from_mb(32);
        let mip0 = 512u64 * 512 * 4;
        planner.register_streaming(StreamTextureId(1), mip0, 4);
        assert_eq!(planner.finest_loaded(StreamTextureId(1)), Some(4));
        let raised =
            planner.raise_mip_for_visible(budget, StreamTextureId(1), 400_000.0, 512 * 512);
        assert_eq!(raised, Some(0));
        assert_eq!(planner.finest_loaded(StreamTextureId(1)), Some(0));
        assert!(planner.used_bytes() <= budget.max_bytes);
    }
}
