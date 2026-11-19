//! Structured log used by the editor shell.
#![forbid(unsafe_code)]
//!
//! Editor mode records every level and can append to a rotating file.
//! Ship mode drops anything finer than a warning before the message is formatted.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread::JoinHandle;

const MEMORY_CAPACITY: usize = 4_000;
const FILE_QUEUE_CAPACITY: usize = 2_048;
const FILE_MAX_BYTES: u64 = 20 * 1024 * 1024;

/// How much detail a build is willing to pay for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogMode {
    /// Keep every level, including trace.
    Editor,
    /// Keep errors and warnings only, and skip formatting the rest.
    Ship,
}

/// Severity. Lower values are more severe. Higher values are more verbose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl Level {
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        }
    }

    fn is_verbose(self) -> bool {
        matches!(self, Level::Debug | Level::Trace)
    }
}

/// One formatted line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub level: Level,
    pub source: String,
    pub message: String,
    pub span: String,
    pub fields: Option<BTreeMap<String, String>>,
}

impl Record {
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.get_or_insert_with(BTreeMap::new).insert(key.into(), value.into());
        self
    }
}

fn format_field_suffix(fields: &BTreeMap<String, String>) -> String {
    fields
        .iter()
        .map(|(key, value)| {
            let key = key.replace('\n', " ");
            let value = value.replace('\n', " ");
            format!("{key}={value}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

struct FileSink {
    sender: SyncSender<Record>,
    dropped: Arc<AtomicU64>,
    _worker: JoinHandle<()>,
}

impl FileSink {
    fn start(path: PathBuf, file_error: Arc<Mutex<Option<String>>>) -> Self {
        let (sender, receiver) = sync_channel(FILE_QUEUE_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let dropped_worker = Arc::clone(&dropped);
        let spawn_error = Arc::clone(&file_error);
        let worker = std::thread::Builder::new()
            .name("seal-log-file".into())
            .spawn(move || {
                file_worker(path, receiver, dropped_worker, file_error);
            })
            .unwrap_or_else(|err| {
                if let Ok(mut slot) = spawn_error.lock() {
                    *slot = Some(err.to_string());
                }
                std::thread::spawn(|| {})
            });
        Self { sender, dropped, _worker: worker }
    }

    fn enqueue(&self, record: Record) -> bool {
        match self.sender.try_send(record.clone()) {
            Ok(()) => true,
            Err(TrySendError::Full(held)) if held.level.is_verbose() => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
            Err(TrySendError::Full(held)) => {
                let _ = self.sender.send(held);
                true
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }

    fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

fn file_worker(
    path: PathBuf,
    receiver: std::sync::mpsc::Receiver<Record>,
    dropped: Arc<AtomicU64>,
    file_error: Arc<Mutex<Option<String>>>,
) {
    for record in receiver {
        write_record_to_disk(&path, &record, &file_error);
        let dropped_count = dropped.swap(0, Ordering::Relaxed);
        if dropped_count > 0 {
            let notice = Record {
                level: Level::Warn,
                source: "seal_log".into(),
                message: format!("dropped {dropped_count} log entries from the file queue"),
                span: String::new(),
                fields: None,
            };
            write_record_to_disk(&path, &notice, &file_error);
        }
    }
}

fn write_record_to_disk(path: &Path, record: &Record, file_error: &Arc<Mutex<Option<String>>>) {
    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            store_file_error(file_error, err.to_string());
            return;
        }
    }
    if let Err(err) = rotate_log_for_new_day(path) {
        store_file_error(file_error, err.to_string());
        return;
    }
    if let Err(err) = rotate_log_if_oversized(path) {
        store_file_error(file_error, err.to_string());
        return;
    }
    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut file) => {
            let message = record.message.replace('\n', " ");
            let mut line = if record.span.is_empty() {
                format!("{} {} {}", record.level.as_str(), record.source, message)
            } else {
                format!("{} {} [{}] {}", record.level.as_str(), record.source, record.span, message)
            };
            if let Some(fields) = record.fields.as_ref().filter(|map| !map.is_empty()) {
                line.push(' ');
                line.push_str(&format_field_suffix(fields));
            }
            line.push('\n');
            if let Err(err) = file.write_all(line.as_bytes()) {
                store_file_error(file_error, err.to_string());
            }
        }
        Err(err) => store_file_error(file_error, err.to_string()),
    }
}

fn store_file_error(slot: &Arc<Mutex<Option<String>>>, message: String) {
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(message);
    }
}

fn current_epoch_day() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|age| age.as_secs() / 86_400)
        .unwrap_or(0)
}

fn file_epoch_day(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    modified.duration_since(std::time::UNIX_EPOCH).ok().map(|age| age.as_secs() / 86_400)
}

/// When the calendar day changes, archive the active file under a day-stamped name.
fn rotate_log_for_new_day(path: &Path) -> std::io::Result<()> {
    if !path.is_file() {
        return Ok(());
    }
    let Some(file_day) = file_epoch_day(path) else {
        return Ok(());
    };
    let today = current_epoch_day();
    if file_day == today {
        return Ok(());
    }
    let archive = path.with_extension(format!("log.{file_day}"));
    if archive.exists() {
        fs::remove_file(&archive)?;
    }
    fs::rename(path, archive)
}

fn rotate_log_if_oversized(path: &Path) -> std::io::Result<()> {
    let Ok(meta) = fs::metadata(path) else {
        return Ok(());
    };
    if meta.len() <= FILE_MAX_BYTES {
        return Ok(());
    }
    let day = file_epoch_day(path).unwrap_or_else(current_epoch_day);
    let archive = path.with_extension(format!("log.{day}.1"));
    if archive.exists() {
        fs::remove_file(&archive)?;
    }
    fs::rename(path, archive)
}

/// In-memory ring plus an optional append-only file.
pub struct Logger {
    mode: LogMode,
    records: Vec<Record>,
    capacity: usize,
    file: Option<PathBuf>,
    file_sink: Option<FileSink>,
    file_error: Arc<Mutex<Option<String>>>,
    memory_dropped: u64,
    ceiling: Level,
    span_depth: usize,
    span_stack: Vec<String>,
}

impl Logger {
    pub fn new(mode: LogMode) -> Self {
        Self {
            mode,
            records: Vec::new(),
            capacity: MEMORY_CAPACITY,
            file: None,
            file_sink: None,
            file_error: Arc::new(Mutex::new(None)),
            memory_dropped: 0,
            ceiling: Level::Trace,
            span_depth: 0,
            span_stack: Vec::new(),
        }
    }

    pub fn with_ceiling(mut self, level: Level) -> Self {
        self.ceiling = level;
        self
    }

    pub fn set_ceiling(&mut self, level: Level) {
        self.ceiling = level;
    }

    pub fn with_memory_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity.max(1);
        self
    }

    pub fn with_file(mut self, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        self.file = Some(path.clone());
        self.file_sink = Some(FileSink::start(path, Arc::clone(&self.file_error)));
        self
    }

    pub fn mode(&self) -> LogMode {
        self.mode
    }

    pub fn enabled(&self, level: Level) -> bool {
        level <= self.ceiling && level <= self.threshold()
    }

    pub fn threshold(&self) -> Level {
        match self.mode {
            LogMode::Editor => Level::Trace,
            LogMode::Ship => Level::Warn,
        }
    }

    /// Records dropped from the in-memory buffer because it was full.
    pub fn memory_dropped(&self) -> u64 {
        self.memory_dropped
    }

    /// Records dropped because the file queue refused verbose lines.
    pub fn file_dropped(&self) -> u64 {
        self.file_sink.as_ref().map(FileSink::dropped).unwrap_or(0)
    }

    pub fn dropped_total(&self) -> u64 {
        self.memory_dropped + self.file_dropped()
    }

    pub fn span_depth(&self) -> usize {
        self.span_depth
    }

    pub fn span_enter(&mut self, name: impl Into<String>) {
        self.span_stack.push(name.into());
        self.span_depth = self.span_stack.len();
    }

    pub fn span_leave(&mut self) {
        if self.span_stack.pop().is_some() {
            self.span_depth = self.span_stack.len();
        }
    }

    fn current_span(&self) -> String {
        self.span_stack.last().cloned().unwrap_or_default()
    }

    pub fn record(&mut self, level: Level, source: impl Into<String>, message: impl Into<String>) {
        if !self.enabled(level) {
            return;
        }
        let record = Record {
            level,
            source: source.into(),
            message: message.into(),
            span: self.current_span(),
            fields: None,
        };
        self.enqueue_file(record.clone());
        self.push_memory(record);
    }

    pub fn records(&self) -> &[Record] {
        &self.records
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.memory_dropped = 0;
        self.span_stack.clear();
        self.span_depth = 0;
    }

    pub fn file_error(&self) -> Option<String> {
        self.file_error.lock().ok().and_then(|guard| guard.clone())
    }

    fn enqueue_file(&mut self, record: Record) {
        let Some(sink) = self.file_sink.as_ref() else {
            return;
        };
        let _ = sink.enqueue(record);
    }

    fn push_memory(&mut self, record: Record) {
        if self.records.len() >= self.capacity {
            if let Some(index) = self.records.iter().position(|item| item.level.is_verbose()) {
                self.records.remove(index);
                self.memory_dropped = self.memory_dropped.saturating_add(1);
            } else {
                self.records.remove(0);
                self.memory_dropped = self.memory_dropped.saturating_add(1);
            }
        }
        self.records.push(record);
    }
}

