//! Low-level command execution for acceptance criteria

use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

use super::config::DEFAULT_COMMAND_TIMEOUT;
use super::result::CriterionResult;

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

    // Wait for completion with timeout
    let wait_result = child
        .wait_timeout(timeout)
        .with_context(|| format!("Failed to wait for command: {command}"))?;

    let duration = start.elapsed();

    match wait_result {
        Some(status) => {
            // Command completed within timeout
            let (stdout, stderr) = collect_child_output(&mut child)?;
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

            // Collect any partial output that was captured
            let (stdout, stderr) = collect_child_output(&mut child).unwrap_or_default();

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

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    cmd.spawn()
        .with_context(|| format!("Failed to spawn command: {command}"))
}

/// Collect stdout and stderr from a child process
fn collect_child_output(child: &mut Child) -> Result<(String, String)> {
    let stdout = child
        .stdout
        .take()
        .map(|mut s| {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut s, &mut buf).ok();
            String::from_utf8_lossy(&buf).to_string()
        })
        .unwrap_or_default();

    let stderr = child
        .stderr
        .take()
        .map(|mut s| {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut s, &mut buf).ok();
            String::from_utf8_lossy(&buf).to_string()
        })
        .unwrap_or_default();

    Ok((stdout, stderr))
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
