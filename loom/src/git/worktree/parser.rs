//! Worktree output parsing
//!
//! Parses git worktree list --porcelain output into structured data.

use anyhow::Result;
use std::path::PathBuf;

/// Parsed worktree information from git worktree list
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub is_bare: bool,
}

/// Parse git worktree list --porcelain output
///
/// Example input:
/// ```text
/// worktree /home/user/repo
/// HEAD abc123def456
/// branch main
///
/// worktree /home/user/repo/.worktrees/stage-1
/// HEAD def789abc012
/// branch loom/stage-1
/// ```
pub fn parse_worktree_list(output: &str) -> Result<Vec<WorktreeInfo>> {
    let mut worktrees = Vec::new();
    let mut current: Option<WorktreeInfo> = None;

    for line in output.lines() {
        if line.starts_with("worktree ") {
            if let Some(wt) = current.take() {
                worktrees.push(wt);
            }
            let path = line.strip_prefix("worktree ").unwrap_or("");
            current = Some(WorktreeInfo {
                path: PathBuf::from(path),
                head: String::new(),
                branch: None,
                is_bare: false,
            });
        } else if line.starts_with("HEAD ") {
            if let Some(ref mut wt) = current {
                wt.head = line.strip_prefix("HEAD ").unwrap_or("").to_string();
            }
        } else if line.starts_with("branch ") {
            if let Some(ref mut wt) = current {
                let branch_line = line.strip_prefix("branch ").unwrap_or("");
                let branch_name = branch_line
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch_line);
                wt.branch = Some(branch_name.to_string());
            }
        } else if line == "bare" {
            if let Some(ref mut wt) = current {
                wt.is_bare = true;
            }
        }
    }

    if let Some(wt) = current {
        worktrees.push(wt);
    }

    Ok(worktrees)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_worktree_list() {
        let output = r#"worktree /home/user/repo
HEAD abc123def456
branch main

worktree /home/user/repo/.worktrees/stage-1
HEAD def789abc012
branch loom/stage-1
"#;

        let worktrees = parse_worktree_list(output).unwrap();
        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].branch, Some("main".to_string()));
        assert_eq!(worktrees[1].branch, Some("loom/stage-1".to_string()));
    }

    #[test]
    fn test_parse_worktree_list_with_bare() {
        let output = r#"worktree /home/user/repo.git
bare
"#;

        let worktrees = parse_worktree_list(output).unwrap();
        assert_eq!(worktrees.len(), 1);
        assert!(worktrees[0].is_bare);
    }

    #[test]
    fn test_parse_worktree_list_empty() {
        let output = "";
        let worktrees = parse_worktree_list(output).unwrap();
        assert!(worktrees.is_empty());
    }
}
