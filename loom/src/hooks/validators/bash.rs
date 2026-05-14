//! Bash command validation for worktree isolation.
//!
//! Validates that bash commands don't violate worktree isolation boundaries.

use super::{BlockedReason, ValidationResult};
use regex::Regex;
use std::sync::LazyLock;

/// Error type for bash validation failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashValidationError {
    /// The blocked reason
    pub reason: BlockedReason,
    /// The offending pattern found in the command
    pub pattern: String,
}

impl BashValidationError {
    /// Create a new bash validation error
    pub fn new(reason: BlockedReason, pattern: impl Into<String>) -> Self {
        Self {
            reason,
            pattern: pattern.into(),
        }
    }
}

// Compiled regex patterns for performance
static GIT_DASH_C_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"git\s+-C\s+").expect("Invalid regex"));
static GIT_WORK_TREE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"git\s+--work-tree").expect("Invalid regex"));
static PATH_TRAVERSAL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.\.[\\/]\.\.").expect("Invalid regex"));
static WORKTREES_ACCESS_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.worktrees/([^/\s]+)").expect("Invalid regex"));
static MESSAGE_DOUBLE_QUOTE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"-m\s*"[^"]*""#).expect("Invalid regex"));
static MESSAGE_SINGLE_QUOTE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-m\s*'[^']*'").expect("Invalid regex"));
static LONG_MESSAGE_DOUBLE_QUOTE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"--message[=\s]*"[^"]*""#).expect("Invalid regex"));
static LONG_MESSAGE_SINGLE_QUOTE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"--message[=\s]*'[^']*'").expect("Invalid regex"));

/// Extract heredoc marker from a line containing <<MARKER or <<'MARKER' etc.
fn extract_heredoc_marker(line: &str) -> Option<String> {
    // Find << in the line
    let idx = line.find("<<")?;
    let rest = &line[idx + 2..];
    // Skip optional dash (<<-)
    let rest = rest.strip_prefix('-').unwrap_or(rest);
    // Skip whitespace
    let rest = rest.trim_start();
    // Strip optional quotes
    let rest = rest.trim_start_matches(['\'', '"']);
    // Extract marker (alphanumeric + underscore)
    let marker: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if marker.is_empty() {
        None
    } else {
        Some(marker)
    }
}

