//! Seal's job system.
//!
//! One worker per hardware thread. Each worker owns a work-stealing deque;
//! jobs spawned from outside the pool go to one shared queue per priority.
//! A thread that waits for jobs (a scope ending, `wait`) runs other jobs
//! while it waits, so waiting never blocks a worker.
//!
//! Results that feed the simulation are returned in a fixed order, keyed by
//! chunk index and never by completion order, so a run with 2 threads and a
//! run with 16 produce identical output.

use std::any::Any;
use std::cell::Cell;
use std::marker::PhantomData;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_deque::{Injector, Steal, Stealer, Worker};

/// Which queue a job goes to. Workers always drain higher priorities first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Priority {
    /// Work the current frame is waiting on.
    FrameCritical = 0,
    /// Ordinary work.
    Normal = 1,
    /// Work that only runs when nothing else is queued (streaming, speculation).
    Background = 2,
}

const PRIORITIES: usize = 3;

type Job = Box<dyn FnOnce() + Send + 'static>;

struct Shared {
    queues: [Injector<Job>; PRIORITIES],
    stealers: Vec<Stealer<Job>>,
    shutdown: AtomicBool,
    sleepers: Mutex<usize>,
    wake: Condvar,
    executed: AtomicUsize,
    stolen: AtomicUsize,
}

impl Shared {
    fn notify(&self) {
        let sleepers = self.sleepers.lock().unwrap_or_else(|e| e.into_inner());
        if *sleepers > 0 {
            self.wake.notify_one();
        }
    }

    /// Find one job: own deque, then shared queues by priority, then steal.
    fn find(&self, local: Option<&Worker<Job>>, me: Option<usize>) -> Option<Job> {
        if let Some(job) = local.and_then(Worker::pop) {
            return Some(job);
        }
        for priority in 0..PRIORITIES {
            // Background jobs are only taken when no worker has normal work
            // left to steal; check stealers first for them.
            if priority == Priority::Background as usize {
                if let Some(job) = self.steal_any(me) {
                    return Some(job);
                }
            }
            loop {
                let steal = match local {
                    Some(local) => self.queues[priority].steal_batch_and_pop(local),
                    None => self.queues[priority].steal(),
                };
                match steal {
                    Steal::Success(job) => return Some(job),
                    Steal::Retry => continue,
                    Steal::Empty => break,
                }
            }
        }
        self.steal_any(me)
    }

    fn steal_any(&self, me: Option<usize>) -> Option<Job> {
        let count = self.stealers.len();
        if count == 0 {
            return None;
        }
        let start = me.map_or(0, |i| i + 1);
        for offset in 0..count {
            let index = (start + offset) % count;
            if Some(index) == me {
                continue;
            }
            loop {
                match self.stealers[index].steal() {
                    Steal::Success(job) => {
                        self.stolen.fetch_add(1, Ordering::Relaxed);
                        return Some(job);
                    }
                    Steal::Retry => continue,
                    Steal::Empty => break,
                }
            }
        }
        None
    }

    fn run(&self, job: Job) {
        let _zone = seal_trace::zone!(seal_trace::Channel::JOBS, "job");
        job();
        self.executed.fetch_add(1, Ordering::Relaxed);
    }
}

thread_local! {
    /// Set on worker threads: (pool identity, worker index, local deque).
    static CURRENT: Cell<Option<(usize, usize, *const Worker<Job>)>> = const { Cell::new(None) };
}

/// A pool of worker threads.
pub struct JobPool {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

/// Counters for tests and the profiler.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PoolStats {
    pub workers: usize,
    pub executed: usize,
    pub stolen: usize,
}

impl JobPool {
    /// A pool with one worker per hardware thread.
    pub fn with_hardware_threads() -> Self {
        let n = std::thread::available_parallelism().map_or(1, |n| n.get());
        Self::new(n)
    }

