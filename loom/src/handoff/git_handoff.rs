//! Git history extraction for handoff and signal generation.
//!
//! This module provides functionality to format git commit history and uncommitted
//! changes for display in handoff and signal files. The formatted information helps
//! resuming sessions understand what work was accomplished in previous sessions.

/// Information about a single commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    /// Short commit hash (7-8 characters)
    pub hash: String,
    /// First line of commit message
    pub message: String,
}

/// Git history extracted from a worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitHistory {
    /// Current branch name (e.g., "loom/stage-1")
    pub branch: String,
    /// Base branch (e.g., "main")
    pub base_branch: String,
    /// Commits since diverging from base
    pub commits: Vec<CommitInfo>,
    /// Files with uncommitted changes
    pub uncommitted_changes: Vec<String>,
}

/// Format git history as markdown for inclusion in handoff/signal files.
pub fn format_git_history_markdown(history: &GitHistory) -> String {
    let mut output = String::new();

    output.push_str("## Git History\n\n");
    output.push_str(&format!(
        "**Branch**: {} (from {})\n\n",
        history.branch, history.base_branch
    ));

    // Check if there's any history to display
    if history.commits.is_empty() && history.uncommitted_changes.is_empty() {
        output.push_str("No commits or uncommitted changes.\n");
        return output;
    }

    // Format commits
    if !history.commits.is_empty() {
        output.push_str("### Commits Since Base\n");
        output.push_str("| Hash | Message |\n");
        output.push_str("|------|---------|");
        for commit in &history.commits {
            output.push_str(&format!("\n| {} | {} |", commit.hash, commit.message));
        }
        output.push('\n');
    }

    // Format uncommitted changes
    if !history.uncommitted_changes.is_empty() {
        if !history.commits.is_empty() {
            output.push('\n');
        }
        output.push_str("### Uncommitted Changes\n");
        for change in &history.uncommitted_changes {
            output.push_str(&format!("- {change}\n"));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_info_creation() {
        let commit = CommitInfo {
            hash: "abc1234".to_string(),
            message: "Add feature X".to_string(),
        };

        assert_eq!(commit.hash, "abc1234");
        assert_eq!(commit.message, "Add feature X");
    }

    #[test]
    fn test_git_history_default() {
        let history = GitHistory::default();

        assert_eq!(history.branch, "");
        assert_eq!(history.base_branch, "");
        assert!(history.commits.is_empty());
        assert!(history.uncommitted_changes.is_empty());
    }

    #[test]
    fn test_format_git_history_with_commits() {
        let history = GitHistory {
            branch: "loom/stage-1".to_string(),
            base_branch: "main".to_string(),
            commits: vec![
                CommitInfo {
                    hash: "abc1234".to_string(),
                    message: "Add feature X".to_string(),
                },
                CommitInfo {
                    hash: "def5678".to_string(),
                    message: "Fix bug in Y".to_string(),
                },
            ],
            uncommitted_changes: vec!["M src/file1.rs".to_string(), "A src/file2.rs".to_string()],
        };

        let markdown = format_git_history_markdown(&history);

        assert!(markdown.contains("## Git History"));
        assert!(markdown.contains("**Branch**: loom/stage-1 (from main)"));
        assert!(markdown.contains("### Commits Since Base"));
        assert!(markdown.contains("| Hash | Message |"));
        assert!(markdown.contains("| abc1234 | Add feature X |"));
        assert!(markdown.contains("| def5678 | Fix bug in Y |"));
        assert!(markdown.contains("### Uncommitted Changes"));
        assert!(markdown.contains("- M src/file1.rs"));
        assert!(markdown.contains("- A src/file2.rs"));
    }

    #[test]
    fn test_format_git_history_empty() {
        let history = GitHistory {
            branch: "loom/stage-1".to_string(),
            base_branch: "main".to_string(),
            commits: vec![],
            uncommitted_changes: vec![],
        };

        let markdown = format_git_history_markdown(&history);

        assert!(markdown.contains("## Git History"));
        assert!(markdown.contains("**Branch**: loom/stage-1 (from main)"));
        assert!(markdown.contains("No commits or uncommitted changes."));
        assert!(!markdown.contains("### Commits Since Base"));
        assert!(!markdown.contains("### Uncommitted Changes"));
    }

    #[test]
    fn test_format_git_history_commits_only() {
        let history = GitHistory {
            branch: "loom/stage-2".to_string(),
            base_branch: "develop".to_string(),
            commits: vec![CommitInfo {
                hash: "1a2b3c4".to_string(),
                message: "Initial implementation".to_string(),
            }],
            uncommitted_changes: vec![],
        };

        let markdown = format_git_history_markdown(&history);

        assert!(markdown.contains("## Git History"));
        assert!(markdown.contains("**Branch**: loom/stage-2 (from develop)"));
        assert!(markdown.contains("### Commits Since Base"));
        assert!(markdown.contains("| 1a2b3c4 | Initial implementation |"));
        assert!(!markdown.contains("### Uncommitted Changes"));
        assert!(!markdown.contains("No commits or uncommitted changes."));
    }

    #[test]
    fn test_format_git_history_uncommitted_only() {
        let history = GitHistory {
            branch: "loom/stage-3".to_string(),
            base_branch: "main".to_string(),
            commits: vec![],
            uncommitted_changes: vec![
                "M src/main.rs".to_string(),
                "D src/old.rs".to_string(),
                "?? src/new.rs".to_string(),
            ],
        };

        let markdown = format_git_history_markdown(&history);

        assert!(markdown.contains("## Git History"));
        assert!(markdown.contains("**Branch**: loom/stage-3 (from main)"));
        assert!(!markdown.contains("### Commits Since Base"));
        assert!(markdown.contains("### Uncommitted Changes"));
        assert!(markdown.contains("- M src/main.rs"));
        assert!(markdown.contains("- D src/old.rs"));
        assert!(markdown.contains("- ?? src/new.rs"));
        assert!(!markdown.contains("No commits or uncommitted changes."));
    }
}