fn global() -> &'static Mutex<Logger> {
    static LOGGER: OnceLock<Mutex<Logger>> = OnceLock::new();
    LOGGER.get_or_init(|| Mutex::new(Logger::new(LogMode::Editor)))
}

pub fn install(logger: Logger) {
    *global().lock().expect("log lock") = logger;
}

pub fn with_logger<R>(body: impl FnOnce(&mut Logger) -> R) -> R {
    let mut guard: MutexGuard<'_, Logger> = global().lock().expect("log lock");
    body(&mut guard)
}

pub fn log(level: Level, source: &str, message: String) {
    with_logger(|logger| logger.record(level, source, message));
}

/// Write one line to the log file and leave the on-screen console alone.
pub fn file_only(level: Level, source: &str, message: String) {
    with_logger(|logger| {
        if !logger.enabled(level) {
            return;
        }
        let span = logger.current_span();
        logger.enqueue_file(Record {
            level,
            source: source.to_string(),
            message,
            span,
            fields: None,
        });
    });
}

pub fn span_enter(name: impl Into<String>) {
    with_logger(|logger| logger.span_enter(name));
}

pub fn span_leave() {
    with_logger(|logger| logger.span_leave());
}

pub fn read_log_file(path: &Path) -> std::io::Result<String> {
    fs::read_to_string(path)
}

