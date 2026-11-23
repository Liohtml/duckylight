//! One program holds the keyboard. A second one is refused; it does not queue or take over.
//!
//! The lock is an OS file lock, so a crashed holder releases it by dying -- a lock file left
//! behind never keeps the program from starting again. The holder's PID sits in a file beside
//! it, only for the message. tools/acceptance_frames.py honours the same file.
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum LockError {
    /// Another program holds the keyboard; its PID, or 0 if unknown.
    Held(u32),
    Io(std::io::Error),
}

pub struct Lock {
    _file: File,
    pid_path: PathBuf,
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.pid_path);
    }
}

/// `$XDG_RUNTIME_DIR/duckylight.lock`, or `%LOCALAPPDATA%\duckylight\duckylight.lock`.
pub fn default_path() -> PathBuf {
    #[cfg(windows)]
    let dir = std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("duckylight"));
    #[cfg(not(windows))]
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let dir = dir.unwrap_or_else(std::env::temp_dir);
    let _ = std::fs::create_dir_all(&dir);
    dir.join("duckylight.lock")
}

pub fn acquire() -> Result<Lock, LockError> {
    acquire_at(&default_path())
}

pub fn acquire_at(path: &Path) -> Result<Lock, LockError> {
    let pid_path = path.with_extension("pid");
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(path).map_err(LockError::Io)?;
    match file.try_lock() {
        Ok(()) => {
            let _ = std::fs::write(&pid_path, std::process::id().to_string());
            Ok(Lock { _file: file, pid_path })
        }
        Err(std::fs::TryLockError::WouldBlock) => {
            let pid = std::fs::read_to_string(&pid_path).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
            Err(LockError::Held(pid))
        }
        Err(std::fs::TryLockError::Error(e)) => Err(LockError::Io(e)),
    }
}
