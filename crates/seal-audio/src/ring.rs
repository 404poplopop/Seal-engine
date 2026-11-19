//! Single-producer, single-consumer stereo ring.
//!
//! The game thread mixes into the ring. The output callback only reads it.
//! Indices are published with a release store and observed with an acquire
//! load, so a slot is never read while it is being written. A cursor whose
//! gap is larger than the capacity is treated as empty instead of reading
//! off the end of the buffer.

use std::cell::UnsafeCell;

#[cfg(target_has_atomic = "ptr")]
mod index {
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub struct Index(AtomicUsize);

    impl Index {
        pub fn new(value: usize) -> Self {
            Self(AtomicUsize::new(value))
        }

        pub fn load(&self) -> usize {
            self.0.load(Ordering::Acquire)
        }

        pub fn store(&self, value: usize) {
            self.0.store(value, Ordering::Release);
        }
    }
}

#[cfg(not(target_has_atomic = "ptr"))]
mod index {
    use std::cell::Cell;

    pub struct Index(Cell<usize>);

    impl Index {
        pub fn new(value: usize) -> Self {
            Self(Cell::new(value))
        }

        pub fn load(&self) -> usize {
            self.0.get()
        }

        pub fn store(&self, value: usize) {
            self.0.set(value);
        }
    }
}

use index::Index;

/// Interleaved stereo frames. `capacity` is rounded up to a power of two.
pub struct StereoRing {
    slots: Box<[UnsafeCell<[f32; 2]>]>,
    cap: usize,
    mask: usize,
    read: Index,
    write: Index,
}

// One producer and one consumer. Index stores are release and loads are
// acquire, so a slot is not read while it is being written.
#[cfg(target_has_atomic = "ptr")]
unsafe impl Sync for StereoRing {}

