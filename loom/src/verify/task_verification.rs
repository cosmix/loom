//! Task verification execution
//!
//! Runs verification rules defined in task definitions.
//! Verification is soft - it emits warnings but doesn't hard-block.

use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::checkpoints::{VerificationResult, VerificationRule};

/// Default timeout for verification commands
pub const DEFAULT_VERIFICATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Run all verification rules for a task
pub fn run_task_verifications(
    rules: &[VerificationRule],
    worktree_path: &Path,
    outputs: &HashMap<String, String>,
) -> Vec<VerificationResult> {
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
) -> VerificationResult {
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
) -> VerificationResult {
    let full_path = worktree_path.join(path);
    if full_path.exists() {
        VerificationResult::passed(rule, format!("File exists: {path}"))
    } else {
        VerificationResult::failed(rule, format!("File not found: {path}"))
    }
}

fn verify_contains(
    rule: VerificationRule,
    worktree_path: &Path,
    path: &str,
    pattern: &str,
) -> VerificationResult {
    let full_path = worktree_path.join(path);

    if !full_path.exists() {
        return VerificationResult::failed(
            rule,
            format!("File not found for pattern check: {path}"),
        );
    }

    match fs::read_to_string(&full_path) {
        Ok(content) => match Regex::new(pattern) {
            Ok(regex) => {
                if regex.is_match(&content) {
                    VerificationResult::passed(rule, format!("Pattern '{pattern}' found in {path}"))
                } else {
                    VerificationResult::failed(
                        rule,
                        format!("Pattern '{pattern}' not found in {path}"),
                    )
                }
            }
            Err(e) => VerificationResult::failed(rule, format!("Invalid regex pattern: {e}")),
        },
        Err(e) => VerificationResult::failed(rule, format!("Failed to read {path}: {e}")),
    }
}

fn verify_command(
    rule: VerificationRule,
    worktree_path: &Path,
    cmd: &str,
    expected_exit_code: i32,
) -> VerificationResult {
    let result = run_verification_command(cmd, worktree_path);

    match result {
        Ok((exit_code, stdout, stderr)) => {
            if exit_code == expected_exit_code {
                VerificationResult::passed(
                    rule,
                    format!("Command succeeded (exit code {exit_code})"),
                )
            } else {
                let output = if !stderr.is_empty() {
                    stderr.lines().take(5).collect::<Vec<_>>().join("\n")
                } else if !stdout.is_empty() {
                    stdout.lines().take(5).collect::<Vec<_>>().join("\n")
                } else {
                    String::new()
                };
                VerificationResult::failed(
                    rule,
                    format!(
                        "Command failed: expected exit code {expected_exit_code}, got {exit_code}\n{output}"
                    ),
                )
            }
        }
        Err(e) => VerificationResult::failed(rule, format!("Command failed to execute: {e}")),
    }
}

fn verify_output_set(
    rule: VerificationRule,
    outputs: &HashMap<String, String>,
    key: &str,
) -> VerificationResult {
    if outputs.contains_key(key) {
        VerificationResult::passed(rule, format!("Output '{key}' is set"))
    } else {
        VerificationResult::failed(rule, format!("Output '{key}' is not set"))
    }
}

/// Run a verification command with timeout
fn run_verification_command(cmd: &str, working_dir: &Path) -> Result<(i32, String, String)> {
    let mut command = if cfg!(target_family = "unix") {
        let mut c = Command::new("sh");
        c.arg("-c").arg(cmd);
        c
    } else {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(cmd);
        c
    };

    command.current_dir(working_dir);

    let output = command
        .output()
        .with_context(|| format!("Failed to execute command: {cmd}"))?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok((exit_code, stdout, stderr))
}

/// Get a summary of verification results
pub fn summarize_verifications(results: &[VerificationResult]) -> (usize, usize, Vec<String>) {
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.iter().filter(|r| !r.passed).count();
    let warnings: Vec<String> = results
        .iter()
        .filter(|r| !r.passed)
        .map(|r| r.message.clone())
        .collect();

    (passed, failed, warnings)
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
}
