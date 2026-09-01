//! Validation logic for worktree isolation enforcement.
//!
//! This module provides Rust implementations of the validation rules enforced
//! by the `worktree-isolation.sh` hook. These validators can be used for:
//! - Testing the validation logic
//! - Pre-validation before spawning sessions
//! - Generating detailed error messages
//!
//! ## Validation Rules
//!
//! ### Bash Commands
//! - Block `git -C` or `git --work-tree` (directory overrides)
//! - Block `../../` path traversal patterns
//! - Block `.worktrees/` access except current worktree
//!
//! ### File Paths (Edit/Write)
//! - Block writes to `.loom/work/stages/` and `.loom/work/sessions/`
//!   (or the legacy `.work/stages/` and `.work/sessions/`)
//! - Block writes to other worktrees
//! - Block `../../` path traversal in paths

mod bash;
mod file_path;

pub use bash::{validate_bash_command, BashValidationError};
pub use file_path::{
    extract_worktree_stage, has_path_traversal, is_protected_state_path, validate_file_path,
    FilePathValidationError,
};

/// Result of a validation check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    /// The operation is allowed
    Allowed,
    /// The operation is blocked with a reason
    Blocked(BlockedReason),
}

impl ValidationResult {
    /// Returns true if the operation is allowed
    pub fn is_allowed(&self) -> bool {
        matches!(self, ValidationResult::Allowed)
    }

    /// Returns true if the operation is blocked
    pub fn is_blocked(&self) -> bool {
        matches!(self, ValidationResult::Blocked(_))
    }

    /// Get the blocked reason if blocked, None if allowed
    pub fn blocked_reason(&self) -> Option<&BlockedReason> {
        match self {
            ValidationResult::Blocked(reason) => Some(reason),
            ValidationResult::Allowed => None,
        }
    }
}

/// Reasons why an operation was blocked
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockedReason {
    /// Git -C or --work-tree detected
    GitDirectoryOverride,
    /// Path traversal (../../) detected
    PathTraversal,
    /// Access to .worktrees/ other than current stage
    CrossWorktreeAccess {
        /// The stage being accessed
        target_stage: Option<String>,
    },
    /// Write to protected state files (.loom/work/stages/ or .loom/work/sessions/)
    ProtectedStateFile,
    /// Write to another worktree
    CrossWorktreeWrite,
}

impl BlockedReason {
    /// Get a human-readable description of the blocked reason
    pub fn description(&self) -> &'static str {
        match self {
            BlockedReason::GitDirectoryOverride => "Git directory override detected",
            BlockedReason::PathTraversal => "Path traversal detected",
            BlockedReason::CrossWorktreeAccess { .. } => "Cross-worktree access detected",
            BlockedReason::ProtectedStateFile => "Protected state file access",
            BlockedReason::CrossWorktreeWrite => "Cross-worktree file write",
        }
    }

    /// Get a suggestion for the correct approach
    pub fn suggestion(&self) -> &'static str {
        match self {
            BlockedReason::GitDirectoryOverride => {
                "Run git commands in the CURRENT worktree only. Use relative paths within this worktree."
            }
            BlockedReason::PathTraversal => {
                "Use relative paths WITHIN this worktree. All files you need are in the worktree."
            }
            BlockedReason::CrossWorktreeAccess { .. } => {
                "Stay in YOUR worktree. Your files and context are all here."
            }
            BlockedReason::ProtectedStateFile => {
                "Use `loom stage complete` to complete a stage. Use `loom memory` to record insights."
            }
            BlockedReason::CrossWorktreeWrite => {
                "Write only to files in YOUR worktree. Files are merged after stage completion."
            }
        }
    }

    /// Format a complete blocked message in the standard format
    pub fn format_message(&self, current_stage: &str) -> String {
        format!(
            r#"
BLOCKED: {}

You tried to: {}
Instead, you should: {}

Current stage: {}
"#,
            self.description(),
            self.action_description(),
            self.suggestion(),
            current_stage
        )
    }

    fn action_description(&self) -> &'static str {
        match self {
            BlockedReason::GitDirectoryOverride => {
                "Use git -C or --work-tree to access another directory"
            }
            BlockedReason::PathTraversal => "Use ../../ to escape the worktree",
            BlockedReason::CrossWorktreeAccess { .. } => {
                "Access .worktrees/ directory (another stage's worktree)"
            }
            BlockedReason::ProtectedStateFile => {
                "Write to .loom/work/stages/ or .loom/work/sessions/"
            }
            BlockedReason::CrossWorktreeWrite => "Write to another stage's worktree",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_result_methods() {
        let allowed = ValidationResult::Allowed;
        assert!(allowed.is_allowed());
        assert!(!allowed.is_blocked());
        assert!(allowed.blocked_reason().is_none());

        let blocked = ValidationResult::Blocked(BlockedReason::PathTraversal);
        assert!(!blocked.is_allowed());
        assert!(blocked.is_blocked());
        assert!(blocked.blocked_reason().is_some());
    }

    #[test]
    fn test_blocked_reason_descriptions() {
        let reasons = [
            BlockedReason::GitDirectoryOverride,
            BlockedReason::PathTraversal,
            BlockedReason::CrossWorktreeAccess { target_stage: None },
            BlockedReason::ProtectedStateFile,
            BlockedReason::CrossWorktreeWrite,
        ];

        for reason in &reasons {
            assert!(!reason.description().is_empty());
            assert!(!reason.suggestion().is_empty());
            assert!(!reason.format_message("test-stage").is_empty());
        }
    }
}
