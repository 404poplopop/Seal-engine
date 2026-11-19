//! Memory for Seal: every allocation has an owner tag.
//!
//! * [`TrackingAllocator`] wraps any global allocator and counts live bytes,
//!   peak bytes and allocation counts per [`MemTag`]. Code enters a tag with
//!   [`tagged`]; untagged allocations count as [`MemTag::Untagged`].
//! * [`Budgets`] holds a byte limit per tag; [`over_budget`] lists the tags
//!   past their limit so the editor can show them and CI can fail on them.
//! * [`FrameArenas`] are double-buffered bump arenas for transient per-frame
//!   data. Data from frame N stays valid during frame N+1, then is reset.

use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

/// Who owns an allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum MemTag {
    Untagged = 0,
    Core,
    World,
    Render,
    Physics,
    Audio,
    Assets,
    Scripts,
    Editor,
    Network,
}

impl MemTag {
    pub const ALL: [MemTag; TAGS] = [
        MemTag::Untagged,
        MemTag::Core,
        MemTag::World,
        MemTag::Render,
        MemTag::Physics,
        MemTag::Audio,
        MemTag::Assets,
        MemTag::Scripts,
        MemTag::Editor,
        MemTag::Network,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MemTag::Untagged => "untagged",
            MemTag::Core => "core",
            MemTag::World => "world",
            MemTag::Render => "render",
            MemTag::Physics => "physics",
            MemTag::Audio => "audio",
            MemTag::Assets => "assets",
            MemTag::Scripts => "scripts",
            MemTag::Editor => "editor",
            MemTag::Network => "network",
        }
    }
}

pub const TAGS: usize = 10;

struct TagCounters {
    live: AtomicU64,
    peak: AtomicU64,
    allocations: AtomicU64,
}

#[allow(clippy::declare_interior_mutable_const)]
const ZERO: TagCounters = TagCounters {
    live: AtomicU64::new(0),
    peak: AtomicU64::new(0),
    allocations: AtomicU64::new(0),
};

static COUNTERS: [TagCounters; TAGS] = [ZERO; TAGS];

thread_local! {
    static CURRENT_TAG: Cell<MemTag> = const { Cell::new(MemTag::Untagged) };
}

/// The tag new allocations on this thread are charged to.
pub fn current_tag() -> MemTag {
    CURRENT_TAG.try_with(Cell::get).unwrap_or(MemTag::Untagged)
}

/// Run `body` with allocations on this thread charged to `tag`.
pub fn tagged<R>(tag: MemTag, body: impl FnOnce() -> R) -> R {
    struct Restore(MemTag);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = CURRENT_TAG.try_with(|c| c.set(self.0));
        }
    }
    let _restore = Restore(CURRENT_TAG.with(|c| c.replace(tag)));
    body()
}

fn charge(tag: MemTag, bytes: u64) {
    let counters = &COUNTERS[tag as usize];
    let live = counters.live.fetch_add(bytes, Ordering::Relaxed) + bytes;
    counters.peak.fetch_max(live, Ordering::Relaxed);
    counters.allocations.fetch_add(1, Ordering::Relaxed);
}

fn release(tag: MemTag, bytes: u64) {
    COUNTERS[tag as usize].live.fetch_sub(bytes, Ordering::Relaxed);
}

/// Usage of one tag at the moment of [`snapshot`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TagUsage {
    pub tag: MemTag,
    pub live_bytes: u64,
    pub peak_bytes: u64,
    pub allocations: u64,
}

/// Live usage of every tag. All zero unless [`TrackingAllocator`] is the global allocator.
pub fn snapshot() -> Vec<TagUsage> {
    MemTag::ALL
        .iter()
        .map(|&tag| {
            let c = &COUNTERS[tag as usize];
            TagUsage {
                tag,
                live_bytes: c.live.load(Ordering::Relaxed),
                peak_bytes: c.peak.load(Ordering::Relaxed),
                allocations: c.allocations.load(Ordering::Relaxed),
            }
        })
        .collect()
}

/// Byte limits per tag. A tag without a limit is never over budget.
#[derive(Clone, Debug, Default)]
pub struct Budgets {
    limits: [Option<u64>; TAGS],
}

impl Budgets {
    pub fn with(mut self, tag: MemTag, bytes: u64) -> Self {
        self.limits[tag as usize] = Some(bytes);
        self
    }

    pub fn limit(&self, tag: MemTag) -> Option<u64> {
        self.limits[tag as usize]
    }
}

/// Tags whose live bytes exceed their budget: (usage, limit).
pub fn over_budget(budgets: &Budgets, usage: &[TagUsage]) -> Vec<(TagUsage, u64)> {
    usage
        .iter()
        .filter_map(|u| {
            budgets.limit(u.tag).filter(|&limit| u.live_bytes > limit).map(|limit| (*u, limit))
        })
        .collect()
}

