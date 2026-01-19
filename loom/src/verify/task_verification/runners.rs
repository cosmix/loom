//! Verification execution logic

use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use wait_timeout::ChildExt;

use crate::checkpoints::{CheckpointVerificationResult, VerificationRule};

use super::types::{
    DEFAULT_VERIFICATION_TIMEOUT, MAX_ERROR_OUTPUT_LINES, OUTPUT_COLLECTION_TIMEOUT,
};

/// Run all verification rules for a task
pub fn run_task_verifications(
    rules: &[VerificationRule],
    worktree_path: &Path,
    outputs: &HashMap<String, String>,
) -> Vec<CheckpointVerificationResult> {
    rules
        .iter()
        .map(|rule| run_single_verification(rule, worktree_path, outputs))
        .collect()
}

/// Run a single verification rule
pub fn run_single_verification(
    rule: &VerificationRule,
    worktree_path: &Path,
    outputs: &HashMap<String, String>,
) -> CheckpointVerificationResult {
    match rule {
        VerificationRule::FileExists { path } => {
            verify_file_exists(rule.clone(), worktree_path, path)
        }
        VerificationRule::Contains { path, pattern } => {
            verify_contains(rule.clone(), worktree_path, path, pattern)
        }
        VerificationRule::Command {
            cmd,
            expected_exit_code,
        } => verify_command(rule.clone(), worktree_path, cmd, *expected_exit_code),
        VerificationRule::OutputSet { key } => verify_output_set(rule.clone(), outputs, key),
    }
}

fn verify_file_exists(
    rule: VerificationRule,
    worktree_path: &Path,
    path: &str,
) -> CheckpointVerificationResult {
    let full_path = worktree_path.join(path);
    if full_path.exists() {
        CheckpointVerificationResult::passed(rule, format!("File exists: {path}"))
    } else {
        CheckpointVerificationResult::failed(rule, format!("File not found: {path}"))
    }
}

fn verify_contains(
    rule: VerificationRule,
    worktree_path: &Path,
    path: &str,
    pattern: &str,
) -> CheckpointVerificationResult {
    let full_path = worktree_path.join(path);

    if !full_path.exists() {
        return CheckpointVerificationResult::failed(
            rule,
            format!("File not found for pattern check: {path}"),
        );
    }

    match fs::read_to_string(&full_path) {
        Ok(content) => match Regex::new(pattern) {
            Ok(regex) => {
                if regex.is_match(&content) {
                    CheckpointVerificationResult::passed(
                        rule,
                        format!("Pattern '{pattern}' found in {path}"),
                    )
                } else {
                    CheckpointVerificationResult::failed(
                        rule,
                        format!("Pattern '{pattern}' not found in {path}"),
                    )
                }
            }
            Err(e) => {
                CheckpointVerificationResult::failed(rule, format!("Invalid regex pattern: {e}"))
            }
        },
        Err(e) => CheckpointVerificationResult::failed(rule, format!("Failed to read {path}: {e}")),
    }
}

fn verify_command(
    rule: VerificationRule,
    worktree_path: &Path,
    cmd: &str,
    expected_exit_code: i32,
) -> CheckpointVerificationResult {
    let result = run_verification_command(cmd, worktree_path);

    match result {
        Ok((exit_code, stdout, stderr)) => {
            if exit_code == expected_exit_code {
                CheckpointVerificationResult::passed(
                    rule,
                    format!("Command succeeded (exit code {exit_code})"),
                )
            } else {
                let output = if !stderr.is_empty() {
                    stderr
                        .lines()
                        .take(MAX_ERROR_OUTPUT_LINES)
                        .collect::<Vec<_>>()
                        .join("\n")
                } else if !stdout.is_empty() {
                    stdout
                        .lines()
                        .take(MAX_ERROR_OUTPUT_LINES)
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    String::new()
                };
                CheckpointVerificationResult::failed(
                    rule,
                    format!(
                        "Command failed: expected exit code {expected_exit_code}, got {exit_code}\n{output}"
                    ),
                )
            }
        }
        Err(e) => {
            CheckpointVerificationResult::failed(rule, format!("Command failed to execute: {e}"))
        }
    }
}

fn verify_output_set(
    rule: VerificationRule,
    outputs: &HashMap<String, String>,
    key: &str,
) -> CheckpointVerificationResult {
    if outputs.contains_key(key) {
        CheckpointVerificationResult::passed(rule, format!("Output '{key}' is set"))
    } else {
        CheckpointVerificationResult::failed(rule, format!("Output '{key}' is not set"))
    }
}

/// Run a verification command with timeout
fn run_verification_command(cmd: &str, working_dir: &Path) -> Result<(i32, String, String)> {
    // Spawn the child process using the appropriate shell
    let mut child = if cfg!(target_family = "unix") {
        let mut c = Command::new("sh");
        c.arg("-c").arg(cmd);
        c.current_dir(working_dir);
        c.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c.spawn()
            .with_context(|| format!("Failed to spawn command: {cmd}"))?
    } else {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(cmd);
        c.current_dir(working_dir);
        c.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c.spawn()
            .with_context(|| format!("Failed to spawn command: {cmd}"))?
    };

    // Wait for completion with timeout
    let wait_result = child
        .wait_timeout(DEFAULT_VERIFICATION_TIMEOUT)
        .with_context(|| format!("Failed to wait for command: {cmd}"))?;

    match wait_result {
        Some(status) => {
            // Command completed within timeout
            let exit_code = status.code().unwrap_or(-1);
            let (stdout, stderr) = collect_output(&mut child)?;
            Ok((exit_code, stdout, stderr))
        }
        None => {
            // Command timed out - kill the process
            let _ = child.kill();
            let _ = child.wait();

            // Collect any partial output that was captured
            let (stdout, stderr) = collect_output(&mut child).unwrap_or_default();

            // Return error with timeout information
            Err(anyhow::anyhow!(
                "Command timed out after {}s\nPartial stdout: {}\nPartial stderr: {}",
                DEFAULT_VERIFICATION_TIMEOUT.as_secs(),
                stdout,
                stderr
            ))
        }
    }
}

