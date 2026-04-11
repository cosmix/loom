//! Integration tests for hook commit message filtering
//!
//! Tests verify that the commit-filter hook correctly blocks commits
//! containing Claude co-authorship attribution per CLAUDE.md rule 8.
//!
//! These tests run the hook script directly with bash - no loom invocation.

use loom::fs::permissions::constants::{HOOK_COMMIT_FILTER, HOOK_COMMON};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use tempfile::TempDir;

/// Install hook script and its dependencies to temp directory and return path
fn setup_hook() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().expect("create temp dir");

    // Install _common.sh first (commit-filter.sh sources it via dirname)
    let common_path = temp.path().join("_common.sh");
    fs::write(&common_path, HOOK_COMMON).expect("write _common.sh");
    let mut perms = fs::metadata(&common_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&common_path, perms).expect("chmod _common.sh");

    // Install the hook script
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
    // Use a proper Co-Authored-By trailer line with anthropic email
    let input = r#"git commit -m "Feature

Co-Authored-By: claude <noreply@anthropic.com>""#;

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
// Tests: Hook BLOCKS --trailer flag with attribution (bypass vector)
// =============================================================================

#[test]
fn blocks_trailer_flag_coauthored_by() {
    let (_temp, hook) = setup_hook();
    let input =
        r#"git commit --trailer "Co-Authored-By: Claude <noreply@anthropic.com>" -m "Fix bug""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

#[test]
fn blocks_trailer_flag_equals_syntax() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit --trailer="Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>" -m "Fix bug""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

#[test]
fn blocks_trailer_flag_signed_off_by() {
    let (_temp, hook) = setup_hook();
    let input =
        r#"git commit --trailer "Signed-off-by: Claude <noreply@anthropic.com>" -m "Fix bug""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

// =============================================================================
// Tests: Hook BLOCKS --author flag with attribution (bypass vector)
// =============================================================================

#[test]
fn blocks_author_flag_claude() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit --author="Claude <noreply@anthropic.com>" -m "Fix bug""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

#[test]
fn blocks_author_flag_anthropic_email() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit --author "Bot <bot@anthropic.com>" -m "Fix bug""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

// =============================================================================
// Tests: Hook BLOCKS GIT_AUTHOR env var attribution (bypass vector)
// =============================================================================

#[test]
fn blocks_git_author_name_claude_with_anthropic_email() {
    let (_temp, hook) = setup_hook();
    let input = r#"GIT_AUTHOR_NAME="Claude" GIT_AUTHOR_EMAIL="noreply@anthropic.com" git commit -m "Fix bug""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

#[test]
fn allows_git_author_name_claude_without_anthropic_email() {
    let (_temp, hook) = setup_hook();
    let input = r#"GIT_AUTHOR_NAME="Claude" git commit -m "Fix bug""#;

    assert_eq!(run_hook(&hook, "Bash", input), 0);
}

#[test]
fn blocks_git_author_email_anthropic() {
    let (_temp, hook) = setup_hook();
    let input = r#"GIT_AUTHOR_EMAIL="noreply@anthropic.com" git commit -m "Fix bug""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

// =============================================================================
// Tests: Hook BLOCKS Signed-off-by attribution
// =============================================================================

#[test]
fn blocks_signed_off_by_claude() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit -m "Fix bug

Signed-off-by: Claude <noreply@anthropic.com>""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

// =============================================================================
// Tests: Hook BLOCKS attribution text patterns
// =============================================================================

#[test]
fn blocks_generated_with_claude_code() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit -m "Fix bug

Generated with [Claude Code](https://claude.com/claude-code)""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

#[test]
fn blocks_generated_with_claude_ai() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit -m "Fix bug

Generated with Claude Code (claude.ai/code)""#;

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
// Tests: Hook ALLOWS legitimate uses of similar patterns (false positive prevention)
// =============================================================================

#[test]
fn allows_human_author_named_claude() {
    let (_temp, hook) = setup_hook();
    // A real person named Claude with a non-Anthropic email should be allowed
    let input = r#"git commit --author="Claude Shannon <cshannon@example.com>" -m "Fix bug""#;

    assert_eq!(run_hook(&hook, "Bash", input), 0);
}

#[test]
fn allows_trailer_with_human_coauthor() {
    let (_temp, hook) = setup_hook();
    let input =
        r#"git commit --trailer "Co-Authored-By: Jane Doe <jane@example.com>" -m "Fix bug""#;

    assert_eq!(run_hook(&hook, "Bash", input), 0);
}

#[test]
fn allows_git_log_author_claude() {
    let (_temp, hook) = setup_hook();
    // git log with author filter should never be blocked
    let input = r#"git log --author="Claude""#;

    assert_eq!(run_hook(&hook, "Bash", input), 0);
}

#[test]
fn allows_message_mentioning_generated() {
    let (_temp, hook) = setup_hook();
    // Mentioning "generated" in a commit message without Claude Code context is fine
    let input = r#"git commit -m "Fix generated code output formatting""#;

    assert_eq!(run_hook(&hook, "Bash", input), 0);
}

// =============================================================================
// Tests: Hook BLOCKS previously-bypassed vectors (multi-flag, -c trailer, committer)
// =============================================================================

#[test]
fn blocks_multi_m_flag_coauthored_by() {
    let (_temp, hook) = setup_hook();
    // Multiple -m flags put Co-Authored-By on the same command line (no newline)
    let input = r#"git commit -m "feat: change" -m "Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

#[test]
fn blocks_multi_m_flag_with_blank_separator() {
    let (_temp, hook) = setup_hook();
    let input = r#"git commit -m "feat: change" -m "" -m "Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

#[test]
fn blocks_git_c_trailer_config() {
    let (_temp, hook) = setup_hook();
    let input = r#"git -c trailer.co-authored-by.value="Claude <noreply@anthropic.com>" commit -m "feat: change""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

#[test]
fn blocks_git_committer_email_anthropic() {
    let (_temp, hook) = setup_hook();
    let input = r#"GIT_COMMITTER_EMAIL="noreply@anthropic.com" git commit -m "feat: change""#;

    assert_eq!(run_hook(&hook, "Bash", input), 2);
}

// =============================================================================
// Tests: Hook script structure validation
// =============================================================================

#[test]
fn hook_contains_blocking_logic() {
    // Check for the regex patterns used in the hook
    assert!(HOOK_COMMIT_FILTER.contains("Co-Authored-By:"));
    assert!(HOOK_COMMIT_FILTER.contains("(claude|anthropic|noreply@anthropic)"));
    assert!(HOOK_COMMIT_FILTER.contains("exit 2"));
    // Check for bypass vector coverage
    assert!(HOOK_COMMIT_FILTER.contains("--trailer"));
    assert!(HOOK_COMMIT_FILTER.contains("--author"));
    assert!(HOOK_COMMIT_FILTER.contains("GIT_AUTHOR_"));
    assert!(HOOK_COMMIT_FILTER.contains("GIT_COMMITTER_EMAIL"));
    assert!(HOOK_COMMIT_FILTER.contains("Signed-off-by:"));
    assert!(HOOK_COMMIT_FILTER.contains("Generated with"));
    assert!(HOOK_COMMIT_FILTER.contains("trailer."));
}

#[test]
fn hook_has_user_friendly_message() {
    assert!(HOOK_COMMIT_FILTER.contains("BLOCKED"));
    assert!(HOOK_COMMIT_FILTER.contains("CLAUDE.md rule 8"));
}