/// A global allocator that charges every allocation to the current tag.
///
/// Each block carries a small header holding its tag, so a block freed on
/// another thread, or under another tag, is still credited back correctly.
pub struct TrackingAllocator<A> {
    inner: A,
}

impl<A> TrackingAllocator<A> {
    pub const fn new(inner: A) -> Self {
        Self { inner }
    }
}

/// Space in front of every block; large enough for any alignment up to 16.
const HEADER: usize = 16;

fn with_header(layout: Layout) -> Option<(Layout, usize)> {
    let align = layout.align().max(HEADER);
    let offset = HEADER.max(align);
    let size = layout.size().checked_add(offset)?;
    Layout::from_size_align(size, align).ok().map(|l| (l, offset))
}

// SAFETY: every method forwards to `inner` with a layout that is a valid
// superset of the caller's, and hands back a pointer `offset` bytes into it,
// aligned to the caller's alignment because `offset` is a multiple of it.
unsafe impl<A: GlobalAlloc> GlobalAlloc for TrackingAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let Some((outer, offset)) = with_header(layout) else {
            return std::ptr::null_mut();
        };
        let base = self.inner.alloc(outer);
        if base.is_null() {
            return base;
        }
        let tag = current_tag();
        base.add(offset - 1).write(tag as u8);
        charge(tag, layout.size() as u64);
        base.add(offset)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let (outer, offset) = with_header(layout).expect("layout was valid at alloc");
        let tag = MemTag::ALL[ptr.sub(1).read() as usize];
        release(tag, layout.size() as u64);
        self.inner.dealloc(ptr.sub(offset), outer);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
        let new_ptr = self.alloc(new_layout);
        if !new_ptr.is_null() {
            std::ptr::copy_nonoverlapping(ptr, new_ptr, layout.size().min(new_size));
            self.dealloc(ptr, layout);
        }
        new_ptr
    }
}

/// Double-buffered bump arenas for per-frame transient data.
pub struct FrameArenas {
    arenas: [bumpalo::Bump; 2],
    frame: u64,
}

impl Default for FrameArenas {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameArenas {
    pub fn new() -> Self {
        Self { arenas: [bumpalo::Bump::new(), bumpalo::Bump::new()], frame: 0 }
    }

    /// The arena for the current frame.
    pub fn current(&self) -> &bumpalo::Bump {
        &self.arenas[(self.frame % 2) as usize]
    }

    /// The arena of the previous frame, still readable this frame.
    pub fn previous(&self) -> &bumpalo::Bump {
        &self.arenas[((self.frame + 1) % 2) as usize]
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Start the next frame: the arena from two frames ago is reset and reused.
    /// Needs `&mut self`, so nothing can still borrow from it.
    pub fn advance(&mut self) {
        self.frame += 1;
        self.arenas[(self.frame % 2) as usize].reset();
    }

    /// Bytes held by both arenas (capacity, not just what is in use).
    pub fn allocated_bytes(&self) -> usize {
        self.arenas.iter().map(bumpalo::Bump::allocated_bytes).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_arenas_keep_the_previous_frame_and_reuse_older_memory() {
        let mut frames = FrameArenas::new();
        let capacity = {
            let v = bumpalo::collections::Vec::from_iter_in(0..10_000u32, frames.current());
            assert_eq!(v.len(), 10_000);
            frames.allocated_bytes()
        };
        frames.advance();
        assert!(frames.previous().allocated_bytes() > 0);
        frames.advance();
        // Frame 0's arena was reset and reused: no growth past the first frame's size.
        let _ = bumpalo::collections::Vec::from_iter_in(0..10_000u32, frames.current());
        assert!(frames.allocated_bytes() <= capacity * 2 + 4096);
        assert_eq!(frames.frame(), 2);
    }

    #[test]
    fn budgets_report_only_tags_past_their_limit() {
        let usage = vec![
            TagUsage { tag: MemTag::Render, live_bytes: 900, peak_bytes: 900, allocations: 3 },
            TagUsage { tag: MemTag::Audio, live_bytes: 50, peak_bytes: 80, allocations: 1 },
            TagUsage {
                tag: MemTag::Editor,
                live_bytes: 1 << 30,
                peak_bytes: 1 << 30,
                allocations: 1,
            },
        ];
        let budgets = Budgets::default().with(MemTag::Render, 512).with(MemTag::Audio, 512);
        let over = over_budget(&budgets, &usage);
        assert_eq!(over.len(), 1);
        assert_eq!(over[0].0.tag, MemTag::Render);
        assert_eq!(over[0].1, 512);
    }

    #[test]
    fn tagged_restores_the_previous_tag_even_on_panic() {
        assert_eq!(current_tag(), MemTag::Untagged);
        tagged(MemTag::Physics, || {
            assert_eq!(current_tag(), MemTag::Physics);
            let _ = std::panic::catch_unwind(|| tagged(MemTag::Audio, || panic!("boom")));
            assert_eq!(current_tag(), MemTag::Physics);
        });
        assert_eq!(current_tag(), MemTag::Untagged);
    }
}