impl StereoRing {
    pub fn with_capacity(frames: usize) -> Self {
        let cap = frames.max(1).next_power_of_two();
        let slots =
            (0..cap).map(|_| UnsafeCell::new([0.0; 2])).collect::<Vec<_>>().into_boxed_slice();
        Self { slots, cap, mask: cap - 1, read: Index::new(0), write: Index::new(0) }
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Frames waiting to be read. A damaged cursor (gap wider than the buffer) reports empty.
    pub fn filled(&self) -> usize {
        let gap = self.write.load().wrapping_sub(self.read.load());
        if gap > self.cap {
            0
        } else {
            gap
        }
    }

    /// Copy `frames` into free slots. Returns how many were stored.
    pub fn push(&self, frames: &[[f32; 2]]) -> usize {
        let read = self.read.load();
        let write = self.write.load();
        let gap = write.wrapping_sub(read);
        if gap > self.cap {
            return 0;
        }
        let free = self.cap - gap;
        let count = frames.len().min(free);
        for (offset, frame) in frames.iter().take(count).enumerate() {
            self.write_slot(write.wrapping_add(offset), *frame);
        }
        self.write.store(write.wrapping_add(count));
        count
    }

    /// Copy queued frames into `out`. Returns how many were written. The rest of `out` is left as it was.
    pub fn pop(&self, out: &mut [[f32; 2]]) -> usize {
        let read = self.read.load();
        let write = self.write.load();
        let gap = write.wrapping_sub(read);
        if gap > self.cap {
            return 0;
        }
        let count = out.len().min(gap);
        for (offset, slot) in out.iter_mut().take(count).enumerate() {
            *slot = self.read_slot(read.wrapping_add(offset));
        }
        self.read.store(read.wrapping_add(count));
        count
    }

    /// Pop into separate left and right buffers. Both slices are filled from the start.
    /// Returns the number of frames written.
    pub fn pop_planar(&self, left: &mut [f32], right: &mut [f32]) -> usize {
        let count = left.len().min(right.len());
        let mut frames = vec![[0.0; 2]; count];
        let wrote = self.pop(&mut frames);
        for (index, frame) in frames.iter().take(wrote).enumerate() {
            left[index] = frame[0];
            right[index] = frame[1];
        }
        wrote
    }

    fn write_slot(&self, index: usize, frame: [f32; 2]) {
        let ptr = self.slots[index & self.mask].get();
        // The producer only writes a slot after observing that the consumer has released it.
        unsafe { *ptr = frame }
    }

    fn read_slot(&self, index: usize) -> [f32; 2] {
        let ptr = self.slots[index & self.mask].get();
        // The consumer only reads a slot after observing the producer's release store.
        unsafe { *ptr }
    }

    #[cfg(test)]
    fn force_indices(&self, read: usize, write: usize) {
        self.read.store(read);
        self.write.store(write);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn push_and_pop_round_trip_and_wrap() {
        let ring = StereoRing::with_capacity(3);
        assert_eq!(ring.capacity(), 4);
        let first = [[1.0, -1.0], [2.0, -2.0], [3.0, -3.0]];
        assert_eq!(ring.push(&first), 3);
        assert_eq!(ring.filled(), 3);
        assert_eq!(ring.push(&[[9.0, 9.0], [8.0, 8.0]]), 1);
        let mut out = [[0.0; 2]; 2];
        assert_eq!(ring.pop(&mut out), 2);
        assert_eq!(out, [[1.0, -1.0], [2.0, -2.0]]);
        assert_eq!(ring.push(&[[4.0, -4.0], [5.0, -5.0]]), 2);
        let mut rest = [[0.0; 2]; 4];
        assert_eq!(ring.pop(&mut rest), 4);
        assert_eq!(rest, [[3.0, -3.0], [9.0, 9.0], [4.0, -4.0], [5.0, -5.0]]);
        assert_eq!(ring.filled(), 0);
        assert_eq!(ring.pop(&mut rest), 0);
    }

    #[test]
    fn a_damaged_cursor_does_not_read_past_the_buffer() {
        let ring = StereoRing::with_capacity(4);
        ring.push(&[[1.0, 1.0]]);
        ring.force_indices(0, ring.capacity() + 5);
        assert_eq!(ring.filled(), 0);
        let mut out = [[7.0; 2]; 4];
        assert_eq!(ring.pop(&mut out), 0);
        assert_eq!(out[0], [7.0, 7.0]);
        assert_eq!(ring.push(&[[2.0, 2.0]]), 0);
    }

    #[test]
    fn planar_pop_splits_channels() {
        let ring = StereoRing::with_capacity(2);
        ring.push(&[[0.25, -0.5]]);
        let mut left = [0.0; 2];
        let mut right = [0.0; 2];
        assert_eq!(ring.pop_planar(&mut left, &mut right), 1);
        assert_eq!(left[0], 0.25);
        assert_eq!(right[0], -0.5);
        assert_eq!(left[1], 0.0);
    }

    #[cfg(target_has_atomic = "ptr")]
    #[test]
    fn producer_and_consumer_keep_every_frame() {
        let ring = Arc::new(StereoRing::with_capacity(64));
        let total = 4_000usize;
        let producer = {
            let ring = Arc::clone(&ring);
            std::thread::spawn(move || {
                let mut sent = 0usize;
                while sent < total {
                    let count = (total - sent).min(17);
                    let frames: Vec<[f32; 2]> = (0..count)
                        .map(|offset| {
                            let value = (sent + offset) as f32;
                            [value, -value]
                        })
                        .collect();
                    let wrote = ring.push(&frames);
                    sent += wrote;
                    if wrote == 0 {
                        std::thread::yield_now();
                    }
                }
            })
        };
        let mut got = Vec::with_capacity(total);
        while got.len() < total {
            let mut buf = vec![[0.0; 2]; 23];
            let count = ring.pop(&mut buf);
            got.extend_from_slice(&buf[..count]);
            if count == 0 {
                std::thread::yield_now();
            }
        }
        producer.join().expect("producer");
        assert_eq!(got.len(), total);
        for (index, frame) in got.iter().enumerate() {
            assert_eq!(frame[0], index as f32);
            assert_eq!(frame[1], -(index as f32));
        }
    }
}
