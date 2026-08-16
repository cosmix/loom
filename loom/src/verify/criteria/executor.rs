//! Low-level command execution for acceptance criteria

use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;
use std::process::Child;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

use crate::models::stage::CommandConfinement;

use super::config::DEFAULT_COMMAND_TIMEOUT;
use super::confine::{self, CommandSpec};
use super::result::CriterionResult;

/// Timeout for collecting output from child process pipes
const OUTPUT_COLLECTION_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum output size for acceptance criteria commands (10MB)
const MAX_OUTPUT_SIZE: usize = 10 * 1024 * 1024;

/// Run a single acceptance criterion (shell command) with default timeout
///
/// This is a convenience wrapper around `run_single_criterion_with_timeout` that uses
/// the default timeout setting.
pub fn run_single_criterion(command: &str, working_dir: Option<&Path>) -> Result<CriterionResult> {
    run_single_criterion_with_timeout(command, working_dir, DEFAULT_COMMAND_TIMEOUT)
}

/// Run a single acceptance criterion (shell command) with specified timeout
///
/// Executes the command using the system shell and captures all output.
/// Returns a CriterionResult with execution details.
///
/// If `working_dir` is provided, the command will be executed in that directory.
///
/// The command will be terminated if it exceeds the specified `timeout` duration.
/// When this happens, the result will have `timed_out` set to true and `success`
/// set to false.
///
/// The command runs at the default confinement level
/// ([`CommandConfinement::Confined`]); callers that know the stage's resolved
/// level use [`run_spec_with_timeout`] instead.
pub fn run_single_criterion_with_timeout(
    command: &str,
    working_dir: Option<&Path>,
    timeout: Duration,
) -> Result<CriterionResult> {
    run_spec_with_timeout(
        &CommandSpec::shell(command),
        working_dir,
        timeout,
        CommandConfinement::default(),
    )
}

/// Run one command spec with a timeout and a confinement level.
///
/// This is the single implementation of the run loop: spawn, drain both pipes
/// concurrently, wait with a timeout, and kill the whole process group if the
/// deadline passes. Everything else in this module funnels into it.
pub fn run_spec_with_timeout(
    spec: &CommandSpec,
    working_dir: Option<&Path>,
    timeout: Duration,
    confinement: CommandConfinement,
) -> Result<CriterionResult> {
    let start = Instant::now();

    let mut child = confine::spawn_confined(spec, working_dir, confinement)?;
    let output = OutputReaders::spawn(&mut child);

    let wait_result = child
        .wait_timeout(timeout)
        .with_context(|| format!("Failed to wait for command: {spec}"))?;

    let duration = start.elapsed();
    let (stdout, stderr) = output.collect();

    match wait_result {
        // Command completed within timeout
        Some(status) => Ok(CriterionResult::new(
            spec.to_string(),
            status.success(),
            stdout,
            stderr,
            status.code(),
            duration,
            false, // not timed out
        )),
        None => {
            // Command timed out - kill the process
            kill_child_process(&mut child);

            Ok(CriterionResult::new(
                spec.to_string(),
                false, // failed due to timeout
                stdout,
                format!(
                    "{}\n[Process killed after {}s timeout]",
                    stderr,
                    timeout.as_secs()
                ),
                None, // no exit code for killed process
                duration,
                true, // timed out
            ))
        }
    }
}

/// Concurrent readers draining a child's piped stdout and stderr.
///
/// IMPORTANT: the pipes must be drained BEFORE waiting for exit. If we wait
/// first, the child may block on write() once the pipe buffer fills up (~64KB
/// on Linux) and never exit, deadlocking the wait.
struct OutputReaders {
    stdout: mpsc::Receiver<String>,
    stderr: mpsc::Receiver<String>,
}

impl OutputReaders {
    /// Take both pipes off `child` and start draining them on their own threads.
    fn spawn(child: &mut Child) -> Self {
        Self {
            stdout: spawn_reader(child.stdout.take()),
            stderr: spawn_reader(child.stderr.take()),
        }
    }

    /// Collect (stdout, stderr). The reader threads should finish promptly once
    /// the process exits; a stalled one yields a marker rather than hanging.
    fn collect(self) -> (String, String) {
        (collect_stream(self.stdout), collect_stream(self.stderr))
    }
}