/// Collect stdout and stderr from a child process with timeout protection
///
/// Spawns separate threads for reading stdout and stderr to avoid blocking.
/// If reads don't complete within the timeout, returns partial output collected so far.
fn collect_output(child: &mut std::process::Child) -> Result<(String, String)> {
    collect_output_with_timeout(child, OUTPUT_COLLECTION_TIMEOUT)
}

/// Collect stdout and stderr with a specified timeout
fn collect_output_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<(String, String)> {
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    // Channel for stdout
    let (stdout_tx, stdout_rx) = mpsc::channel();
    // Channel for stderr
    let (stderr_tx, stderr_rx) = mpsc::channel();

    // Spawn thread to read stdout
    if let Some(stdout) = stdout_handle {
        thread::spawn(move || {
            let result = read_stream_to_string(stdout);
            let _ = stdout_tx.send(result);
        });
    } else {
        let _ = stdout_tx.send(String::new());
    }

    // Spawn thread to read stderr
    if let Some(stderr) = stderr_handle {
        thread::spawn(move || {
            let result = read_stream_to_string(stderr);
            let _ = stderr_tx.send(result);
        });
    } else {
        let _ = stderr_tx.send(String::new());
    }

    // Wait for both with timeout
    let stdout = stdout_rx
        .recv_timeout(timeout)
        .unwrap_or_else(|_| "[output collection timed out]".to_string());

    let stderr = stderr_rx
        .recv_timeout(timeout)
        .unwrap_or_else(|_| "[output collection timed out]".to_string());

    Ok((stdout, stderr))
}

/// Read a stream to string, handling errors gracefully
fn read_stream_to_string<R: Read>(mut stream: R) -> String {
    let mut buf = Vec::new();
    match stream.read_to_end(&mut buf) {
        Ok(_) => String::from_utf8_lossy(&buf).to_string(),
        Err(_) => "[error reading output]".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_file_exists_verification() {
        let temp = TempDir::new().unwrap();
        let worktree = temp.path();

        // Create a test file
        fs::write(worktree.join("test.txt"), "content").unwrap();

        let rule = VerificationRule::FileExists {
            path: "test.txt".to_string(),
        };

        let result = run_single_verification(&rule, worktree, &HashMap::new());
        assert!(result.passed);

        let rule_missing = VerificationRule::FileExists {
            path: "missing.txt".to_string(),
        };

        let result_missing = run_single_verification(&rule_missing, worktree, &HashMap::new());
        assert!(!result_missing.passed);
    }

    #[test]
    fn test_contains_verification() {
        let temp = TempDir::new().unwrap();
        let worktree = temp.path();

        fs::write(worktree.join("test.txt"), "hello world").unwrap();

        let rule = VerificationRule::Contains {
            path: "test.txt".to_string(),
            pattern: r"hello\s+world".to_string(),
        };

        let result = run_single_verification(&rule, worktree, &HashMap::new());
        assert!(result.passed);

        let rule_missing = VerificationRule::Contains {
            path: "test.txt".to_string(),
            pattern: r"goodbye".to_string(),
        };

        let result_missing = run_single_verification(&rule_missing, worktree, &HashMap::new());
        assert!(!result_missing.passed);
    }

    #[test]
    fn test_command_verification() {
        let temp = TempDir::new().unwrap();
        let worktree = temp.path();

        let rule = VerificationRule::Command {
            cmd: "true".to_string(),
            expected_exit_code: 0,
        };

        let result = run_single_verification(&rule, worktree, &HashMap::new());
        assert!(result.passed);

        let rule_fail = VerificationRule::Command {
            cmd: "false".to_string(),
            expected_exit_code: 0,
        };

        let result_fail = run_single_verification(&rule_fail, worktree, &HashMap::new());
        assert!(!result_fail.passed);
    }

    #[test]
    fn test_output_set_verification() {
        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), "value".to_string());

        let rule = VerificationRule::OutputSet {
            key: "result".to_string(),
        };

        let result = run_single_verification(&rule, Path::new("."), &outputs);
        assert!(result.passed);

        let rule_missing = VerificationRule::OutputSet {
            key: "missing".to_string(),
        };

        let result_missing = run_single_verification(&rule_missing, Path::new("."), &outputs);
        assert!(!result_missing.passed);
    }

    #[test]
    fn test_command_timeout() {
        let temp = TempDir::new().unwrap();
        let worktree = temp.path();

        // Create a command that sleeps longer than the timeout (30 seconds)
        // Using a 35 second sleep to exceed the DEFAULT_VERIFICATION_TIMEOUT
        let rule = VerificationRule::Command {
            cmd: "sleep 35".to_string(),
            expected_exit_code: 0,
        };

        let result = run_single_verification(&rule, worktree, &HashMap::new());

        // The command should fail due to timeout
        assert!(!result.passed);
        assert!(result.message.contains("timed out") || result.message.contains("timeout"));
    }
}
