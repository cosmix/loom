//! Per-session stderr capture: where the wrapper tees claude's stderr and
//! where the crash handler reads it back.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn logs_dir(work_dir: &Path) -> PathBuf {
    work_dir.join("logs")
}

pub fn create_logs_dir(work_dir: &Path) -> Result<()> {
    let dir = logs_dir(work_dir);
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create logs directory: {}", dir.display()))
}

/// Get the path to the log the wrapper tees claude's stderr into.
///
/// Keyed by the SESSION ID rather than by the pid key, because the reader is
/// the crash handler, which holds a session id and nothing else. A stage that
/// crashed and retried therefore keeps one log per attempt.
pub fn stderr_log_path(work_dir: &Path, session_id: &str) -> PathBuf {
    logs_dir(work_dir).join(format!("{session_id}.stderr.log"))
}
