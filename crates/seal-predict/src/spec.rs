//! Speculation on spare capacity (Part 4d.8 of the design).
//!
//! Speculative tasks run on their own threads at the lowest priority the OS
//! offers (`SCHED_IDLE` on Linux, idle priority on Windows), so they get CPU
//! time only when real work does not want it. Nothing ever waits for them:
//! [`Speculator::take`] never blocks, a result is used only when the inputs
//! it was computed from match the real ones, and [`Speculator::cancel_all`]
//! drops everything in flight the moment real work moves on. Each source
//! keeps hit and waste counts and is paused when its accuracy falls below
//! the break-even accuracy from the cost model.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Checked by a running task; once it reads true, the result will be dropped.
#[derive(Clone)]
pub struct CancelToken {
    epoch: u64,
    current: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn cancelled(&self) -> bool {
        self.stop.load(Ordering::Relaxed) || self.current.load(Ordering::Relaxed) != self.epoch
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SourceStats {
    pub submitted: u64,
    pub hits: u64,
    /// Results that existed but did not match the real inputs.
    pub mispredicted: u64,
    /// Asked for but not ready in time, or cancelled.
    pub late: u64,
    pub paused: bool,
}

impl SourceStats {
    pub fn accuracy(&self) -> f64 {
        let judged = self.hits + self.mispredicted;
        if judged == 0 {
            1.0
        } else {
            self.hits as f64 / judged as f64
        }
    }
}

type Job = Box<dyn FnOnce() + Send>;

struct Shared<K, V> {
    results: Mutex<HashMap<K, (u64, u64, V)>>,
    epoch: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}

pub struct Speculator<K, V> {
    shared: Arc<Shared<K, V>>,
    queue: Option<Sender<(u64, Job)>>,
    threads: Vec<JoinHandle<()>>,
    stats: Vec<SourceStats>,
    /// Pause a source once it has this many judged guesses below break-even.
    min_samples: u64,
    breakeven: f64,
    budget_ms: f64,
    low_priority: Arc<AtomicBool>,
}

/// Lower the calling thread to the OS's lowest scheduling class.
fn lower_priority() -> bool {
    #[cfg(target_os = "linux")]
    unsafe {
        let param = libc::sched_param { sched_priority: 0 };
        libc::sched_setscheduler(0, libc::SCHED_IDLE, &param) == 0
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_IDLE,
        };
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_IDLE) != 0
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        false
    }
}

impl<K, V> Speculator<K, V>
where
    K: Eq + Hash + Send + 'static,
    V: Send + 'static,
{
    /// `threads` speculative workers, `sources` independent speculation
    /// sources, paused when accuracy drops below `breakeven`.
    pub fn new(threads: usize, sources: usize, breakeven: f64) -> Self {
        let epoch = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            results: Mutex::new(HashMap::new()),
            epoch: epoch.clone(),
            stop: stop.clone(),
        });
        let (tx, rx) = channel::<(u64, Job)>();
        let rx = Arc::new(Mutex::new(rx));
        let low_priority = Arc::new(AtomicBool::new(threads > 0));
        let threads = (0..threads)
            .map(|i| {
                let rx: Arc<Mutex<Receiver<(u64, Job)>>> = rx.clone();
                let (epoch, stop, low) = (epoch.clone(), stop.clone(), low_priority.clone());
                std::thread::Builder::new()
                    .name(format!("seal-spec-{i}"))
                    .spawn(move || {
                        if !lower_priority() {
                            low.store(false, Ordering::Relaxed);
                        }
                        loop {
                            let next = rx.lock().expect("queue").recv();
                            let Ok((job_epoch, job)) = next else { break };
                            if stop.load(Ordering::Relaxed) {
                                break;
                            }
                            // Tasks from before a cancel are skipped unrun.
                            if job_epoch == epoch.load(Ordering::Relaxed) {
                                job();
                            }
                        }
                    })
                    .expect("spawn speculative thread")
            })
            .collect();
        Self {
            shared,
            queue: Some(tx),
            threads,
            stats: vec![SourceStats::default(); sources],
            min_samples: 16,
            breakeven: breakeven.clamp(0.0, 1.0),
            budget_ms: f64::INFINITY,
            low_priority,
        }
    }

    /// Whether every speculative thread got the lowest OS priority.
    pub fn low_priority(&self) -> bool {
        self.low_priority.load(Ordering::Relaxed)
    }

    /// Set this frame's spare budget (see [`crate::cost::spare_budget_ms`]).
    pub fn begin_frame(&mut self, budget_ms: f64) {
        self.budget_ms = budget_ms.max(0.0);
    }

    pub fn stats(&self, source: usize) -> SourceStats {
        self.stats[source]
    }

    /// Queue `work` for `key`, computed from inputs hashing to `inputs`.
    /// Returns false when the source is paused or the budget is spent; the
    /// caller then just computes normally later.
    pub fn submit(
        &mut self,
        source: usize,
        key: K,
        inputs: u64,
        cost_ms: f64,
        work: impl FnOnce(&CancelToken) -> Option<V> + Send + 'static,
    ) -> bool {
        if self.stats[source].paused || cost_ms > self.budget_ms {
            return false;
        }
        let Some(queue) = &self.queue else { return false };
        self.budget_ms -= cost_ms;
        self.stats[source].submitted += 1;
        let epoch = self.shared.epoch.load(Ordering::Relaxed);
        let token = CancelToken {
            epoch,
            current: self.shared.epoch.clone(),
            stop: self.shared.stop.clone(),
        };
        let shared = self.shared.clone();
        let job: Job = Box::new(move || {
            if let Some(value) = work(&token) {
                if !token.cancelled() {
                    shared.results.lock().expect("results").insert(key, (epoch, inputs, value));
                }
            }
        });
        queue.send((epoch, job)).is_ok()
    }

    /// Take the speculative result for `key` if it is ready and was computed
    /// from `actual_inputs`. Never blocks.
    pub fn take(&mut self, source: usize, key: &K, actual_inputs: u64) -> Option<V> {
        let epoch = self.shared.epoch.load(Ordering::Relaxed);
        let found = self.shared.results.lock().expect("results").remove(key);
        let stats = &mut self.stats[source];
        let out = match found {
            Some((e, inputs, value)) if e == epoch && inputs == actual_inputs => {
                stats.hits += 1;
                Some(value)
            }
            Some(_) => {
                stats.mispredicted += 1;
                None
            }
            None => {
                stats.late += 1;
                None
            }
        };
        let judged = stats.hits + stats.mispredicted;
        if judged >= self.min_samples && stats.accuracy() < self.breakeven {
            stats.paused = true;
        }
        out
    }

    /// Unpause a source, for example after its inputs changed character.
    pub fn resume(&mut self, source: usize) {
        self.stats[source] = SourceStats::default();
    }

    /// Drop every queued, running and finished speculative result.
    pub fn cancel_all(&mut self) {
        self.shared.epoch.fetch_add(1, Ordering::Relaxed);
        self.shared.results.lock().expect("results").clear();
    }
}

