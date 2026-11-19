use std::backtrace::Backtrace;
use std::fs;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::CoreError;

pub fn write_diagnostics(directory: &Path, recovery: &str) -> Result<PathBuf, CoreError> {
    fs::create_dir_all(directory)?;
    let path = directory.join("diagnostics.txt");
    let log_path = directory.join("editor.log");
    let log = fs::read_to_string(&log_path).unwrap_or_default();
    let body = format!(
        "version {}\nos {} {}\nrecovery {}\n\n{log}",
        crate::version::ENGINE_VERSION_FULL,
        std::env::consts::OS,
        std::env::consts::ARCH,
        recovery
    );
    fs::write(&path, body)?;
    Ok(path)
}

pub fn install_panic_hook(directory: PathBuf) {
    std::panic::set_hook(Box::new(move |info| {
        if let Err(err) = write_crash(&directory, info) {
            eprintln!("failed to write crash report: {err}");
        }
    }));
}

pub fn write_crash(directory: &Path, info: &PanicHookInfo) -> Result<PathBuf, CoreError> {
    fs::create_dir_all(directory)?;
    let stamp = crash_report_timestamp();
    let path = directory.join(format!("crash-{stamp}.txt"));
    let log_path = directory.join("editor.log");
    let log = fs::read_to_string(&log_path).unwrap_or_default();
    let log_tail = tail_log_lines(&log, 200);
    let backtrace = Backtrace::force_capture();
    let mut body = format!(
        "version {}\nos {} {}\n\npanic:\n{}\n\nbacktrace:\n{}\n\nlog (last 200 lines):\n{}",
        crate::version::ENGINE_VERSION_FULL,
        std::env::consts::OS,
        std::env::consts::ARCH,
        redact_paths(&info.to_string()),
        redact_paths(&format!("{backtrace}")),
        redact_paths(&log_tail),
    );
    if let Some(settings_path) = crash_settings_path(directory) {
        let settings = fs::read_to_string(&settings_path)?;
        body.push_str("\n\nsettings.seal:\n");
        body.push_str(&redact_paths(&settings));
    }
    fs::write(&path, &body)?;
    fs::write(directory.join("unclean-shutdown"), b"panic")?;
    Ok(path)
}

pub fn mark_clean_exit(directory: &Path) -> Result<(), CoreError> {
    fs::create_dir_all(directory)?;
    let unclean = directory.join("unclean-shutdown");
    if unclean.exists() {
        fs::remove_file(unclean)?;
    }
    fs::write(directory.join("clean-shutdown"), b"ok")?;
    Ok(())
}

/// Returns true when the previous process did not record a clean exit.
pub fn take_unclean_start(directory: &Path) -> bool {
    let clean = directory.join("clean-shutdown");
    let was_clean = clean.exists();
    let _ = fs::remove_file(&clean);
    !was_clean && directory.exists()
}

fn crash_settings_path(directory: &Path) -> Option<PathBuf> {
    let direct = directory.join("settings.seal");
    if direct.is_file() {
        return Some(direct);
    }
    directory.parent().map(|parent| parent.join("settings.seal")).filter(|path| path.is_file())
}

pub fn redact_paths(text: &str) -> String {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
    if home.is_empty() {
        text.to_string()
    } else {
        text.replace(&home, "~")
    }
}

fn tail_log_lines(log: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = log.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

fn crash_report_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let time_of_day = secs % 86_400;
    let hour = time_of_day / 3_600;
    let minute = (time_of_day % 3_600) / 60;
    let (year, month, day) = civil_date_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}-{minute:02}")
}

fn civil_date_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    if mp < 10 {
        (year, month, day)
    } else {
        year += 1;
        (year, month, day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_include_version_and_a_clean_exit_is_remembered() {
        let dir = std::env::temp_dir().join(format!("seal-diag-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("editor.log"), "INFO shell hello\n").unwrap();
        let path = write_diagnostics(&dir, "restored layout").unwrap();
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains(crate::version::ENGINE_VERSION_FULL));
        assert!(text.contains("restored layout"));
        assert!(text.contains("hello"));
        assert!(take_unclean_start(&dir));
        mark_clean_exit(&dir).unwrap();
        assert!(!take_unclean_start(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn crash_settings_are_found_beside_the_log_directory() {
        let dir = std::env::temp_dir().join(format!("seal-crash-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let logs = dir.join("logs");
        fs::create_dir_all(&logs).unwrap();
        assert!(crash_settings_path(&logs).is_none());
        fs::write(dir.join("settings.seal"), "ui.theme = \"dark\"\n").unwrap();
        assert_eq!(crash_settings_path(&logs).unwrap(), dir.join("settings.seal"));
        fs::write(logs.join("settings.seal"), "ui.theme = \"light\"\n").unwrap();
        assert_eq!(crash_settings_path(&logs).unwrap(), logs.join("settings.seal"));
        let _ = fs::remove_dir_all(&dir);
    }
}
