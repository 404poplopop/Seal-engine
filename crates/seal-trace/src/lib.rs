//! Trace channels for the whole engine.
//!
//! Every subsystem marks its work with [`zone!`] (a timed span), [`counter`]
//! (a value over time) and [`frame_mark`]. Channels are switched on and off
//! at runtime; a zone on a disabled channel costs one atomic load.
//!
//! Recorded events sit in per-thread buffers (bounded; the oldest are dropped
//! and counted) until [`drain`] collects them. [`Trace::to_chrome_json`]
//! writes the Chrome trace format, which Perfetto and chrome://tracing open.
//! With the `tracy` feature the same events also stream to Tracy.
#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

/// A set of trace channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Channel(pub u32);

impl Channel {
    pub const CPU: Channel = Channel(1 << 0);
    pub const GPU: Channel = Channel(1 << 1);
    pub const MEMORY: Channel = Channel(1 << 2);
    pub const IO: Channel = Channel(1 << 3);
    pub const NETWORK: Channel = Channel(1 << 4);
    pub const ASSETS: Channel = Channel(1 << 5);
    pub const AUDIO: Channel = Channel(1 << 6);
    pub const JOBS: Channel = Channel(1 << 7);
    pub const LOCKS: Channel = Channel(1 << 8);
    pub const ALL: Channel = Channel((1 << 9) - 1);
    pub const NONE: Channel = Channel(0);

    pub const NAMES: [(&'static str, Channel); 9] = [
        ("cpu", Channel::CPU),
        ("gpu", Channel::GPU),
        ("memory", Channel::MEMORY),
        ("io", Channel::IO),
        ("network", Channel::NETWORK),
        ("assets", Channel::ASSETS),
        ("audio", Channel::AUDIO),
        ("jobs", Channel::JOBS),
        ("locks", Channel::LOCKS),
    ];

    /// Parse "cpu,jobs" or "all" or "none".
    pub fn parse(text: &str) -> Result<Channel, String> {
        let mut mask = 0;
        for word in text.split(',').map(str::trim).filter(|w| !w.is_empty()) {
            mask |= match word {
                "all" => Channel::ALL.0,
                "none" => 0,
                _ => {
                    Channel::NAMES
                        .iter()
                        .find(|(n, _)| *n == word)
                        .ok_or(format!("unknown trace channel {word}"))?
                        .1
                         .0
                }
            };
        }
        Ok(Channel(mask))
    }

    /// The first channel name in this set.
    pub fn name(self) -> &'static str {
        Channel::NAMES.iter().find(|(_, c)| c.0 & self.0 != 0).map_or("none", |(n, _)| n)
    }
}

impl std::ops::BitOr for Channel {
    type Output = Channel;
    fn bitor(self, other: Channel) -> Channel {
        Channel(self.0 | other.0)
    }
}

static ENABLED: AtomicU32 = AtomicU32::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);
static CAPACITY: AtomicU64 = AtomicU64::new(1 << 20);
static NEXT_THREAD: AtomicU32 = AtomicU32::new(0);

/// Turn channels on (replacing the current set).
pub fn enable(channels: Channel) {
    ENABLED.store(channels.0, Ordering::Relaxed);
}

/// The channels currently recording.
pub fn enabled() -> Channel {
    Channel(ENABLED.load(Ordering::Relaxed))
}

#[inline]
pub fn is_enabled(channel: Channel) -> bool {
    ENABLED.load(Ordering::Relaxed) & channel.0 != 0
}

/// Most events kept per thread before the oldest are dropped.
pub fn set_capacity_per_thread(events: usize) {
    CAPACITY.store(events.max(1) as u64, Ordering::Relaxed);
}

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

fn now_ns() -> u64 {
    epoch().elapsed().as_nanos() as u64
}

#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    /// A timed span: start and duration in nanoseconds.
    Zone {
        duration_ns: u64,
    },
    Counter {
        value: f64,
    },
    Frame,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub name: &'static str,
    pub channel: Channel,
    pub thread: u32,
    pub start_ns: u64,
    pub kind: EventKind,
}