impl<K, V> Drop for Speculator<K, V> {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.queue.take();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn heavy(x: u64) -> u64 {
        (0..20_000u64).fold(x, |a, i| a.wrapping_mul(6364136223846793005).wrapping_add(i))
    }

    fn wait_ready(spec: &Speculator<u64, u64>, keys: usize) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while spec.shared.results.lock().unwrap().len() < keys && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn results_are_identical_with_speculation_on_and_off() {
        let mut spec = Speculator::<u64, u64>::new(2, 1, 0.1);
        let predicted: Vec<u64> = (0..64).map(|i| if i % 4 == 0 { i + 1000 } else { i }).collect();
        for (key, &input) in predicted.iter().enumerate() {
            assert!(spec.submit(0, key as u64, input, 0.0, move |_| Some(heavy(input))));
        }
        wait_ready(&spec, 64);
        let actual: Vec<u64> = (0..64).collect();
        let mut reused = 0;
        let mut with = Vec::new();
        for (key, &input) in actual.iter().enumerate() {
            let value = match spec.take(0, &(key as u64), input) {
                Some(v) => {
                    reused += 1;
                    v
                }
                None => heavy(input),
            };
            with.push(value);
        }
        let without: Vec<u64> = actual.iter().map(|&i| heavy(i)).collect();
        assert_eq!(with, without);
        assert_eq!(reused, 48);
        let stats = spec.stats(0);
        assert_eq!((stats.hits, stats.mispredicted), (48, 16));
    }

    #[test]
    fn cancel_drops_everything_and_take_never_blocks() {
        let mut spec = Speculator::<u64, u64>::new(1, 1, 0.0);
        for key in 0..200u64 {
            spec.submit(0, key, key, 0.0, move |token| {
                if token.cancelled() {
                    return None;
                }
                Some(heavy(key))
            });
        }
        spec.cancel_all();
        let start = Instant::now();
        for key in 0..200u64 {
            assert_eq!(spec.take(0, &key, key), None);
        }
        assert!(start.elapsed() < Duration::from_millis(500));
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            spec.shared.results.lock().unwrap().is_empty(),
            "stale results never land after a cancel"
        );
    }

    #[test]
    fn bad_sources_pause_and_budget_limits_submissions() {
        let mut spec = Speculator::<u64, u64>::new(1, 2, 0.5);
        for key in 0..20u64 {
            spec.submit(0, key, 1, 0.0, |_| Some(0));
        }
        wait_ready(&spec, 20);
        for key in 0..20u64 {
            spec.take(0, &key, 2);
        }
        assert!(spec.stats(0).paused);
        assert!(!spec.submit(0, 99, 1, 0.0, |_| Some(0)));
        spec.begin_frame(1.0);
        assert!(spec.submit(1, 1, 1, 0.6, |_| Some(0)));
        assert!(!spec.submit(1, 2, 1, 0.6, |_| Some(0)), "over budget");
        spec.resume(0);
        assert!(spec.submit(0, 3, 1, 0.3, |_| Some(0)));
    }
}
