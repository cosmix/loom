//! Parse NUL-delimited Git worktree records.

use anyhow::{bail, Context, Result};
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

/// Parsed worktree information from git worktree list.
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub is_bare: bool,
}

/// Preserve arbitrary path bytes and reject incomplete records.
pub fn parse_worktree_list(output: &[u8]) -> Result<Vec<WorktreeInfo>> {
    if !output.is_empty() && !output.ends_with(b"\0\0") {
        bail!("incomplete git worktree listing");
    }
    let mut worktrees = Vec::new();
    let mut current: Option<WorktreeInfo> = None;
    for field in output.split(|byte| *byte == 0) {
        if field.is_empty() {
            if let Some(wt) = current.take() {
                worktrees.push(wt);
            }
        } else if let Some(path) = field.strip_prefix(b"worktree ") {
            if current.is_some() || path.is_empty() {
                bail!("invalid git worktree listing");
            }
            current = Some(WorktreeInfo {
                path: PathBuf::from(std::ffi::OsString::from_vec(path.to_vec())),
                head: String::new(),
                branch: None,
                is_bare: false,
            });
        } else {
            let wt = current.as_mut().context("worktree field without path")?;
            if let Some(head) = field.strip_prefix(b"HEAD ") {
                wt.head = std::str::from_utf8(head)?.to_string();
            } else if let Some(branch) = field.strip_prefix(b"branch ") {
                let branch = std::str::from_utf8(branch)?;
                wt.branch = Some(
                    branch
                        .strip_prefix("refs/heads/")
                        .unwrap_or(branch)
                        .to_string(),
                );
            } else if field == b"bare" {
                wt.is_bare = true;
            }
        }
    }
    Ok(worktrees)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nul_records_and_rejects_incomplete_output() {
        let output =
            b"worktree /repo\0HEAD abc\0branch refs/heads/main\0\0worktree /repo.git\0bare\0\0";
        let worktrees = parse_worktree_list(output).unwrap();
        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert!(worktrees[1].is_bare);
        assert!(parse_worktree_list(b"worktree /repo\0HEAD abc\0").is_err());
    }
}