/// Strip heredoc bodies and -m/--message quoted content from a command.
/// This prevents false positives where pattern words appear inside message text.
fn strip_embedded_content(command: &str) -> String {
    // Phase 1: Strip heredoc bodies
    let mut result = String::new();
    let mut inside_heredoc = false;
    let mut marker = String::new();

    for line in command.lines() {
        if inside_heredoc {
            if line == marker {
                inside_heredoc = false;
            }
            continue;
        }
        if let Some(m) = extract_heredoc_marker(line) {
            marker = m;
            inside_heredoc = true;
            result.push_str(line);
            result.push('\n');
            continue;
        }
        result.push_str(line);
        result.push('\n');
    }

    // Phase 2: Strip -m/--message quoted content
    let result = MESSAGE_DOUBLE_QUOTE_PATTERN.replace_all(&result, r#"-m """#);
    let result = MESSAGE_SINGLE_QUOTE_PATTERN.replace_all(&result, "-m ''");
    let result = LONG_MESSAGE_DOUBLE_QUOTE_PATTERN.replace_all(&result, r#"--message="""#);
    let result = LONG_MESSAGE_SINGLE_QUOTE_PATTERN.replace_all(&result, "--message=''");

    result.into_owned()
}

/// Validate a bash command for worktree isolation violations.
///
/// # Arguments
/// * `command` - The bash command to validate
/// * `current_stage` - The current stage ID (used to allow access to own worktree)
///
/// # Returns
/// * `ValidationResult::Allowed` if the command is safe
/// * `ValidationResult::Blocked(reason)` if the command violates isolation
///
/// # Examples
/// ```
/// use loom::hooks::validators::validate_bash_command;
///
/// // Safe command
/// let result = validate_bash_command("cargo build", "my-stage");
/// assert!(result.is_allowed());
///
/// // Blocked: git -C
/// let result = validate_bash_command("git -C ../other commit", "my-stage");
/// assert!(result.is_blocked());
/// ```
pub fn validate_bash_command(command: &str, current_stage: &str) -> ValidationResult {
    let stripped = strip_embedded_content(command);
    let command = &stripped; // Shadow command with stripped version for all checks

    // Check for git -C (directory override)
    if GIT_DASH_C_PATTERN.is_match(command) {
        return ValidationResult::Blocked(BlockedReason::GitDirectoryOverride);
    }

    // Check for git --work-tree (directory override)
    if GIT_WORK_TREE_PATTERN.is_match(command) {
        return ValidationResult::Blocked(BlockedReason::GitDirectoryOverride);
    }

    // Check for ../../ path traversal
    if PATH_TRAVERSAL_PATTERN.is_match(command) {
        return ValidationResult::Blocked(BlockedReason::PathTraversal);
    }

    // Check for .worktrees/ access (allow current stage only)
    if let Some(captures) = WORKTREES_ACCESS_PATTERN.captures(command) {
        let accessed_stage = captures.get(1).map(|m| m.as_str()).unwrap_or("");
        if accessed_stage != current_stage {
            return ValidationResult::Blocked(BlockedReason::CrossWorktreeAccess {
                target_stage: Some(accessed_stage.to_string()),
            });
        }
    }

    ValidationResult::Allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_normal_commands() {
        let commands = [
            "cargo build",
            "cargo test",
            "git status",
            "git add src/main.rs",
            "git commit -m 'test'",
            "ls -la",
            "pwd",
            "cat file.txt",
            "rg pattern src/",
        ];

        for cmd in &commands {
            let result = validate_bash_command(cmd, "test-stage");
            assert!(
                result.is_allowed(),
                "Expected '{}' to be allowed, but got blocked",
                cmd
            );
        }
    }

    #[test]
    fn test_blocks_git_dash_c() {
        let commands = [
            "git -C ../other status",
            "git -C /path/to/other commit",
            "git -C . status", // Even current dir is suspicious
        ];

        for cmd in &commands {
            let result = validate_bash_command(cmd, "test-stage");
            assert!(result.is_blocked(), "Expected '{}' to be blocked", cmd);
            assert_eq!(
                result.blocked_reason(),
                Some(&BlockedReason::GitDirectoryOverride)
            );
        }
    }

    #[test]
    fn test_blocks_git_work_tree() {
        let commands = [
            "git --work-tree=/other status",
            "git --work-tree=../parent status",
        ];

        for cmd in &commands {
            let result = validate_bash_command(cmd, "test-stage");
            assert!(result.is_blocked(), "Expected '{}' to be blocked", cmd);
            assert_eq!(
                result.blocked_reason(),
                Some(&BlockedReason::GitDirectoryOverride)
            );
        }
    }

    #[test]
    fn test_blocks_path_traversal() {
        let commands = [
            "cat ../../file.txt",
            "ls ../../../",
            r"cat ..\..\file.txt", // Windows-style
            "cd ../../other && ls",
        ];

        for cmd in &commands {
            let result = validate_bash_command(cmd, "test-stage");
            assert!(result.is_blocked(), "Expected '{}' to be blocked", cmd);
            assert_eq!(result.blocked_reason(), Some(&BlockedReason::PathTraversal));
        }
    }

    #[test]
    fn test_allows_single_parent() {
        // Single .. is generally OK (within worktree)
        let result = validate_bash_command("cat ../file.txt", "test-stage");
        assert!(result.is_allowed());
    }

    #[test]
    fn test_blocks_cross_worktree_access() {
        let result = validate_bash_command("ls .worktrees/other-stage/", "my-stage");
        assert!(result.is_blocked());
        assert!(matches!(
            result.blocked_reason(),
            Some(BlockedReason::CrossWorktreeAccess { .. })
        ));
    }

    #[test]
    fn test_allows_own_worktree_access() {
        // Should allow access to own worktree
        let result = validate_bash_command("ls .worktrees/my-stage/", "my-stage");
        assert!(result.is_allowed());
    }

    #[test]
    fn test_cross_worktree_captures_target() {
        let result = validate_bash_command("cat .worktrees/other-stage/file.txt", "my-stage");
        if let ValidationResult::Blocked(BlockedReason::CrossWorktreeAccess { target_stage }) =
            result
        {
            assert_eq!(target_stage, Some("other-stage".to_string()));
        } else {
            panic!("Expected CrossWorktreeAccess block");
        }
    }

    #[test]
    fn test_false_positive_worktrees_in_message() {
        // Issue #13: git commit -m "Add .worktrees/ to .gitignore" was blocked
        let result = validate_bash_command(
            r#"git commit -m "Add .worktrees/ to .gitignore""#,
            "my-stage",
        );
        assert!(
            result.is_allowed(),
            "Message containing .worktrees/ should not be blocked"
        );
    }

    #[test]
    fn test_false_positive_git_c_in_message() {
        let result = validate_bash_command(
            r#"git commit -m "Use git -C for directory changes""#,
            "my-stage",
        );
        assert!(
            result.is_allowed(),
            "Message containing git -C should not be blocked"
        );
    }

    #[test]
    fn test_false_positive_path_traversal_in_message() {
        let result = validate_bash_command(r#"git commit -m "Fixed ../../path issue""#, "my-stage");
        assert!(
            result.is_allowed(),
            "Message containing ../../ should not be blocked"
        );
    }

    #[test]
    fn test_false_positive_heredoc_content() {
        let cmd = "cat <<'EOF'\ngit -C ../other status\n../../escape\n.worktrees/other-stage/\nEOF";
        let result = validate_bash_command(cmd, "my-stage");
        assert!(
            result.is_allowed(),
            "Content inside heredoc should not be blocked"
        );
    }

    #[test]
    fn test_true_positive_still_blocks() {
        // Real violations must still be caught
        let result = validate_bash_command("git -C ../other status", "my-stage");
        assert!(result.is_blocked(), "Real git -C should still be blocked");

        let result = validate_bash_command("cd ../../other && ls", "my-stage");
        assert!(
            result.is_blocked(),
            "Real path traversal should still be blocked"
        );

        let result = validate_bash_command("ls .worktrees/other-stage/", "my-stage");
        assert!(
            result.is_blocked(),
            "Real cross-worktree access should still be blocked"
        );
    }

    #[test]
    fn test_strip_embedded_content_message() {
        let stripped = strip_embedded_content(r#"git commit -m "Add .worktrees/ to .gitignore""#);
        assert!(
            !stripped.contains(".worktrees/"),
            "Message content should be stripped"
        );
        assert!(
            stripped.contains("git commit"),
            "Command structure should remain"
        );
    }

    #[test]
    fn test_strip_embedded_content_heredoc() {
        let cmd = "cat <<'EOF'\n.worktrees/other\n../../bad\nEOF\necho done";
        let stripped = strip_embedded_content(cmd);
        assert!(
            !stripped.contains(".worktrees/other"),
            "Heredoc body should be stripped"
        );
        assert!(
            !stripped.contains("../../bad"),
            "Heredoc body should be stripped"
        );
        assert!(
            stripped.contains("echo done"),
            "Post-heredoc content should remain"
        );
    }

    #[test]
    fn test_heredoc_indented_terminator_not_closed_by_trimmed_line() {
        // An indented terminator must NOT close the heredoc.
        // Only an exact-match (non-indented) terminator closes it.
        // With the old `line.trim() == marker`, "    EOF".trim() == "EOF" would
        // incorrectly close the heredoc early, leaving "still inside heredoc body"
        // un-stripped — this test catches that regression.
        let command = "git commit -m \"$(cat <<'EOF'\nThis is the message\n    EOF\nstill inside heredoc body\nEOF\n)\"";
        let stripped = strip_embedded_content(command);
        assert!(
            !stripped.contains("still inside heredoc body"),
            "Indented terminator should not close heredoc; 'still inside heredoc body' should be stripped.\nStripped: {:?}",
            stripped
        );
    }
}
