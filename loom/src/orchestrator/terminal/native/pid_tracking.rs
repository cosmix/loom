//! Reliable native-session PID tracking and process discovery.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
const PROCESS_DISCOVERY_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(target_os = "macos")]
fn run_process_discovery_probe(
    program: &str,
    args: &[&str],
    operation: impl Into<String>,
) -> Option<std::process::Output> {
    let mut command = Command::new(program);
    command.args(args);
    crate::process::run_bounded_output(&mut command, PROCESS_DISCOVERY_PROBE_TIMEOUT, operation)
        .ok()
}

pub fn pids_dir(work_dir: &Path) -> PathBuf {
    work_dir.join("pids")
}

pub fn wrappers_dir(work_dir: &Path) -> PathBuf {
    work_dir.join("wrappers")
}

pub fn create_pid_dir(work_dir: &Path) -> Result<()> {
    let dir = pids_dir(work_dir);
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create pids directory: {}", dir.display()))
}

pub fn create_wrappers_dir(work_dir: &Path) -> Result<()> {
    let dir = wrappers_dir(work_dir);
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create wrappers directory: {}", dir.display()))
}

/// Get the path to a PID file for a tracking key.
///
/// `pid_key` is the per-session tracking key (`tracking_key` + `session.id`),
/// NOT a bare stage id — consecutive sessions for the same stage must not share
/// a PID file, or liveness for an old session would read the new session's PID.
pub fn pid_file_path(work_dir: &Path, pid_key: &str) -> PathBuf {
    pids_dir(work_dir).join(format!("{pid_key}.pid"))
}

/// Get the path to a wrapper script for a tracking key.
pub fn wrapper_script_path(work_dir: &Path, pid_key: &str) -> PathBuf {
    wrappers_dir(work_dir).join(format!("{pid_key}-wrapper.sh"))
}

pub type PidEntry = crate::process::ProcessIdentity;

/// Read the PID (and optional start-time) from a PID file.
///
/// File format written by the wrapper script:
/// - line 1: the PID (`echo $$`)
/// - line 2 (optional): the process start-time in clock ticks
///
/// Returns `None` if the file doesn't exist or the first line isn't a valid PID.
pub fn read_pid_entry(work_dir: &Path, pid_key: &str) -> Option<PidEntry> {
    let path = pid_file_path(work_dir, pid_key);
    let contents = fs::read_to_string(&path).ok()?;
    let mut lines = contents.lines();
    let pid: u32 = lines.next()?.trim().parse().ok()?;
    let start_time = lines.next().and_then(|s| s.trim().parse::<u64>().ok());
    Some(PidEntry { pid, start_time })
}

/// Crash-atomically persist the complete process identity observed by Loom.
pub fn write_pid_entry(work_dir: &Path, pid_key: &str, entry: PidEntry) -> Result<()> {
    create_pid_dir(work_dir)?;
    let mut contents = format!("{}\n", entry.pid);
    if let Some(start_time) = entry.start_time {
        contents.push_str(&format!("{start_time}\n"));
    }

    let path = pid_file_path(work_dir, pid_key);
    crate::fs::locking::locked_write(&path, &contents)
        .with_context(|| format!("Failed to persist PID identity: {}", path.display()))
}

pub fn remove_pid_file(work_dir: &Path, pid_key: &str) {
    let path = pid_file_path(work_dir, pid_key);
    let _ = fs::remove_file(path);
}

pub fn remove_wrapper_script(work_dir: &Path, pid_key: &str) {
    let path = wrapper_script_path(work_dir, pid_key);
    let _ = fs::remove_file(path);
}

pub fn cleanup_stage_files(work_dir: &Path, pid_key: &str) {
    remove_pid_file(work_dir, pid_key);
    remove_wrapper_script(work_dir, pid_key);
}

/// Discover the Claude process PID by scanning /proc (Linux)
///
/// Searches for a process whose `/proc/<pid>/environ` exports
/// `LOOM_SESSION_ID=<session_id>` (the marker the wrapper script sets) and
/// whose working directory matches `worktree_path`. The session-id marker is
/// the primary discriminator: it ensures we never latch onto a user's
/// interactive `claude` that happens to share the spawn directory (which for
/// merge/knowledge sessions is the repo root).
///
/// # Arguments
/// * `worktree_path` - The expected working directory of the Claude process
/// * `session_id` - The LOOM_SESSION_ID this session exported (required)
/// * `timeout` - Maximum time to wait for the process to appear
#[cfg(target_os = "linux")]
pub fn discover_claude_pid(
    worktree_path: &Path,
    session_id: &str,
    timeout: Duration,
) -> Option<u32> {
    let deadline = Instant::now() + timeout;
    let canonical_worktree = worktree_path.canonicalize().ok()?;

    while Instant::now() < deadline {
        if let Some(pid) = find_claude_process(&canonical_worktree, session_id) {
            return Some(pid);
        }
        thread::sleep(Duration::from_millis(100));
    }

    None
}