struct ThreadBuffer {
    id: u32,
    name: String,
    events: Mutex<VecDeque<Event>>,
}

fn registry() -> &'static Mutex<Vec<Arc<ThreadBuffer>>> {
    static THREADS: OnceLock<Mutex<Vec<Arc<ThreadBuffer>>>> = OnceLock::new();
    THREADS.get_or_init(|| Mutex::new(Vec::new()))
}

thread_local! {
    static LOCAL: RefCell<Option<Arc<ThreadBuffer>>> = const { RefCell::new(None) };
}

fn with_buffer<R>(f: impl FnOnce(&ThreadBuffer) -> R) -> R {
    LOCAL.with(|cell| {
        let mut slot = cell.borrow_mut();
        let buffer = slot.get_or_insert_with(|| {
            let id = NEXT_THREAD.fetch_add(1, Ordering::Relaxed);
            let name =
                std::thread::current().name().map_or_else(|| format!("thread {id}"), String::from);
            let buffer = Arc::new(ThreadBuffer { id, name, events: Mutex::new(VecDeque::new()) });
            registry().lock().unwrap_or_else(|e| e.into_inner()).push(buffer.clone());
            buffer
        });
        f(buffer)
    })
}

fn record(name: &'static str, channel: Channel, start_ns: u64, kind: EventKind) {
    with_buffer(|buffer| {
        let mut events = buffer.events.lock().unwrap_or_else(|e| e.into_inner());
        if events.len() as u64 >= CAPACITY.load(Ordering::Relaxed) {
            events.pop_front();
            DROPPED.fetch_add(1, Ordering::Relaxed);
        }
        events.push_back(Event { name, channel, thread: buffer.id, start_ns, kind });
    });
}

