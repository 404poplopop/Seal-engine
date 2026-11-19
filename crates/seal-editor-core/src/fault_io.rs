//! Simulated disk faults for persistence tests (fail mid-write, delay, disk full).
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use crate::error::CoreError;

/// What the fake disk should do on the next write path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FaultPlan {
    pub fail_after_bytes: Option<u64>,
    pub delay: Option<Duration>,
    pub disk_full: bool,
}

impl FaultPlan {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn fail_at_byte(byte: u64) -> Self {
        Self { fail_after_bytes: Some(byte), delay: None, disk_full: false }
    }

    pub fn slow(delay: Duration) -> Self {
        Self { fail_after_bytes: None, delay: Some(delay), disk_full: false }
    }

    pub fn full() -> Self {
        Self { fail_after_bytes: None, delay: None, disk_full: true }
    }
}

/// Append bytes like a journal record write, applying `plan` on the way to disk.
pub fn append_bytes(path: &Path, bytes: &[u8], plan: &FaultPlan) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    write_all(&mut file, bytes, plan)?;
    sync_all(&mut file, plan)?;
    Ok(())
}

/// Leave only the first `byte_count` bytes on disk (simulates a kill mid-write).
pub fn truncate_file(path: &Path, byte_count: u64) -> Result<(), CoreError> {
    let file = OpenOptions::new().write(true).open(path)?;
    file.set_len(byte_count)?;
    file.sync_all().map_err(CoreError::from)?;
    Ok(())
}

/// How many randomized fault iterations to run; set `SEAL_JOURNAL_FAULT_RUNS`.
pub fn env_run_count() -> Option<u32> {
    std::env::var("SEAL_JOURNAL_FAULT_RUNS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .filter(|count| *count > 0)
}

fn write_all(file: &mut File, bytes: &[u8], plan: &FaultPlan) -> Result<(), CoreError> {
    if plan.disk_full {
        return Err(storage_full("injected disk full"));
    }
    if let Some(delay) = plan.delay {
        std::thread::sleep(delay);
    }
    let fail_at = plan.fail_after_bytes;
    if let Some(limit) = fail_at {
        if limit == 0 {
            return Err(kill_fault("injected fail at byte 0"));
        }
        let keep = limit.min(bytes.len() as u64) as usize;
        file.write_all(&bytes[..keep]).map_err(CoreError::from)?;
        if keep < bytes.len() {
            return Err(kill_fault("injected fail mid-write"));
        }
        return Ok(());
    }
    file.write_all(bytes).map_err(CoreError::from)
}

fn sync_all(file: &mut File, plan: &FaultPlan) -> Result<(), CoreError> {
    if plan.disk_full {
        return Err(storage_full("injected disk full at fsync"));
    }
    if let Some(delay) = plan.delay {
        std::thread::sleep(delay);
    }
    file.sync_all().map_err(CoreError::from)
}

fn storage_full(message: &str) -> CoreError {
    let err = io::Error::new(io::ErrorKind::StorageFull, message);
    CoreError::from(err)
}

fn kill_fault(message: &str) -> CoreError {
    CoreError::new(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn fail_at_byte_leaves_a_partial_append() {
        let dir = std::env::temp_dir().join(format!("seal-fault-byte-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("log");
        let data = b"0123456789abcdef";
        let err = append_bytes(&path, data, &FaultPlan::fail_at_byte(5)).unwrap_err();
        assert!(err.to_string().contains("mid-write"));
        assert_eq!(fs::read(&path).unwrap(), b"01234");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delay_and_disk_full_are_distinct_errors() {
        let dir = std::env::temp_dir().join(format!("seal-fault-delay-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("log");
        let start = Instant::now();
        append_bytes(&path, b"x", &FaultPlan::slow(Duration::from_millis(40))).unwrap();
        assert!(start.elapsed() >= Duration::from_millis(35));
        let full = append_bytes(&path, b"y", &FaultPlan::full()).unwrap_err();
        assert_eq!(full.fault, Some(crate::save_fault::SaveFault::DiskFull));
        let _ = fs::remove_dir_all(&dir);
    }
}