/// Ceiling for a settings pair. Verbose keeps trace lines and is the slow path.
pub fn ceiling_for(level: &str, verbose: bool) -> Level {
    if verbose {
        Level::Trace
    } else {
        Level::parse(level).unwrap_or(Level::Info)
    }
}

/// Delete `editor.log*` files in `dir` that are older than `retention_days`.
pub fn prune_old_logs(dir: &Path, retention_days: u64) -> std::io::Result<usize> {
    if retention_days == 0 || !dir.is_dir() {
        return Ok(0);
    }
    let max_age = std::time::Duration::from_secs(retention_days.saturating_mul(24 * 60 * 60));
    let now = std::time::SystemTime::now();
    let mut removed = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("editor.log") {
            continue;
        }
        let modified = entry.metadata()?.modified()?;
        if now.duration_since(modified).unwrap_or_default() > max_age {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Confirm a file can be opened, used by callers that want a real sink.
pub fn touch_log(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    File::create(path)?;
    Ok(())
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {{
        if $crate::with_logger(|logger| logger.enabled($crate::Level::Error)) {
            $crate::log($crate::Level::Error, module_path!(), format!($($arg)*));
        }
    }};
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {{
        if $crate::with_logger(|logger| logger.enabled($crate::Level::Warn)) {
            $crate::log($crate::Level::Warn, module_path!(), format!($($arg)*));
        }
    }};
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        if $crate::with_logger(|logger| logger.enabled($crate::Level::Info)) {
            $crate::log($crate::Level::Info, module_path!(), format!($($arg)*));
        }
    }};
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {{
        if $crate::with_logger(|logger| logger.enabled($crate::Level::Debug)) {
            $crate::log($crate::Level::Debug, module_path!(), format!($($arg)*));
        }
    }};
}