    /// A pool with `workers` threads. Zero workers is allowed: then every job
    /// runs on the thread that waits for it, which is useful for tests.
    pub fn new(workers: usize) -> Self {
        let locals: Vec<Worker<Job>> = (0..workers).map(|_| Worker::new_lifo()).collect();
        let stealers = locals.iter().map(Worker::stealer).collect();
        let shared = Arc::new(Shared {
            queues: [Injector::new(), Injector::new(), Injector::new()],
            stealers,
            shutdown: AtomicBool::new(false),
            sleepers: Mutex::new(0),
            wake: Condvar::new(),
            executed: AtomicUsize::new(0),
            stolen: AtomicUsize::new(0),
        });
        let threads = locals
            .into_iter()
            .enumerate()
            .map(|(index, local)| {
                let shared = Arc::clone(&shared);
                std::thread::Builder::new()
                    .name(format!("seal-worker-{index}"))
                    .spawn(move || worker_loop(shared, index, local))
                    .expect("spawn worker thread")
            })
            .collect();
        Self { shared, threads }
    }

    pub fn workers(&self) -> usize {
        self.threads.len()
    }

    pub fn stats(&self) -> PoolStats {
        PoolStats {
            workers: self.threads.len(),
            executed: self.shared.executed.load(Ordering::Relaxed),
            stolen: self.shared.stolen.load(Ordering::Relaxed),
        }
    }

    fn identity(&self) -> usize {
        Arc::as_ptr(&self.shared) as usize
    }

    fn push(&self, priority: Priority, job: Job) {
        let me = self.identity();
        let pushed_local = CURRENT.with(|current| match current.get() {
            Some((pool, _, local)) if pool == me && priority == Priority::Normal => {
                // SAFETY: the pointer is the deque owned by this very worker
                // thread, set in `worker_loop` and cleared before it drops.
                unsafe { (*local).push(job) };
                None
            }
            _ => Some(job),
        });
        if let Some(job) = pushed_local {
            self.shared.queues[priority as usize].push(job);
        }
        self.shared.notify();
    }

    /// Run a detached job. It must own everything it touches.
    pub fn spawn(&self, priority: Priority, job: impl FnOnce() + Send + 'static) {
        self.push(priority, Box::new(job));
    }

    /// Run one queued job on this thread, if there is one.
    fn help(&self) -> bool {
        let me = self.identity();
        let (local, index) = CURRENT.with(|current| match current.get() {
            Some((pool, index, local)) if pool == me => (Some(local), Some(index)),
            _ => (None, None),
        });
        // SAFETY: as in `push`, the deque belongs to the current thread.
        let local = local.map(|ptr| unsafe { &*ptr });
        match self.shared.find(local, index) {
            Some(job) => {
                self.shared.run(job);
                true
            }
            None => false,
        }
    }

    /// Run jobs that borrow from the caller's stack. Returns after every job
    /// spawned in the scope has finished; a panic in any job is re-raised here.
    pub fn scope<'env, R>(&self, body: impl FnOnce(&Scope<'_, 'env>) -> R) -> R {
        let scope = Scope { pool: self, state: Arc::new(ScopeState::default()), _env: PhantomData };
        let result = {
            // Wait even if `body` panics, so no job outlives its borrows.
            let _guard = WaitGuard(&scope);
            panic::catch_unwind(AssertUnwindSafe(|| body(&scope)))
        };
        if let Some(payload) = scope.state.take_panic() {
            panic::resume_unwind(payload);
        }
        match result {
            Ok(value) => value,
            Err(payload) => panic::resume_unwind(payload),
        }
    }

    /// Split `0..len` into chunks of `chunk` items, run `job(chunk_index, range)`
    /// for each in parallel, and return the results in chunk order.
    pub fn map_chunks<T, F>(&self, len: usize, chunk: usize, job: F) -> Vec<T>
    where
        T: Send,
        F: Fn(usize, std::ops::Range<usize>) -> T + Sync,
    {
        let chunk = chunk.max(1);
        let count = len.div_ceil(chunk);
        let mut results: Vec<Option<T>> = (0..count).map(|_| None).collect();
        let job = &job;
        self.scope(|scope| {
            for (index, slot) in results.iter_mut().enumerate() {
                let start = index * chunk;
                let range = start..(start + chunk).min(len);
                scope.spawn(Priority::Normal, move || *slot = Some(job(index, range)));
            }
        });
        results.into_iter().map(|r| r.expect("every chunk ran")).collect()
    }

    /// Run `job(chunk_index, chunk)` over mutable chunks of `items` in parallel.
    pub fn for_each_chunk_mut<T, F>(&self, items: &mut [T], chunk: usize, job: F)
    where
        T: Send,
        F: Fn(usize, &mut [T]) + Sync,
    {
        let job = &job;
        self.scope(|scope| {
            for (index, part) in items.chunks_mut(chunk.max(1)).enumerate() {
                scope.spawn(Priority::Normal, move || job(index, part));
            }
        });
    }

    /// Map chunks in parallel, then fold the chunk results in chunk order.
    /// The fold order never depends on thread count or timing.
    pub fn reduce_ordered<T, A, F, G>(
        &self,
        len: usize,
        chunk: usize,
        map: F,
        init: A,
        fold: G,
    ) -> A
    where
        T: Send,
        F: Fn(usize, std::ops::Range<usize>) -> T + Sync,
        G: FnMut(A, T) -> A,
    {
        self.map_chunks(len, chunk, map).into_iter().fold(init, fold)
    }
}