fn spawn_reader<R: Read + Send + 'static>(stream: Option<R>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    match stream {
        Some(stream) => {
            thread::spawn(move || {
                let _ = tx.send(read_stream_to_string(stream));
            });
        }
        None => {
            let _ = tx.send(String::new());
        }
    }
    rx
}

fn collect_stream(stream: mpsc::Receiver<String>) -> String {
    stream
        .recv_timeout(OUTPUT_COLLECTION_TIMEOUT)
        .unwrap_or_else(|_| "[output collection timed out]".to_string())
}

/// Read a stream to string, handling errors gracefully
///
/// Reads output in chunks with a maximum size limit to prevent OOM attacks.
/// If output exceeds MAX_OUTPUT_SIZE, the remaining data is discarded and
/// a truncation message is appended.
fn read_stream_to_string<R: Read>(mut stream: R) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];

    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break, // EOF
            Ok(n) => {
                let remaining = MAX_OUTPUT_SIZE.saturating_sub(buf.len());
                if remaining == 0 {
                    // Already at limit, discard remaining data but drain the stream
                    // to prevent broken pipe errors
                    let mut discard = [0u8; 8192];
                    while stream.read(&mut discard).unwrap_or(0) > 0 {}
                    buf.extend_from_slice(b"\n[output truncated at 10MB]");
                    break;
                }
                let to_copy = n.min(remaining);
                buf.extend_from_slice(&chunk[..to_copy]);
                if to_copy < n {
                    // Hit the limit mid-chunk
                    let mut discard = [0u8; 8192];
                    while stream.read(&mut discard).unwrap_or(0) > 0 {}
                    buf.extend_from_slice(b"\n[output truncated at 10MB]");
                    break;
                }
            }
            Err(_) => {
                if buf.is_empty() {
                    return "[error reading output]".to_string();
                }
                break;
            }
        }
    }

    String::from_utf8_lossy(&buf).to_string()
}

/// Terminate a child process and its entire process group.
///
/// On Unix, sends SIGKILL to the negative PID (the process group) so that
/// grandchildren spawned by compound commands (e.g. `cargo test` in `a && b`)
/// are also killed. Falls back to killing only the direct child if the group
/// kill fails (e.g. the child already exited).
///
/// On Windows, calls TerminateProcess on the direct child.
fn kill_child_process(child: &mut Child) {
    #[cfg(unix)]
    {
        // Kill the entire process group — the child was spawned with
        // setpgid(0,0) so its pgid equals its pid. Negative pid targets
        // the whole group.
        if let Ok(pid) = i32::try_from(child.id()) {
            let pgid = nix::unistd::Pid::from_raw(-pid);
            let _ = nix::sys::signal::kill(pgid, nix::sys::signal::Signal::SIGKILL);
        }
        // Also kill the direct child (covers the edge case where setpgid
        // raced with the child exec-ing into a new pgid).
        let _ = child.kill();
        let _ = child.wait();
    }

    #[cfg(not(unix))]
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_stream_small_input() {
        let data = b"hello world";
        let result = read_stream_to_string(Cursor::new(data));
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_read_stream_empty_input() {
        let data: &[u8] = b"";
        let result = read_stream_to_string(Cursor::new(data));
        assert_eq!(result, "");
    }

    #[test]
    fn test_read_stream_truncates_at_limit() {
        // Create data larger than MAX_OUTPUT_SIZE
        let data = vec![b'x'; MAX_OUTPUT_SIZE + 1000];
        let result = read_stream_to_string(Cursor::new(data));

        // Should contain the truncation message
        assert!(result.contains("[output truncated at 10MB]"));

        // Should not exceed MAX_OUTPUT_SIZE + truncation message length
        assert!(result.len() <= MAX_OUTPUT_SIZE + 50);
    }

    #[test]
    fn test_read_stream_exact_limit() {
        // Data exactly at the limit should NOT be truncated
        let data = vec![b'y'; MAX_OUTPUT_SIZE];
        let result = read_stream_to_string(Cursor::new(data));
        assert!(!result.contains("[output truncated"));
        assert_eq!(result.len(), MAX_OUTPUT_SIZE);
    }

    #[test]
    fn test_max_output_size_is_10mb() {
        assert_eq!(MAX_OUTPUT_SIZE, 10 * 1024 * 1024);
    }
}
