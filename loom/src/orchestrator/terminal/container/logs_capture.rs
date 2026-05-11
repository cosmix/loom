//! Container log capture for crash reports and diagnostics.
//!
//! When a container session exits abnormally, `<runtime> logs` is the only
//! way to recover stderr/stdout from the entrypoint or firewall script.
//! These helpers wrap that call and persist the tail to
//! `<work_dir>/crashes/<stage>-<ts>-<session>.container.log` so investigators
//! can read it after the container has been removed.

use anyhow::{Context, Result};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::runtime::Runtime;

/// Default number of trailing log lines captured into a crash report.
pub const DEFAULT_TAIL: usize = 500;

/// Capture the trailing log output for a running or exited container.
///
/// Wraps `<runtime> logs --tail=N <name>`. Combined stdout+stderr is returned
/// because container runtimes route diagnostics to either stream depending on
/// the runtime and the entrypoint's behaviour. Best-effort: if the container
/// no longer exists (e.g. removed before capture) the returned string is
/// empty rather than an error — log capture must never block crash handling.
pub fn capture_logs(runtime: Runtime, name: &str, tail_lines: Option<usize>) -> Result<String> {
    if name.is_empty() {
        return Ok(String::new());
    }
    let tail_arg = format!("--tail={}", tail_lines.unwrap_or(DEFAULT_TAIL));
    let output = Command::new(runtime.binary())
        .args(["logs", &tail_arg, name])
        .output()
        .with_context(|| format!("Failed to invoke `{} logs` for {}", runtime.binary(), name))?;

    // If the container does not exist (already removed, never started), the
    // runtime returns a non-zero exit code. Treat that as "no log available"
    // rather than propagating the error.
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        if stderr.contains("no such")
            || stderr.contains("not found")
            || stderr.contains("does not exist")
        {
            return Ok(String::new());
        }
    }

    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(&stderr);
    }
    Ok(combined)
}

/// Persist a captured log tail to the crashes directory under `work_dir`.
///
/// File name: `<stage_id>-<timestamp>-<session_id>.container.log`. Timestamp
/// uses UTC YYYYMMDDTHHMMSSZ so files sort chronologically and don't collide
/// across rapid retries.
pub fn persist_log(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    content: &str,
) -> Result<PathBuf> {
    let crashes_dir = work_dir.join("crashes");
    std::fs::create_dir_all(&crashes_dir)
        .with_context(|| format!("Failed to create {}", crashes_dir.display()))?;
    let ts = Utc::now().format("%Y%m%dT%H%M%SZ");
    let path = crashes_dir.join(format!("{stage_id}-{ts}-{session_id}.container.log"));
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write container log to {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn capture_logs_empty_name_returns_empty() {
        // Empty container name must short-circuit without invoking the runtime.
        let out = capture_logs(Runtime::Docker, "", Some(5)).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn persist_log_writes_into_crashes_subdir() {
        let tmp = TempDir::new().unwrap();
        let path = persist_log(tmp.path(), "stage-x", "session-y", "hello world").unwrap();
        assert!(path.exists());
        assert!(path.parent().unwrap().ends_with(tmp.path().join("crashes")));
        let body = std::fs::read_to_string(&path).unwrap();
        assert_eq!(body, "hello world");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("stage-x-"));
        assert!(name.ends_with("-session-y.container.log"));
    }

    #[test]
    fn default_tail_is_five_hundred() {
        assert_eq!(DEFAULT_TAIL, 500);
    }
}