impl Drop for JobPool {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::SeqCst);
        {
            let _sleepers = self.shared.sleepers.lock().unwrap_or_else(|e| e.into_inner());
            self.shared.wake.notify_all();
        }
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

fn worker_loop(shared: Arc<Shared>, index: usize, local: Worker<Job>) {
    let identity = Arc::as_ptr(&shared) as usize;
    CURRENT.with(|current| current.set(Some((identity, index, &local as *const _))));
    let mut idle_spins = 0u32;
    while !shared.shutdown.load(Ordering::Acquire) {
        if let Some(job) = shared.find(Some(&local), Some(index)) {
            idle_spins = 0;
            shared.run(job);
            continue;
        }
        idle_spins += 1;
        if idle_spins < 64 {
            std::hint::spin_loop();
            continue;
        }
        if idle_spins < 128 {
            std::thread::yield_now();
            continue;
        }
        let mut sleepers = shared.sleepers.lock().unwrap_or_else(|e| e.into_inner());
        *sleepers += 1;
        // A short timeout covers a push that raced with going to sleep.
        let (guard, _) = shared
            .wake
            .wait_timeout(sleepers, Duration::from_millis(2))
            .unwrap_or_else(|e| e.into_inner());
        sleepers = guard;
        *sleepers -= 1;
    }
    CURRENT.with(|current| current.set(None));
}

#[derive(Default)]
struct ScopeState {
    pending: AtomicUsize,
    panic: Mutex<Option<Box<dyn Any + Send>>>,
}

impl ScopeState {
    fn take_panic(&self) -> Option<Box<dyn Any + Send>> {
        self.panic.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

/// Spawns jobs that may borrow anything that outlives the scope.
pub struct Scope<'pool, 'env> {
    pool: &'pool JobPool,
    state: Arc<ScopeState>,
    _env: PhantomData<&'env mut &'env ()>,
}

impl<'env> Scope<'_, 'env> {
    pub fn spawn(&self, priority: Priority, job: impl FnOnce() + Send + 'env) {
        self.state.pending.fetch_add(1, Ordering::AcqRel);
        let state = Arc::clone(&self.state);
        let wrapped = move || {
            if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(job)) {
                let mut slot = state.panic.lock().unwrap_or_else(|e| e.into_inner());
                slot.get_or_insert(payload);
            }
            state.pending.fetch_sub(1, Ordering::AcqRel);
        };
        let boxed: Box<dyn FnOnce() + Send + 'env> = Box::new(wrapped);
        // SAFETY: `JobPool::scope` does not return (or unwind) until
        // `pending` is back to zero, so the job never outlives `'env`.
        let boxed: Job = unsafe { std::mem::transmute(boxed) };
        self.pool.push(priority, boxed);
    }

    /// How many jobs of this scope have not finished yet.
    pub fn pending(&self) -> usize {
        self.state.pending.load(Ordering::Acquire)
    }
}

struct WaitGuard<'a, 'pool, 'env>(&'a Scope<'pool, 'env>);

