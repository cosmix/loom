//! Integration tests for hook commit message filtering
//!
//! Tests verify that the commit-filter hook correctly blocks commits
//! containing Claude co-authorship attribution per CLAUDE.md rule 8.
//!
//! These tests run the hook script directly with bash - no loom invocation.

use loom::fs::permissions::constants::HOOK_COMMIT_FILTER;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use tempfile::TempDir;

/// Install hook script to temp directory and return path
fn setup_hook() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().expect("create temp dir");
    let hook_path = temp.path().join("commit-filter.sh");
    fs::write(&hook_path, HOOK_COMMIT_FILTER).expect("write hook");

    let mut perms = fs::metadata(&hook_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook_path, perms).expect("chmod");

    (temp, hook_path)
}

/// Run hook with tool_name and command, return exit code
/// The hook reads JSON from stdin: {"tool_name": "...", "tool_input": {"command": "..."}}
fn run_hook(hook_path: &std::path::Path, tool_name: &str, command: &str) -> i32 {
    use std::io::Write;
    use std::process::Stdio;

    // Build JSON input matching what Claude Code sends
    let json_input = format!(
        r#"{{"tool_name": "{tool_name}", "tool_input": {{"command": {}}}}}"#,
        serde_json::to_string(command).unwrap_or_else(|_| format!("\"{command}\""))
    );

    let mut child = Command::new("bash")
        .arg(hook_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hook");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(json_input.as_bytes()).ok();
    }

    child.wait().expect("wait hook").code().unwrap_or(-1)
}

// =============================================================================
// Tests: Hook BLOCKS commits with Claude attribution (exit code 2)
// =============================================================================

#[test]
fn blocks_coauthored_by_claude_simple() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit -m "Fix bug

Co-Authored-By: Claude <noreply@anthropic.com>""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

#[test]
fn blocks_coauthored_by_claude_opus() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit -m "Add feature

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

#[test]
fn blocks_claude_with_anthropic_email() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit -m "Feature co-authored-by claude noreply@anthropic.com""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

#[test]
fn blocks_case_insensitive() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit -m "Fix

CO-AUTHORED-BY: CLAUDE <NOREPLY@ANTHROPIC.COM>""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

#[test]
fn blocks_heredoc_format() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit -m "$(cat <<'EOF'
Fix parser

Co-Authored-By: Claude <noreply@anthropic.com>
EOF
)""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

// =============================================================================
// Tests: Hook ALLOWS commits without Claude attribution
// =============================================================================

#[test]
fn allows_clean_commit() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit -m "Fix bug in parser""#;

    assert_eq!(run_hook(&hook, "Bash", input), 0);
}

#[test]
fn allows_human_coauthor() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit -m "Feature

Co-Authored-By: Jane Doe <jane@example.com>""#;

    assert_eq!(run_hook(&hook, "Bash", input), 0);
}

#[test]
fn allows_claude_in_message_not_coauthor() {
    let (_temp, hook) = setup_hook();
    // "Claude" in message body without co-author pattern should pass
    let input = r#"git commit -m "Rename Claude class to Assistant""#;

    assert_eq!(run_hook(&hook, "Bash", input), 0);
}

#[test]
fn allows_non_bash_tool() {
    let (_temp, hook) = setup_hook();
    // Non-Bash tools bypass the check entirely
    let input = r#"git commit -m "Fix

Co-Authored-By: Claude <noreply@anthropic.com>""#;

    assert_eq!(run_hook(&hook, "Write", input), 0);
}

#[test]
fn allows_non_commit_git_commands() {
    let (_temp, hook) = setup_hook();
    let input = r#"git log --author="Claude""#;

    assert_eq!(run_hook(&hook, "Bash", input), 0);
}

#[test]
fn allows_empty_input() {
    let (_temp, hook) = setup_hook();
    assert_eq!(run_hook(&hook, "Bash", ""), 0);
}

// =============================================================================
// Tests: Hook script structure validation
// =============================================================================

#[test]
fn hook_contains_blocking_logic() {
    // Check for the regex patterns used in the hook
    assert!(HOOK_COMMIT_FILTER.contains("co-authored-by.*(claude|anthropic|noreply@anthropic)"));
    assert!(HOOK_COMMIT_FILTER.contains("exit 2"));
}

#[test]
fn hook_has_user_friendly_message() {
    assert!(HOOK_COMMIT_FILTER.contains("BLOCKED"));
    assert!(HOOK_COMMIT_FILTER.contains("CLAUDE.md rule 8"));
}
