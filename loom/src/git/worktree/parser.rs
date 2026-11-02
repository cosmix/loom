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
                // Git allows any bytes in a ref. Replacing the invalid ones keeps
                // the record, and a replaced name matches no branch loom asks about.
                let branch = String::from_utf8_lossy(branch);
                wt.branch = Some(
                    branch
                        .strip_prefix("refs/heads/")
                        .unwrap_or(&branch)
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

    #[test]
    fn a_non_utf8_branch_keeps_its_worktree_and_every_other_record() {
        let output = b"worktree /bad\0HEAD abc\0branch refs/heads/loom/b\xffd\0\0worktree /repo\0HEAD def\0branch refs/heads/main\0\0";

        let worktrees = parse_worktree_list(output).unwrap();

        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].path, PathBuf::from("/bad"));
        assert_eq!(worktrees[0].branch.as_deref(), Some("loom/b\u{fffd}d"));
        assert_eq!(worktrees[1].branch.as_deref(), Some("main"));
    }

    #[test]
    fn git_lists_a_worktree_on_a_non_utf8_branch() {
        use std::os::unix::ffi::OsStrExt;
        let repo = tempfile::TempDir::new().unwrap();
        let root = repo.path();
        let linked = tempfile::TempDir::new().unwrap();
        let path = linked.path().join("bad");
        let git = |args: &[&std::ffi::OsStr]| {
            let output = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t.com"])
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(output.status.success(), "{args:?}");
        };
        let os = std::ffi::OsStr::new;
        git(&[os("init"), os("-q"), os("-b"), os("main")]);
        git(&[
            os("commit"),
            os("-q"),
            os("--allow-empty"),
            os("-m"),
            os("seed"),
        ]);
        let branch = std::ffi::OsStr::from_bytes(b"loom/b\xffd");
        git(&[
            os("worktree"),
            os("add"),
            os("-q"),
            os("-b"),
            branch,
            path.as_os_str(),
        ]);

        let worktrees = crate::git::worktree::list_worktrees(root).unwrap();

        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[1].branch.as_deref(), Some("loom/b\u{fffd}d"));
    }
}
