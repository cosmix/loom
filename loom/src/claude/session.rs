//! Foreground Claude session driver: spawn, marker-based completion
//! detection, and idle-session teardown. Shared by `loom pressure` and
//! `loom knowledge bootstrap`.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::Duration;

/// Environment variable enabling Claude Code's agent-teams feature.
pub(crate) const AGENT_TEAMS_ENV: &str = "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS";

/// What to do after a child process exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitAction {
    /// Exit 0 — proceed to the next step.
    Continue,
    /// User interrupt (130/2) or signal-killed child (no code) — abort cleanly.
    Abort,
    /// Other non-zero — warn and continue.
    Warn,
}

/// Outcome of a foreground Claude step.
#[derive(Debug)]
pub(crate) enum ClaudeOutcome {
    /// The agent signalled completion (the marker appeared) and the driver
    /// terminated the idle session. Always treated as success.
    Completed,
    /// The process exited on its own — the user exited manually (typically
    /// code 0) or Claude crashed/was interrupted. Classified via [`ExitAction`].
    Exited(ExitStatus),
}

/// How often to poll for the completion marker / child exit.
const POLL_INTERVAL_MS: u64 = 300;
/// Grace period after SIGTERM before escalating to SIGKILL.
const TERM_GRACE_MS: u64 = 4000;

/// Classify a finished child process for pipeline control.
pub(crate) fn classify_exit(status: ExitStatus) -> ExitAction {
    classify_code(status.code())
}

/// Pure classification of a child exit code (`None` = killed by a signal).
fn classify_code(code: Option<i32>) -> ExitAction {
    match code {
        Some(0) => ExitAction::Continue,
        // Ctrl+C (130/2) or signal-killed (no code) → abort the whole pipeline.
        None | Some(130) | Some(2) => ExitAction::Abort,
        Some(_) => ExitAction::Warn,
    }
}

/// Send SIGTERM to a process, ignoring "already gone".
fn send_sigterm(pid: u32) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
}

/// Ensure the marker's parent directory exists so neither clearing the stale
/// marker nor the session's completion touch can fail on a missing directory.
fn ensure_marker_dir(marker: &Path) -> Result<()> {
    match marker.parent() {
        Some(parent) => std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create marker dir {}", parent.display())),
        None => Ok(()),
    }
}

/// Delete a file, treating "not found" as success.
pub(crate) fn remove_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("failed to delete {}", path.display())),
    }
}

