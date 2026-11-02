//! Durable per-dispute counters for session and verdict retry budgets.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::fs::locking::{atomic_write_locked, locked_dir_update};

#[cfg(test)]
thread_local! {
    static FAIL_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(super) fn with_write_failure<T>(f: impl FnOnce() -> T) -> T {
    FAIL_WRITE.with(|flag| {
        struct Reset<'a>(&'a std::cell::Cell<bool>, bool);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        let _reset = Reset(flag, flag.replace(true));
        f()
    })
}

#[derive(Debug)]
pub(super) struct CounterError {
    path: PathBuf,
    operation: &'static str,
    detail: String,
}

impl CounterError {
    fn new(path: &Path, operation: &'static str, detail: impl fmt::Display) -> Self {
        Self {
            path: path.to_path_buf(),
            operation,
            detail: detail.to_string(),
        }
    }
}

impl fmt::Display for CounterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot {} counter {}: {}",
            self.operation,
            self.path.display(),
            self.detail
        )
    }
}

impl std::error::Error for CounterError {}

/// Only a counter that has never existed starts at zero.
pub(super) fn read(path: &Path) -> Result<u32, CounterError> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return match std::fs::symlink_metadata(path) {
                Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => Ok(0),
                Ok(_) => Err(CounterError::new(path, "read", error)),
                Err(other) => Err(CounterError::new(path, "inspect", other)),
            };
        }
        Err(error) => return Err(CounterError::new(path, "read", error)),
    };
    raw.trim()
        .parse::<u32>()
        .map_err(|error| CounterError::new(path, "parse", error))
}

/// Serialize a read-modify-write against the dispute directory lock and
/// publish atomically before allowing the caller to consume the charge.
pub(super) fn charge(path: &Path) -> Result<u32, CounterError> {
    let parent = path
        .parent()
        .ok_or_else(|| CounterError::new(path, "locate", "no parent"))?;
    locked_dir_update(parent, || {
        let next = read(path)?
            .checked_add(1)
            .ok_or_else(|| CounterError::new(path, "increment", "counter overflow"))?;
        #[cfg(test)]
        if FAIL_WRITE.with(|flag| flag.get()) {
            return Err(CounterError::new(path, "write", "injected write failure").into());
        }
        atomic_write_locked(path, &next.to_string())
            .map_err(|error| CounterError::new(path, "write", error))?;
        Ok(next)
    })
    .map_err(|error| {
        error
            .downcast::<CounterError>()
            .unwrap_or_else(|error| CounterError::new(path, "lock", error))
    })
}
