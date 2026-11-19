//! A timing wheel: items fire at a tick, in `(tick, key)` order.
//!
//! Near entries sit in one of 256 slots; far ones wait in an ordered map and
//! move into the slots as the wheel turns. Firing order never depends on
//! insertion order, so a wheel can stand in for per-tick checks exactly.

use std::collections::BTreeMap;

const SLOTS: u64 = 256;

pub struct TimingWheel<T> {
    now: u64,
    slots: Vec<Vec<(u64, u64, T)>>,
    far: BTreeMap<(u64, u64), T>,
    len: usize,
}

impl<T> TimingWheel<T> {
    /// A wheel whose current tick is `now`. Items can be scheduled after it.
    pub fn new(now: u64) -> Self {
        Self { now, slots: (0..SLOTS).map(|_| Vec::new()).collect(), far: BTreeMap::new(), len: 0 }
    }

    pub fn now(&self) -> u64 {
        self.now
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Schedule `item` to fire at `tick`. Ticks at or before now fire on the next advance.
    pub fn schedule(&mut self, tick: u64, key: u64, item: T) {
        let tick = tick.max(self.now + 1);
        self.len += 1;
        if tick < self.now + SLOTS {
            self.slots[(tick % SLOTS) as usize].push((tick, key, item));
        } else {
            self.far.insert((tick, key), item);
        }
    }

    /// Turn the wheel to `target`, handing every item that fires to `fire` in
    /// `(tick, key)` order. `fire` may schedule more items through the wheel
    /// it is given; those fire in this same call when they fall due.
    pub fn advance(&mut self, target: u64, mut fire: impl FnMut(&mut Self, u64, u64, T)) {
        while self.now < target {
            if self.len == 0 {
                self.now = target;
                break;
            }
            // Jump over empty stretches straight to the next far entry.
            if self.slots.iter().all(Vec::is_empty) {
                if let Some((&(tick, _), _)) = self.far.iter().next() {
                    if tick > target {
                        self.now = target;
                        break;
                    }
                    self.now = tick - 1;
                }
            }
            self.now += 1;
            let tick = self.now;
            // Pull far entries that now fit in the slot window.
            let edge = tick + SLOTS;
            while let Some((&(t, k), _)) = self.far.iter().next() {
                if t >= edge {
                    break;
                }
                let item = self.far.remove(&(t, k)).expect("present");
                self.slots[(t % SLOTS) as usize].push((t, k, item));
            }
            let slot = (tick % SLOTS) as usize;
            if self.slots[slot].is_empty() {
                continue;
            }
            let mut due = std::mem::take(&mut self.slots[slot]);
            due.sort_by_key(|entry| (entry.0, entry.1));
            self.len -= due.len();
            for (t, key, item) in due {
                debug_assert_eq!(t, tick);
                fire(self, t, key, item);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_in_tick_then_key_order_whatever_the_insert_order() {
        let mut wheel = TimingWheel::new(0);
        for (tick, key) in [(5, 2), (3, 9), (5, 1), (1000, 0), (300, 4), (3, 1)] {
            wheel.schedule(tick, key, ());
        }
        let mut seen = Vec::new();
        wheel.advance(2000, |_, tick, key, ()| seen.push((tick, key)));
        assert_eq!(seen, vec![(3, 1), (3, 9), (5, 1), (5, 2), (300, 4), (1000, 0)]);
        assert!(wheel.is_empty());
    }

    #[test]
    fn rescheduling_from_inside_fire_makes_a_periodic_timer() {
        let mut wheel = TimingWheel::new(0);
        wheel.schedule(7, 0, ());
        let mut fires = Vec::new();
        wheel.advance(700, |wheel, tick, key, ()| {
            fires.push(tick);
            wheel.schedule(tick + 300, key, ());
        });
        assert_eq!(fires, vec![7, 307, 607]);
        assert_eq!(wheel.len(), 1);
        assert_eq!(wheel.now(), 700);
    }

    #[test]
    fn stepwise_and_single_jump_give_the_same_fires() {
        let make = || {
            let mut wheel = TimingWheel::new(0);
            for i in 0..500u64 {
                wheel.schedule(1 + (i * 7919) % 5000, i, ());
            }
            wheel
        };
        let mut a = make();
        let mut fa = Vec::new();
        a.advance(6000, |_, t, k, ()| fa.push((t, k)));
        let mut b = make();
        let mut fb = Vec::new();
        for step in (0..=6000).step_by(37) {
            b.advance(step, |_, t, k, ()| fb.push((t, k)));
        }
        b.advance(6000, |_, t, k, ()| fb.push((t, k)));
        assert_eq!(fa, fb);
        assert_eq!(fa.len(), 500);
    }
}
