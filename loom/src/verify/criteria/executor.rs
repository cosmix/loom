//! Low-level command execution for acceptance criteria

use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

use super::config::DEFAULT_COMMAND_TIMEOUT;
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
pub fn run_single_criterion_with_timeout(
    command: &str,
    working_dir: Option<&Path>,
    timeout: Duration,
) -> Result<CriterionResult> {
    let start = Instant::now();

    // Spawn the child process using the appropriate shell
    let mut child = spawn_shell_command(command, working_dir)?;

    // IMPORTANT: Start reading output BEFORE waiting for exit.
    // If we wait first, the child may block on write() when the pipe buffer
    // fills up (~64KB on Linux), causing a deadlock. We must drain the pipes
    // concurrently with waiting for the process to exit.
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    // Spawn threads to read stdout/stderr concurrently
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();

    if let Some(stdout) = stdout_handle {
        thread::spawn(move || {
            let result = read_stream_to_string(stdout);
            let _ = stdout_tx.send(result);
        });
    } else {
        let _ = stdout_tx.send(String::new());
    }

    if let Some(stderr) = stderr_handle {
        thread::spawn(move || {
            let result = read_stream_to_string(stderr);
            let _ = stderr_tx.send(result);
        });
    } else {
        let _ = stderr_tx.send(String::new());
    }

    // Now wait for completion with timeout
    let wait_result = child
        .wait_timeout(timeout)
        .with_context(|| format!("Failed to wait for command: {command}"))?;

    let duration = start.elapsed();

    // Collect output from reader threads (they should complete quickly after process exits)
    let stdout = stdout_rx
        .recv_timeout(OUTPUT_COLLECTION_TIMEOUT)
        .unwrap_or_else(|_| "[output collection timed out]".to_string());
    let stderr = stderr_rx
        .recv_timeout(OUTPUT_COLLECTION_TIMEOUT)
        .unwrap_or_else(|_| "[output collection timed out]".to_string());

    match wait_result {
        Some(status) => {
            // Command completed within timeout
            let success = status.success();
            let exit_code = status.code();

            Ok(CriterionResult::new(
                command.to_string(),
                success,
                stdout,
                stderr,
                exit_code,
                duration,
                false, // not timed out
            ))
        }
        None => {
            // Command timed out - kill the process
            kill_child_process(&mut child);

            Ok(CriterionResult::new(
                command.to_string(),
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

/// Spawn a shell command as a child process
///
/// Uses `sh -c` on Unix and `cmd /C` on Windows to execute the command.
/// The command string is passed as a single argument to avoid shell injection
/// through improper argument splitting.
pub(crate) fn spawn_shell_command(command: &str, working_dir: Option<&Path>) -> Result<Child> {
    let mut cmd = if cfg!(target_family = "unix") {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    } else {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(command);
        c
    };

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    cmd.spawn()
        .with_context(|| format!("Failed to spawn command: {command}"))
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

/// Terminate a child process
///
/// Attempts to kill the process. On Unix, this sends SIGKILL.
/// On Windows, this calls TerminateProcess.
fn kill_child_process(child: &mut Child) {
    // Attempt to kill - ignore errors since the process may have already exited
    let _ = child.kill();
    // Wait to reap the zombie process
    let _ = child.wait();
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