/// Ends its zone when dropped. Made by [`zone!`] or [`Zone::begin`].
#[must_use = "a zone ends when this guard is dropped"]
pub struct Zone {
    open: Option<(&'static str, Channel, u64)>,
    #[cfg(feature = "tracy")]
    _tracy: Option<tracy_client::Span>,
}

impl Zone {
    #[inline]
    pub fn begin(channel: Channel, name: &'static str) -> Zone {
        if !is_enabled(channel) {
            return Zone {
                open: None,
                #[cfg(feature = "tracy")]
                _tracy: None,
            };
        }
        Zone {
            open: Some((name, channel, now_ns())),
            #[cfg(feature = "tracy")]
            _tracy: tracy_client::Client::running().map(|c| c.span_alloc(Some(name), "", "", 0, 0)),
        }
    }
}

impl Drop for Zone {
    #[inline]
    fn drop(&mut self) {
        if let Some((name, channel, start)) = self.open.take() {
            record(
                name,
                channel,
                start,
                EventKind::Zone { duration_ns: now_ns().saturating_sub(start) },
            );
        }
    }
}

/// `let _z = zone!(Channel::CPU, "physics");` times the rest of the scope.
#[macro_export]
macro_rules! zone {
    ($channel:expr, $name:expr) => {
        $crate::Zone::begin($channel, $name)
    };
}

/// Record a value (bytes in use, jobs queued, ...) at this moment.
pub fn counter(channel: Channel, name: &'static str, value: f64) {
    if is_enabled(channel) {
        record(name, channel, now_ns(), EventKind::Counter { value });
        #[cfg(feature = "tracy")]
        if let Some(client) = tracy_client::Client::running() {
            client.plot(tracy_client::PlotName::new_leak(name.to_string()), value);
        }
    }
}

/// Mark the end of a frame.
pub fn frame_mark() {
    if ENABLED.load(Ordering::Relaxed) != 0 {
        record("frame", Channel::CPU, now_ns(), EventKind::Frame);
        #[cfg(feature = "tracy")]
        if let Some(client) = tracy_client::Client::running() {
            client.frame_mark();
        }
    }
}

/// Start the Tracy client so a viewer can connect (only with the `tracy` feature).
pub fn start_tracy() -> bool {
    #[cfg(feature = "tracy")]
    {
        tracy_client::Client::start();
        true
    }
    #[cfg(not(feature = "tracy"))]
    false
}

/// Turn a runtime string into a `&'static str` for a zone name, once per
/// distinct name (for names known only at startup, like system names).
pub fn intern(name: &str) -> &'static str {
    static NAMES: OnceLock<Mutex<Vec<&'static str>>> = OnceLock::new();
    let mut names =
        NAMES.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = names.iter().find(|n| **n == name) {
        return existing;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    names.push(leaked);
    leaked
}

/// Everything recorded since the last drain.
#[derive(Clone, Debug, Default)]
pub struct Trace {
    pub events: Vec<Event>,
    /// Thread id to name.
    pub threads: Vec<(u32, String)>,
    /// Events lost because a thread's buffer was full.
    pub dropped: u64,
}

/// Take every recorded event from every thread.
pub fn drain() -> Trace {
    let mut trace = Trace { dropped: DROPPED.swap(0, Ordering::Relaxed), ..Trace::default() };
    for buffer in registry().lock().unwrap_or_else(|e| e.into_inner()).iter() {
        let mut events = buffer.events.lock().unwrap_or_else(|e| e.into_inner());
        if !events.is_empty() {
            trace.threads.push((buffer.id, buffer.name.clone()));
        }
        trace.events.extend(events.drain(..));
    }
    trace.events.sort_by_key(|e| (e.start_ns, e.thread));
    trace
}

fn json_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl Trace {
    /// Total time and count per zone name, longest total first.
    pub fn summary(&self) -> Vec<(&'static str, u64, usize)> {
        let mut totals: Vec<(&'static str, u64, usize)> = Vec::new();
        for e in &self.events {
            if let EventKind::Zone { duration_ns } = e.kind {
                match totals.iter_mut().find(|t| t.0 == e.name) {
                    Some(t) => {
                        t.1 += duration_ns;
                        t.2 += 1;
                    }
                    None => totals.push((e.name, duration_ns, 1)),
                }
            }
        }
        totals.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        totals
    }

    /// Chrome trace event JSON (open in Perfetto or chrome://tracing).
    pub fn to_chrome_json(&self) -> String {
        let mut out = String::from("{\"traceEvents\":[");
        let mut first = true;
        let mut sep = |out: &mut String| {
            if !first {
                out.push(',');
            }
            first = false;
        };
        for (id, name) in &self.threads {
            sep(&mut out);
            let _ = write!(out, "{{\"ph\":\"M\",\"name\":\"thread_name\",\"pid\":1,\"tid\":{id},\"args\":{{\"name\":");
            json_string(&mut out, name);
            out.push_str("}}");
        }
        for e in &self.events {
            sep(&mut out);
            let ts = e.start_ns as f64 / 1000.0;
            out.push_str("{\"name\":");
            json_string(&mut out, e.name);
            let _ = write!(
                out,
                ",\"cat\":\"{}\",\"pid\":1,\"tid\":{},\"ts\":{ts:.3}",
                e.channel.name(),
                e.thread
            );
            match e.kind {
                EventKind::Zone { duration_ns } => {
                    let _ =
                        write!(out, ",\"ph\":\"X\",\"dur\":{:.3}}}", duration_ns as f64 / 1000.0);
                }
                EventKind::Counter { value } => {
                    let value = if value.is_finite() { value } else { 0.0 };
                    let _ = write!(out, ",\"ph\":\"C\",\"args\":{{\"value\":{value}}}}}");
                }
                EventKind::Frame => out.push_str(",\"ph\":\"i\",\"s\":\"g\"}"),
            }
        }
        let _ = write!(out, "],\"otherData\":{{\"dropped\":{}}}}}", self.dropped);
        out
    }
}
