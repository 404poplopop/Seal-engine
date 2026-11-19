use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::CoreError;

/// Write `bytes` so a crash cannot replace `path` with a partial file.
/// The previous complete file is copied to a `.bak` beside it before the new
/// file takes the name, so a crash still leaves the old file or the new one.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let temp = temp_path(path)?;
    if let Err(err) = write_synced(&temp, bytes) {
        let _ = fs::remove_file(&temp);
        return Err(err);
    }
    replace_temp(&temp, path)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = sync_directory(parent);
        }
    }
    Ok(())
}

/// Move the temp file onto `path`. A scanner can hold a new file for a moment, so this retries
/// and then copies if the rename still fails.
fn replace_temp(temp: &Path, path: &Path) -> Result<(), CoreError> {
    if let Ok(meta) = fs::metadata(path) {
        if meta.permissions().readonly() {
            let _ = fs::remove_file(temp);
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "the file is read-only",
            )
            .into());
        }
    }
    let mut last = CoreError::new("save did not land");
    for _ in 0..6 {
        if path.exists() {
            if let Err(err) = refresh_backup(path) {
                last = err;
                std::thread::sleep(std::time::Duration::from_millis(30));
                continue;
            }
            if let Err(err) = fs::remove_file(path) {
                last = err.into();
                std::thread::sleep(std::time::Duration::from_millis(30));
                continue;
            }
        }
        match fs::rename(temp, path) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last = err.into();
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        }
    }
    match fs::copy(temp, path) {
        Ok(_) => {
            let _ = fs::remove_file(temp);
            Ok(())
        }
        Err(_) => {
            let _ = fs::remove_file(temp);
            Err(last)
        }
    }
}

/// Copy the current file to its `.bak` through a temp file, so a kill mid-copy
/// leaves the old backup whole instead of a half-written one.
fn refresh_backup(path: &Path) -> Result<(), CoreError> {
    let backup = backup_path(path)?;
    let temp = temp_path(&backup)?;
    let copied = fs::copy(path, &temp).map_err(CoreError::from).and_then(|_| {
        // Windows refuses to flush a read-only handle, so open it for writing,
        // and treat the flush as best effort like the old in-place copy did.
        if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&temp) {
            let _ = file.sync_all();
        }
        fs::rename(&temp, &backup)?;
        Ok(())
    });
    if copied.is_err() {
        let _ = fs::remove_file(&temp);
    }
    copied
}

/// The file is already synced. Some systems deny a directory sync; that does not undo the write.
fn sync_directory(parent: &Path) -> Result<(), CoreError> {
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// The primary file when it has bytes, otherwise the backup. Empty means neither is usable.
pub fn read_durable(path: &Path) -> Result<Option<Vec<u8>>, CoreError> {
    if path.is_file() {
        let bytes = fs::read(path)?;
        if !bytes.is_empty() {
            return Ok(Some(bytes));
        }
    }
    let backup = backup_path(path)?;
    if backup.is_file() {
        let bytes = fs::read(&backup)?;
        if !bytes.is_empty() {
            return Ok(Some(bytes));
        }
    }
    Ok(None)
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

pub fn backup_path(path: &Path) -> Result<PathBuf, CoreError> {
    let name = path.file_name().ok_or_else(|| CoreError::new("path has no file name"))?;
    let mut backup = name.to_os_string();
    backup.push(".bak");
    Ok(path.with_file_name(backup))
}

fn temp_path(path: &Path) -> Result<PathBuf, CoreError> {
    let name = path.file_name().ok_or_else(|| CoreError::new("path has no file name"))?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let mut temp_name = std::ffi::OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(".{}.{nanos}.tmp", std::process::id()));
    Ok(path.with_file_name(temp_name))
}

pub fn move_aside(path: &Path) -> Result<PathBuf, CoreError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let name = path.file_name().ok_or_else(|| CoreError::new("path has no file name"))?;
    let mut aside = name.to_os_string();
    aside.push(format!(".bad-{nanos}"));
    let destination = path.with_file_name(aside);
    fs::rename(path, &destination)?;
    Ok(destination)
}

/// Ask the editor that already owns this state directory to come forward.
pub fn write_focus_request(dir: &Path) -> Result<(), CoreError> {
    atomic_write(&dir.join("focus.seal"), std::process::id().to_string().as_bytes())
}

/// The process id in a focus request, if one is waiting.
pub fn read_focus_request(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("focus.seal")).ok()?;
    let text = text.trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

