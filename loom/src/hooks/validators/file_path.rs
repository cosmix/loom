//! File path validation for worktree isolation.
//!
//! Validates that Edit/Write operations don't violate worktree isolation boundaries.

use super::{BlockedReason, ValidationResult};
use regex::Regex;
use std::sync::LazyLock;

/// Error type for file path validation failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePathValidationError {
    /// The blocked reason
    pub reason: BlockedReason,
    /// The offending path
    pub path: String,
}

impl FilePathValidationError {
    /// Create a new file path validation error
    pub fn new(reason: BlockedReason, path: impl Into<String>) -> Self {
        Self {
            reason,
            path: path.into(),
        }
    }
}

// Compiled regex patterns for performance
//
// The optional `loom/` covers both the current nested state root
// (`.loom/work/`) and a legacy project that has not moved
// (`.work/`) — one pattern protects either spelling.
static PROTECTED_STATE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.(?:loom/)?work/(stages|sessions)/").expect("Invalid regex"));
static PATH_TRAVERSAL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.\.[\\/]\.\.").expect("Invalid regex"));
static WORKTREES_PATH_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.worktrees/([^/]+)/").expect("Invalid regex"));

/// Validate a file path for Edit/Write operations.
///
/// # Arguments
/// * `path` - The file path to validate
/// * `current_stage` - The current stage ID (used to allow writes to own worktree)
///
/// # Returns
/// * `ValidationResult::Allowed` if the path is safe to write
/// * `ValidationResult::Blocked(reason)` if the path violates isolation
///
/// # Examples
/// ```
/// use loom::hooks::validators::validate_file_path;
///
/// // Safe path
/// let result = validate_file_path("src/main.rs", "my-stage");
/// assert!(result.is_allowed());
///
/// // Blocked: protected state file
/// let result = validate_file_path(".loom/work/stages/01-my-stage.md", "my-stage");
/// assert!(result.is_blocked());
/// ```
pub fn validate_file_path(path: &str, current_stage: &str) -> ValidationResult {
    // Check for writes to protected state files (.loom/work/stages/ or .loom/work/sessions/)
    if PROTECTED_STATE_PATTERN.is_match(path) {
        return ValidationResult::Blocked(BlockedReason::ProtectedStateFile);
    }

    // Check for ../../ path traversal
    if PATH_TRAVERSAL_PATTERN.is_match(path) {
        return ValidationResult::Blocked(BlockedReason::PathTraversal);
    }

    // Check for writes to other worktrees
    if let Some(captures) = WORKTREES_PATH_PATTERN.captures(path) {
        let target_stage = captures.get(1).map(|m| m.as_str()).unwrap_or("");
        if target_stage != current_stage {
            return ValidationResult::Blocked(BlockedReason::CrossWorktreeWrite);
        }
    }

    ValidationResult::Allowed
}

/// Check if a path is a protected state file that should not be directly edited.
///
/// Protected paths include:
/// - `.loom/work/stages/*` (or legacy `.work/stages/*`) - Stage state files (managed by loom)
/// - `.loom/work/sessions/*` (or legacy `.work/sessions/*`) - Session tracking files (managed by loom)
///
/// Note: Other `.loom/work/` paths (like `.loom/work/heartbeat/`) are allowed.
pub fn is_protected_state_path(path: &str) -> bool {
    PROTECTED_STATE_PATTERN.is_match(path)
}

/// Check if a path contains path traversal patterns.
pub fn has_path_traversal(path: &str) -> bool {
    PATH_TRAVERSAL_PATTERN.is_match(path)
}