impl Drop for WaitGuard<'_, '_, '_> {
    fn drop(&mut self) {
        let scope = self.0;
        let mut idle = 0u32;
        while scope.state.pending.load(Ordering::Acquire) > 0 {
            if scope.pool.help() {
                idle = 0;
            } else {
                idle += 1;
                if idle < 64 {
                    std::hint::spin_loop();
                } else {
                    std::thread::yield_now();
                }
            }
        }
    }
}

/// A counter jobs can count down; `wait` runs other jobs until it reaches zero.
#[derive(Clone, Default)]
pub struct Counter(Arc<AtomicUsize>);

impl Counter {
    pub fn new(count: usize) -> Self {
        Self(Arc::new(AtomicUsize::new(count)))
    }

    pub fn decrement(&self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }

    pub fn value(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }

    pub fn wait(&self, pool: &JobPool) {
        while self.value() > 0 {
            if !pool.help() {
                std::thread::yield_now();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn scoped_jobs_can_borrow_and_write_the_callers_data() {
        let pool = JobPool::new(4);
        let mut data = vec![0u64; 1000];
        pool.for_each_chunk_mut(&mut data, 64, |chunk, part| {
            for (i, value) in part.iter_mut().enumerate() {
                *value = (chunk * 64 + i) as u64 * 2;
            }
        });
        assert!(data.iter().enumerate().all(|(i, v)| *v == i as u64 * 2));
    }

    #[test]
    fn ordered_results_do_not_depend_on_thread_count() {
        // Floating-point sums change with order; fixed-order folding keeps them equal.
        let values: Vec<f32> =
            (0..100_000).map(|i| ((i * 7919) % 1000) as f32 * 0.001 + 1e-7).collect();
        let sum_with = |threads: usize| {
            let pool = JobPool::new(threads);
            pool.reduce_ordered(
                values.len(),
                777,
                |_, range| values[range].iter().sum::<f32>(),
                0.0f32,
                |acc, part| acc + part,
            )
        };
        let one = sum_with(0).to_bits();
        for threads in [1, 2, 3, 8] {
            assert_eq!(sum_with(threads).to_bits(), one, "threads = {threads}");
        }
    }

    #[test]
    fn a_pool_with_no_workers_runs_jobs_on_the_waiting_thread() {
        let pool = JobPool::new(0);
        let results = pool.map_chunks(10, 3, |index, range| (index, range.len()));
        assert_eq!(results, vec![(0, 3), (1, 3), (2, 3), (3, 1)]);
    }

    #[test]
    fn nested_scopes_from_inside_jobs_finish() {
        let pool = JobPool::new(3);
        let total = AtomicU64::new(0);
        pool.scope(|outer| {
            for _ in 0..8 {
                outer.spawn(Priority::Normal, || {
                    pool.scope(|inner| {
                        for _ in 0..8 {
                            inner.spawn(Priority::Normal, || {
                                total.fetch_add(1, Ordering::Relaxed);
                            });
                        }
                    });
                });
            }
        });
        assert_eq!(total.load(Ordering::Relaxed), 64);
    }

    #[test]
    fn a_panicking_job_is_reported_after_the_others_finish() {
        let pool = JobPool::new(2);
        let done = AtomicU64::new(0);
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
            pool.scope(|scope| {
                scope.spawn(Priority::Normal, || panic!("job failed"));
                for _ in 0..16 {
                    scope.spawn(Priority::Normal, || {
                        done.fetch_add(1, Ordering::Relaxed);
                    });
                }
            })
        }));
        assert!(outcome.is_err());
        assert_eq!(done.load(Ordering::Relaxed), 16);
    }

    #[test]
    fn counters_and_detached_jobs_work_together() {
        let pool = JobPool::new(2);
        let counter = Counter::new(5);
        let hits = Arc::new(AtomicU64::new(0));
        for priority in [
            Priority::FrameCritical,
            Priority::Normal,
            Priority::Background,
            Priority::Normal,
            Priority::Normal,
        ] {
            let counter = counter.clone();
            let hits = Arc::clone(&hits);
            pool.spawn(priority, move || {
                hits.fetch_add(1, Ordering::Relaxed);
                counter.decrement();
            });
        }
        counter.wait(&pool);
        assert_eq!(hits.load(Ordering::Relaxed), 5);
        assert!(pool.stats().executed >= 5);
    }
}
