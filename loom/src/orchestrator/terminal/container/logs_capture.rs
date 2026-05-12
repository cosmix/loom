//! Container log capture for crash reports and diagnostics.
//!
//! When a container session exits abnormally, `<runtime> logs` is the only
//! way to recover stderr/stdout from the entrypoint or firewall script.
//! These helpers wrap that call and persist the tail to
//! `<work_dir>/crashes/<stage>-<ts>-<session>.container.log` so investigators
//! can read it after the container has been removed.

use anyhow::{Context, Result};
use chrono::Utc;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::fs::safe_write::{
    safe_create_dir_all_in_workdir, safe_locked_write_in_workdir, safe_open_dirfd,
};

use super::runtime::Runtime;

/// Default number of trailing log lines captured into a crash report.
pub const DEFAULT_TAIL: usize = 500;

/// Hard ceiling on bytes persisted from a single capture. Anything larger is
/// truncated at the **start** so the most recent (and most diagnostically
/// useful) output survives. Matches the limit enforced by
/// [`crate::fs::safe_write::MAX_LOG_BYTES`].
pub const MAX_LOG_BYTES: usize = 4 * 1024 * 1024;

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
    Ok(truncate_to_tail(combined, MAX_LOG_BYTES))
}

/// Persist a captured log tail to the crashes directory under `work_dir`.
///
/// File name: `<stage_id>-<timestamp>-<session_id>.container.log`. Timestamp
/// uses UTC YYYYMMDDTHHMMSSZ so files sort chronologically and don't collide
/// across rapid retries. Written via the symlink-refusing `safe_write` family
/// so a planted symlink under `.work/crashes/` cannot redirect the write.
pub fn persist_log(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    content: &str,
) -> Result<PathBuf> {
    let dirfd = safe_open_dirfd(work_dir)
        .with_context(|| format!("Failed to open dirfd at {}", work_dir.display()))?;
    safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), Path::new("crashes"), 0o755)
        .with_context(|| format!("Failed to create crashes dir under {}", work_dir.display()))?;
    let ts = Utc::now().format("%Y%m%dT%H%M%SZ");
    let filename = format!("{stage_id}-{ts}-{session_id}.container.log");
    let relpath = PathBuf::from("crashes").join(&filename);
    let truncated = truncate_to_tail(content.to_string(), MAX_LOG_BYTES);
    safe_locked_write_in_workdir(dirfd.as_raw_fd(), &relpath, truncated.as_bytes())
        .with_context(|| format!("Failed to write container log to crashes/{filename}"))?;
    Ok(work_dir.join(&relpath))
}

/// Truncate `content` to at most `limit` bytes, dropping the prefix and
/// prepending a marker so callers know truncation occurred. Boundary is
/// shifted to the next char boundary so we never split a UTF-8 codepoint
/// (mistakes.md "String Handling: UTF-8 Truncation Panic").
fn truncate_to_tail(content: String, limit: usize) -> String {
    if content.len() <= limit {
        return content;
    }
    let marker = "[... truncated ...]\n";
    let keep = limit.saturating_sub(marker.len());
    let mut start = content.len() - keep;
    while start < content.len() && !content.is_char_boundary(start) {
        start += 1;
    }
    let mut out = String::with_capacity(marker.len() + content.len() - start);
    out.push_str(marker);
    out.push_str(&content[start..]);
    out
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

    #[test]
    fn persist_log_truncates_oversize_input() {
        let tmp = TempDir::new().unwrap();
        let huge = "A".repeat(MAX_LOG_BYTES + 1024);
        let path = persist_log(tmp.path(), "stage-x", "session-y", &huge).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.len() <= MAX_LOG_BYTES, "got {} bytes", body.len());
        assert!(body.starts_with("[... truncated ...]"));
    }

    #[test]
    fn truncate_to_tail_keeps_recent_bytes() {
        // The marker `"[... truncated ...]\n"` is 20 bytes; the limit must
        // leave room for both the marker AND some tail content, otherwise
        // `keep` collapses to zero and the function returns marker-only.
        // Real callers pass MAX_LOG_BYTES (4 MiB), so this is purely a
        // test-shape concern: pick a limit comfortably larger than the
        // marker.
        let s = format!("{}TAIL", "A".repeat(100));
        let out = truncate_to_tail(s, 32);
        assert!(out.ends_with("TAIL"), "got: {out:?}");
        assert!(out.starts_with("[... truncated ...]"));
    }

    #[test]
    fn truncate_to_tail_passthrough_small() {
        let s = "small".to_string();
        let out = truncate_to_tail(s.clone(), 100);
        assert_eq!(out, s);
    }
}