/// Extract the worktree stage ID from a path, if present.
///
/// Returns Some(stage_id) if the path is within a worktree, None otherwise.
pub fn extract_worktree_stage(path: &str) -> Option<String> {
    WORKTREES_PATH_PATTERN
        .captures(path)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_normal_paths() {
        let paths = [
            "src/main.rs",
            "Cargo.toml",
            "tests/integration/test_foo.rs",
            ".loom/work/heartbeat/stage.json", // Heartbeat is allowed
            ".loom/work/signals/session-123.md", // Signals are allowed
            ".loom/work/handoffs/handoff-001.md", // Handoffs are allowed
            ".work/heartbeat/stage.json",      // Legacy layout, same rule
        ];

        for path in &paths {
            let result = validate_file_path(path, "test-stage");
            assert!(
                result.is_allowed(),
                "Expected '{}' to be allowed, but got blocked",
                path
            );
        }
    }

    #[test]
    fn test_blocks_protected_stages() {
        let paths = [
            ".loom/work/stages/01-bootstrap.md",
            ".loom/work/stages/02-implementation.md",
            "/absolute/.loom/work/stages/stage.md",
            ".work/stages/01-bootstrap.md", // Legacy layout, same rule
        ];

        for path in &paths {
            let result = validate_file_path(path, "test-stage");
            assert!(result.is_blocked(), "Expected '{}' to be blocked", path);
            assert_eq!(
                result.blocked_reason(),
                Some(&BlockedReason::ProtectedStateFile)
            );
        }
    }

    #[test]
    fn test_blocks_protected_sessions() {
        let paths = [
            ".loom/work/sessions/session-abc123.md",
            ".loom/work/sessions/session-xyz.md",
            ".work/sessions/session-abc123.md", // Legacy layout, same rule
        ];

        for path in &paths {
            let result = validate_file_path(path, "test-stage");
            assert!(result.is_blocked(), "Expected '{}' to be blocked", path);
            assert_eq!(
                result.blocked_reason(),
                Some(&BlockedReason::ProtectedStateFile)
            );
        }
    }

    #[test]
    fn test_blocks_path_traversal() {
        let paths = [
            "../../main-repo/file.txt",
            "../../../escape.txt",
            r"..\..\file.txt", // Windows-style
        ];

        for path in &paths {
            let result = validate_file_path(path, "test-stage");
            assert!(result.is_blocked(), "Expected '{}' to be blocked", path);
            assert_eq!(result.blocked_reason(), Some(&BlockedReason::PathTraversal));
        }
    }

    #[test]
    fn test_allows_single_parent() {
        // Single .. is generally OK (within worktree)
        let result = validate_file_path("../sibling/file.txt", "test-stage");
        assert!(result.is_allowed());
    }

    #[test]
    fn test_blocks_cross_worktree_write() {
        let result = validate_file_path(".worktrees/other-stage/src/main.rs", "my-stage");
        assert!(result.is_blocked());
        assert_eq!(
            result.blocked_reason(),
            Some(&BlockedReason::CrossWorktreeWrite)
        );
    }

    #[test]
    fn test_allows_own_worktree_write() {
        let result = validate_file_path(".worktrees/my-stage/src/main.rs", "my-stage");
        assert!(result.is_allowed());
    }

    #[test]
    fn test_is_protected_state_path() {
        assert!(is_protected_state_path(".loom/work/stages/01-stage.md"));
        assert!(is_protected_state_path(".loom/work/sessions/session.md"));
        assert!(!is_protected_state_path(".loom/work/heartbeat/stage.json"));
        assert!(!is_protected_state_path(".loom/work/signals/signal.md"));
        assert!(!is_protected_state_path("src/main.rs"));
        // Legacy layout, same rule.
        assert!(is_protected_state_path(".work/stages/01-stage.md"));
        assert!(is_protected_state_path(".work/sessions/session.md"));
        assert!(!is_protected_state_path(".work/heartbeat/stage.json"));
    }

    #[test]
    fn test_has_path_traversal() {
        assert!(has_path_traversal("../../file.txt"));
        assert!(has_path_traversal("foo/../../bar"));
        assert!(!has_path_traversal("../file.txt"));
        assert!(!has_path_traversal("./file.txt"));
        assert!(!has_path_traversal("src/main.rs"));
    }

    #[test]
    fn test_extract_worktree_stage() {
        assert_eq!(
            extract_worktree_stage(".worktrees/my-stage/src/main.rs"),
            Some("my-stage".to_string())
        );
        assert_eq!(
            extract_worktree_stage(".worktrees/other/file.txt"),
            Some("other".to_string())
        );
        assert_eq!(extract_worktree_stage("src/main.rs"), None);
        assert_eq!(extract_worktree_stage(".worktrees/"), None);
    }
}