/// `Command::spawn` briefly, and rarely, fails with `ETXTBSY` ("text file
/// busy") when the target binary is still settling right after being
/// written — e.g. a fork/exec race right after a test writes a fake `claude`
/// script. A handful of short retries clears it without treating it as a
/// real spawn failure.
fn spawn_retrying_text_busy(command: &mut Command) -> std::io::Result<Child> {
    const MAX_ATTEMPTS: u32 = 5;
    const RETRY_DELAY: Duration = Duration::from_millis(20);

    for attempt in 1..=MAX_ATTEMPTS {
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(e) if attempt < MAX_ATTEMPTS && e.raw_os_error() == Some(libc::ETXTBSY) => {
                thread::sleep(RETRY_DELAY);
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("loop always returns on its final attempt")
}

/// Spawn `claude_path` with `args` in the foreground (inherited stdin/stdout/stderr,
/// `AGENT_TEAMS_ENV=1`, cwd = `cwd`), clear any stale `marker` first (creating its
/// parent dir), and return `Completed` once `marker` appears (after SIGTERM then
/// SIGKILL teardown). If the child has already exited, return `Completed` when the
/// marker exists and `Exited(status)` otherwise. Deletes the marker before returning.
pub(crate) fn run_foreground(
    claude_path: &Path,
    cwd: &Path,
    args: &[String],
    marker: &Path,
) -> Result<ClaudeOutcome> {
    // Clear any stale marker from a previous step before spawning. The
    // marker's parent dir may not exist yet; this driver runs unsandboxed,
    // so it can create it.
    ensure_marker_dir(marker)?;
    remove_if_exists(marker)?;

    let mut cmd = Command::new(claude_path);
    cmd.args(args);
    cmd.env(AGENT_TEAMS_ENV, "1");
    cmd.current_dir(cwd);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    let mut child = spawn_retrying_text_busy(&mut cmd).context("failed to spawn claude")?;

    let outcome = loop {
        // The agent exited on its own (manual exit, crash, or Ctrl-C) — but
        // the marker wins over the exit code if it is already on disk,
        // because touching it is the session's final action.
        if let Some(status) = child.try_wait().context("failed to poll claude")? {
            break if marker.exists() {
                ClaudeOutcome::Completed
            } else {
                ClaudeOutcome::Exited(status)
            };
        }
        // The agent signalled completion → terminate the idle session.
        if marker.exists() {
            terminate_idle_session(&mut child)?;
            break ClaudeOutcome::Completed;
        }
        thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    };

    remove_if_exists(marker)?;
    Ok(outcome)
}

/// Terminate an idle child: SIGTERM, then poll for up to [`TERM_GRACE_MS`] for
/// it to exit on its own, falling back to SIGKILL — mirroring how the loom
/// daemon terminates a session whose stage has completed. Split out of
/// [`run_foreground`] purely to keep that function under the maintainability
/// line limit.
fn terminate_idle_session(child: &mut Child) -> Result<()> {
    send_sigterm(child.id());
    let grace_polls = TERM_GRACE_MS / POLL_INTERVAL_MS;
    let mut reaped = false;
    for _ in 0..grace_polls {
        thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        if child.try_wait().context("failed to poll claude")?.is_some() {
            reaped = true;
            break;
        }
    }
    if !reaped {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn run_foreground_completes_when_marker_appears() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("marker.done");
        let args = vec![
            "-c".to_string(),
            "touch \"$0\"; exec sleep 30".to_string(),
            marker.to_string_lossy().into_owned(),
        ];
        let start = std::time::Instant::now();
        let outcome = run_foreground(Path::new("/bin/sh"), temp.path(), &args, &marker).unwrap();
        assert!(matches!(outcome, ClaudeOutcome::Completed));
        assert!(!marker.exists());
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn run_foreground_reports_self_exit() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("marker.done");
        let args = vec!["-c".to_string(), "exit 3".to_string()];
        let outcome = run_foreground(Path::new("/bin/sh"), temp.path(), &args, &marker).unwrap();
        match outcome {
            ClaudeOutcome::Exited(status) => assert_eq!(status.code(), Some(3)),
            ClaudeOutcome::Completed => panic!("expected Exited, got Completed"),
        }
    }

    #[test]
    fn run_foreground_clears_stale_marker() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("marker.done");
        std::fs::write(&marker, "stale").unwrap();
        let args = vec!["-c".to_string(), "exit 0".to_string()];
        let outcome = run_foreground(Path::new("/bin/sh"), temp.path(), &args, &marker).unwrap();
        match outcome {
            ClaudeOutcome::Exited(status) => assert_eq!(status.code(), Some(0)),
            ClaudeOutcome::Completed => {
                panic!("stale marker should have been cleared before spawn")
            }
        }
    }

    #[test]
    fn test_ensure_marker_dir_creates_parent_and_is_idempotent() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let marker = root
            .join(".loom")
            .join("work")
            .join("pressure")
            .join("claude-1.done");
        assert!(!marker.parent().unwrap().exists());

        ensure_marker_dir(&marker).unwrap();
        assert!(marker.parent().unwrap().is_dir());

        // Idempotent: calling again on an already-existing dir is still Ok.
        ensure_marker_dir(&marker).unwrap();
        assert!(marker.parent().unwrap().is_dir());
    }

    #[test]
    fn test_classify_code_all_arms() {
        assert_eq!(classify_code(Some(0)), ExitAction::Continue);
        assert_eq!(classify_code(Some(130)), ExitAction::Abort);
        assert_eq!(classify_code(Some(2)), ExitAction::Abort);
        assert_eq!(classify_code(None), ExitAction::Abort); // signal-killed
        assert_eq!(classify_code(Some(1)), ExitAction::Warn);
        assert_eq!(classify_code(Some(42)), ExitAction::Warn);
    }

    #[test]
    fn run_foreground_sets_cwd_and_agent_teams_env() {
        let temp = TempDir::new().unwrap();
        let cwd = temp.path().canonicalize().unwrap();
        let marker = cwd.join("marker.done");
        let args = vec![
            "-c".to_string(),
            "pwd > \"$0.env\"; printenv CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS >> \"$0.env\"; \
             touch \"$0\"; exec sleep 30"
                .to_string(),
            marker.to_string_lossy().into_owned(),
        ];
        let outcome = run_foreground(Path::new("/bin/sh"), &cwd, &args, &marker).unwrap();
        assert!(matches!(outcome, ClaudeOutcome::Completed));

        let env_path = PathBuf::from(format!("{}.env", marker.display()));
        let contents = std::fs::read_to_string(&env_path).unwrap();
        let mut lines = contents.lines();
        assert_eq!(lines.next(), Some(cwd.to_string_lossy().as_ref()));
        assert_eq!(lines.next(), Some("1"));
    }

    #[test]
    fn run_foreground_marker_then_exit_is_completed() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("marker.done");
        let args = vec![
            "-c".to_string(),
            "touch \"$0\"; exit 0".to_string(),
            marker.to_string_lossy().into_owned(),
        ];
        let outcome = run_foreground(Path::new("/bin/sh"), temp.path(), &args, &marker).unwrap();
        assert!(matches!(outcome, ClaudeOutcome::Completed));
        assert!(!marker.exists());
    }

    #[test]
    fn run_foreground_marker_then_nonzero_exit_is_completed() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("marker.done");
        let args = vec![
            "-c".to_string(),
            "touch \"$0\"; exit 3".to_string(),
            marker.to_string_lossy().into_owned(),
        ];
        let outcome = run_foreground(Path::new("/bin/sh"), temp.path(), &args, &marker).unwrap();
        assert!(matches!(outcome, ClaudeOutcome::Completed));
        assert!(!marker.exists());
    }
}