/// Read whether `/proc/<pid>/environ` exports `LOOM_SESSION_ID=<session_id>`.
///
/// `environ` is NUL-separated `KEY=VALUE` records. Returns `false` if the file
/// is unreadable (e.g. process gone or owned by another user).
#[cfg(target_os = "linux")]
fn process_has_session_marker(pid: u32, session_id: &str) -> bool {
    let needle = format!("LOOM_SESSION_ID={session_id}");
    match fs::read(format!("/proc/{pid}/environ")) {
        Ok(bytes) => bytes
            .split(|&b| b == 0)
            .any(|record| record == needle.as_bytes()),
        Err(_) => false,
    }
}

/// Find a Claude process matching this session's LOOM_SESSION_ID marker (Linux).
///
/// The session-id env marker is required; the working-directory match is a
/// secondary sanity check. This prevents `kill_session`'s discovery fallback
/// from ever targeting an unrelated interactive `claude` running at the repo
/// root.
#[cfg(target_os = "linux")]
fn find_claude_process(worktree_path: &Path, session_id: &str) -> Option<u32> {
    let proc_dir = Path::new("/proc");

    let entries = fs::read_dir(proc_dir).ok()?;

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let pid_str = file_name.to_string_lossy();

        // Skip non-numeric entries
        let pid: u32 = match pid_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Check if this is a claude process
        let cmdline_path = entry.path().join("cmdline");
        if let Ok(cmdline) = fs::read_to_string(&cmdline_path) {
            // cmdline uses null bytes as separators
            if !cmdline.contains("claude") {
                continue;
            }
        } else {
            continue;
        }

        // PRIMARY constraint: the process must carry this session's marker.
        if !process_has_session_marker(pid, session_id) {
            continue;
        }

        // Secondary sanity check: working directory matches the spawn dir.
        let cwd_link = entry.path().join("cwd");
        if let Ok(cwd) = fs::read_link(&cwd_link) {
            // Canonicalize cwd for comparison
            if let Ok(canonical_cwd) = cwd.canonicalize() {
                if canonical_cwd == worktree_path {
                    return Some(pid);
                }
            } else if cwd == worktree_path {
                return Some(pid);
            }
        }
    }

    None
}

/// Discover the Claude process PID using ps and lsof (macOS)
///
/// Searches for a `claude` process whose environment exports
/// `LOOM_SESSION_ID=<session_id>` (the marker the wrapper script sets) and
/// whose working directory matches `worktree_path`. The session-id marker is
/// the primary discriminator so we never latch onto a user's interactive
/// `claude` sharing the spawn directory.
///
/// # Arguments
/// * `worktree_path` - The expected working directory of the Claude process
/// * `session_id` - The LOOM_SESSION_ID this session exported (required)
/// * `timeout` - Maximum time to wait for the process to appear
#[cfg(target_os = "macos")]
pub fn discover_claude_pid(
    worktree_path: &Path,
    session_id: &str,
    timeout: Duration,
) -> Option<u32> {
    let deadline = Instant::now() + timeout;
    let canonical_worktree = worktree_path.canonicalize().ok()?;

    while Instant::now() < deadline {
        if let Some(pid) = find_claude_process(&canonical_worktree, session_id) {
            return Some(pid);
        }
        thread::sleep(Duration::from_millis(100));
    }

    None
}

/// Whether a process's environment (via `ps -Ewww`) exports
/// `LOOM_SESSION_ID=<session_id>` (macOS).
#[cfg(target_os = "macos")]
fn process_has_session_marker(pid: u32, session_id: &str) -> bool {
    let needle = format!("LOOM_SESSION_ID={session_id}");
    let pid_arg = pid.to_string();
    match run_process_discovery_probe(
        "ps",
        &["-E", "-ww", "-p", &pid_arg],
        format!("session environment probe for PID {pid}"),
    ) {
        Some(out) => String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .any(|tok| tok == needle),
        None => false,
    }
}