#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {{
        if $crate::with_logger(|logger| logger.enabled($crate::Level::Trace)) {
            $crate::log($crate::Level::Trace, module_path!(), format!($($arg)*));
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ship_mode_drops_debug_before_the_message_is_stored() {
        let mut logger = Logger::new(LogMode::Ship);
        let mut built = false;
        if logger.enabled(Level::Debug) {
            built = true;
            logger.record(Level::Debug, "test", "should not appear");
        }
        logger.record(Level::Error, "test", "kept");
        assert!(!built);
        assert_eq!(logger.records().len(), 1);
        assert_eq!(logger.records()[0].level, Level::Error);
    }

    #[test]
    fn editor_mode_keeps_trace_and_rotates_a_file() {
        let dir = std::env::temp_dir().join(format!("seal-log-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("editor.log");
        let mut logger = Logger::new(LogMode::Editor).with_file(&path);
        logger.record(Level::Trace, "shell", "hello");
        assert_eq!(logger.records().len(), 1);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("TRACE shell hello"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verbose_keeps_trace_and_old_log_files_are_removed() {
        assert_eq!(ceiling_for("info", false), Level::Info);
        assert_eq!(ceiling_for("info", true), Level::Trace);
        let mut logger = Logger::new(LogMode::Editor).with_ceiling(ceiling_for("info", false));
        logger.record(Level::Trace, "shell", "hidden");
        assert!(logger.records().is_empty());
        logger.set_ceiling(ceiling_for("info", true));
        logger.record(Level::Trace, "shell", "visible");
        assert_eq!(logger.records().len(), 1);
        assert_eq!(logger.records()[0].message, "visible");

        let dir = std::env::temp_dir().join(format!("seal-log-prune-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let old = dir.join("editor.log.1");
        let fresh = dir.join("editor.log");
        fs::write(&old, b"old").unwrap();
        fs::write(&fresh, b"fresh").unwrap();
        let aged = std::time::SystemTime::now() - std::time::Duration::from_secs(20 * 24 * 60 * 60);
        fs::File::options().write(true).open(&old).unwrap().set_modified(aged).unwrap();
        assert_eq!(prune_old_logs(&dir, 14).unwrap(), 1);
        assert!(!old.exists());
        assert!(fresh.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn memory_ring_drops_verbose_lines_first() {
        let mut logger = Logger::new(LogMode::Editor).with_memory_capacity(2);
        logger.record(Level::Info, "a", "one");
        logger.record(Level::Debug, "b", "two");
        logger.record(Level::Warn, "c", "three");
        assert_eq!(logger.records().len(), 2);
        assert_eq!(logger.records()[0].message, "one");
        assert_eq!(logger.records()[1].message, "three");
        assert_eq!(logger.memory_dropped(), 1);
    }

    #[test]
    fn structured_fields_are_appended_when_present() {
        let dir = std::env::temp_dir().join(format!("seal-log-fields-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("editor.log");
        let record = Record {
            level: Level::Info,
            source: "shell".into(),
            message: "saved\nscene".into(),
            span: String::new(),
            fields: None,
        }
        .with_field("scene", "level\n01");
        let error = Arc::new(Mutex::new(None));
        write_record_to_disk(&path, &record, &error);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("INFO shell saved scene scene=level 01"));
        assert!(error.lock().unwrap().is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