pub fn clear_focus_request(dir: &Path) -> Result<(), CoreError> {
    let path = dir.join("focus.seal");
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Exclusive lock for one editor state directory.
/// The operating system drops the lock if this process dies.
pub struct EditorLock {
    file: File,
}

impl EditorLock {
    pub fn acquire(directory: &Path) -> Result<Self, CoreError> {
        fs::create_dir_all(directory)?;
        let path = directory.join("session.lock");
        let file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)?;
        file.try_lock().map_err(|err| match err {
            std::fs::TryLockError::WouldBlock => {
                CoreError::new("another editor is already using this state")
            }
            std::fs::TryLockError::Error(io) => CoreError::from(io),
        })?;
        Ok(Self { file })
    }
}

impl Drop for EditorLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LockOwner {
    pid: u32,
    machine: String,
}

/// Exclusive lock for one project folder, with the owning process and machine written beside it.
pub struct ProjectGuard {
    file: File,
    pub replaced_stale: bool,
}

impl ProjectGuard {
    pub fn acquire(root: &Path) -> Result<Self, CoreError> {
        fs::create_dir_all(root)?;
        let path = root.join("project.lock");
        let mut file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => {
                let previous = read_lock_owner(&mut file);
                let replaced_stale = previous.as_ref().is_some_and(owner_is_stale);
                write_lock_owner(&mut file)?;
                Ok(Self { file, replaced_stale })
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                let (machine, pid) = match read_lock_owner(&mut file) {
                    Some(owner) => (owner.machine, owner.pid.to_string()),
                    None => ("unknown".to_string(), "0".to_string()),
                };
                Err(CoreError::new(format!("project-open-elsewhere\t{machine}\t{pid}")))
            }
            Err(std::fs::TryLockError::Error(err)) => Err(err.into()),
        }
    }
}

impl Drop for ProjectGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn machine_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".into())
}

fn read_lock_owner(file: &mut File) -> Option<LockOwner> {
    use std::io::{Read, Seek, SeekFrom};
    file.seek(SeekFrom::Start(0)).ok()?;
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    let mut lines = text.lines();
    let pid = lines.next()?.parse().ok()?;
    let machine = lines.next().unwrap_or("unknown").to_string();
    Some(LockOwner { pid, machine })
}

fn write_lock_owner(file: &mut File) -> Result<(), CoreError> {
    use std::io::{Seek, SeekFrom, Write};
    file.seek(SeekFrom::Start(0))?;
    file.set_len(0)?;
    writeln!(file, "{}", std::process::id())?;
    writeln!(file, "{}", machine_name())?;
    file.sync_all()?;
    Ok(())
}

fn owner_is_stale(owner: &LockOwner) -> bool {
    if owner.pid == 0 || owner.pid == std::process::id() {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        !Path::new(&format!("/proc/{}", owner.pid)).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replaced_file_keeps_the_previous_bytes_in_the_backup() {
        let dir = std::env::temp_dir().join(format!("seal-atomic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scene.seal");
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert_eq!(fs::read(backup_path(&path).unwrap()).unwrap(), b"first");
        fs::remove_file(&path).unwrap();
        assert_eq!(read_durable(&path).unwrap().as_deref(), Some(b"first".as_slice()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_read_only_folder_is_permission_and_a_long_name_is_too_long() {
        let dir = std::env::temp_dir().join(format!("seal-readonly-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // A read-only file is refused everywhere. Windows ignores the read-only
        // flag on folders, so the folder case runs on Unix only.
        let locked = dir.join("locked.seal");
        fs::write(&locked, b"old").unwrap();
        let mut perms = fs::metadata(&locked).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&locked, perms).unwrap();
        let err = atomic_write(&locked, b"kept").unwrap_err();
        assert_eq!(err.fault, Some(crate::save_fault::SaveFault::Permission));
        assert_eq!(fs::read(&locked).unwrap(), b"old");
        allow_writes(&locked);
        fs::remove_file(&locked).unwrap();
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&dir).unwrap().permissions();
            perms.set_readonly(true);
            fs::set_permissions(&dir, perms).unwrap();
            let err = atomic_write(&dir.join("scene.seal"), b"kept").unwrap_err();
            assert_eq!(err.fault, Some(crate::save_fault::SaveFault::Permission));
        }
        let plan =
            crate::save_fault::plan_save_failure(crate::save_fault::SaveFault::Permission, 0)
                .unwrap();
        assert_eq!(plan.heading_key, "save-cannot");
        assert_eq!(plan.action_key, "save-elsewhere");
        allow_writes(&dir);

        let long = dir.join("a".repeat(300));
        let err = atomic_write(&long, b"x").unwrap_err();
        assert_eq!(err.fault, Some(crate::save_fault::SaveFault::PathTooLong));
        let _ = fs::remove_dir_all(&dir);
    }

    fn allow_writes(dir: &Path) {
        let mut perms = fs::metadata(dir).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
        }
        #[cfg(not(unix))]
        {
            #[allow(clippy::permissions_set_readonly_false)]
            {
                perms.set_readonly(false);
            }
        }
        fs::set_permissions(dir, perms).unwrap();
    }
}