/// Find a Claude process matching this session's LOOM_SESSION_ID marker (macOS).
///
/// The session-id env marker is required; the working-directory match is a
/// secondary sanity check.
#[cfg(target_os = "macos")]
fn find_claude_process(worktree_path: &Path, session_id: &str) -> Option<u32> {
    // Run ps aux to list all processes
    let output = run_process_discovery_probe("ps", &["aux"], "Claude process listing")?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Filter for lines containing "claude"
    for line in stdout.lines() {
        if !line.contains("claude") {
            continue;
        }

        // Parse PID from second column
        // ps aux format: USER PID %CPU %MEM VSZ RSS TTY STAT START TIME COMMAND
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let pid: u32 = match parts[1].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // PRIMARY constraint: the process must carry this session's marker.
        if !process_has_session_marker(pid, session_id) {
            continue;
        }

        // Secondary sanity check: working directory matches the spawn dir.
        if let Some(cwd) = get_process_cwd_macos(pid) {
            if let Ok(canonical_cwd) = cwd.canonicalize() {
                if canonical_cwd == worktree_path {
                    return Some(pid);
                }
            } else if cwd == worktree_path {
                return Some(pid);
            }
        }
    }

    None
}

/// Get the current working directory of a process using lsof (macOS)
#[cfg(target_os = "macos")]
fn get_process_cwd_macos(pid: u32) -> Option<PathBuf> {
    let pid_arg = pid.to_string();
    let output = run_process_discovery_probe(
        "lsof",
        &["-p", &pid_arg],
        format!("process cwd probe for PID {pid}"),
    )?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("cwd") {
            // lsof output format: COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME
            // The NAME column is the path when FD is "cwd"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 9 {
                return Some(PathBuf::from(parts[8]));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cleanup_stage_files() {
        use crate::models::session::SessionType;

        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();
        let pid_key = "loom-test-stage-session-cleanup-1234567890";

        super::super::wrapper::create_wrapper_script(
            work_dir,
            pid_key,
            "test-stage",
            "session-cleanup-1234567890",
            "claude 'test'",
            None,
            SessionType::Stage,
        )
        .unwrap();

        assert!(wrapper_script_path(work_dir, pid_key).exists());
        cleanup_stage_files(work_dir, pid_key);
        assert!(!wrapper_script_path(work_dir, pid_key).exists());
    }

    /// `LOOM_MERGE_SESSION` follows the session KIND, not a `merge-` prefix on
    /// the stage id. The stage id handed to the wrapper is now the plain plan
    /// stage id for every kind, so a prefix sniff would both miss real merge

    #[test]
    fn test_read_pid_entry_pid_only() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();
        create_pid_dir(work_dir).unwrap();
        let pid_key = "loom-stage-session-1";
        fs::write(pid_file_path(work_dir, pid_key), "12345\n").unwrap();

        let entry = read_pid_entry(work_dir, pid_key).unwrap();
        assert_eq!(entry.pid, 12345);
        assert_eq!(entry.start_time, None);
    }

    #[test]
    fn test_read_pid_entry_with_start_time() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();
        create_pid_dir(work_dir).unwrap();
        let pid_key = "loom-stage-session-2";
        fs::write(pid_file_path(work_dir, pid_key), "999\n7777\n").unwrap();

        let entry = read_pid_entry(work_dir, pid_key).unwrap();
        assert_eq!(entry.pid, 999);
        assert_eq!(entry.start_time, Some(7777));
    }

    #[test]
    fn test_read_pid_entry_missing() {
        let temp_dir = TempDir::new().unwrap();
        assert!(read_pid_entry(temp_dir.path(), "no-such-key").is_none());
    }

    #[test]
    fn test_verify_pid_entry_dead_pid() {
        // A PID that does not exist must never match, regardless of start_time.
        let entry = PidEntry {
            pid: 999_999_999,
            start_time: Some(123),
        };
        assert_eq!(
            crate::process::verify_process_identity(entry),
            crate::process::IdentityStatus::Dead
        );
    }

    #[test]
    fn test_verify_pid_entry_live_no_start_time_is_unverifiable() {
        let entry = PidEntry {
            pid: std::process::id(),
            start_time: None,
        };
        assert_eq!(
            crate::process::verify_process_identity(entry),
            crate::process::IdentityStatus::Unverifiable
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_verify_pid_entry_start_time_mismatch_is_reuse() {
        // Our own PID is alive, but a bogus recorded start_time means the PID
        // was "recycled" — so the entry must NOT match.
        let entry = PidEntry {
            pid: std::process::id(),
            start_time: Some(1), // never the real start-time
        };
        assert_eq!(
            crate::process::verify_process_identity(entry),
            crate::process::IdentityStatus::Dead
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_process_start_time_self_then_matches() {
        // The real recorded start-time of our own process matches on probe.
        let pid = std::process::id();
        let start =
            crate::process::process_start_time(pid).expect("own start-time readable on Linux");
        let entry = PidEntry {
            pid,
            start_time: Some(start),
        };
        assert_eq!(
            crate::process::verify_process_identity(entry),
            crate::process::IdentityStatus::VerifiedAlive
        );
    }
}
