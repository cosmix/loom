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
fn read_stream_to_string<R: Read>(mut stream: R) -> String {
    let mut buf = Vec::new();
    match stream.read_to_end(&mut buf) {
        Ok(_) => String::from_utf8_lossy(&buf).to_string(),
        Err(_) => "[error reading output]".to_string(),
    }
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
