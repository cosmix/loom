//! Branch information types and structured ref parsing.

use anyhow::Result;
use std::path::Path;

/// Branch information
#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub commit_hash: String,
    pub commit_message: String,
}

/// List local refs without relying on Git's human-oriented branch display.
pub(super) fn list_refs(repo_root: &Path, merged: Option<&str>) -> Result<Vec<BranchInfo>> {
    let merged_arg = merged.map(|target| format!("--merged={}", super::naming::branch_ref(target)));
    let mut args = vec![
        "for-each-ref",
        "--format=%(HEAD)%00%(refname)%00%(objectname)%00%(subject)",
    ];
    if let Some(ref arg) = merged_arg {
        args.push(arg);
    }
    args.push("refs/heads/");
    let output = crate::git::runner::run_git_checked_bytes(&args, repo_root)?;
    parse_branch_list(&String::from_utf8_lossy(&output))
}

fn parse_branch_list(output: &str) -> Result<Vec<BranchInfo>> {
    output
        .lines()
        .map(|line| {
            let mut fields = line.splitn(4, '\0');
            let head = fields.next().unwrap_or_default();
            let reference = fields
                .next()
                .ok_or_else(|| anyhow::anyhow!("missing branch ref"))?;
            let hash = fields
                .next()
                .ok_or_else(|| anyhow::anyhow!("missing branch hash"))?;
            let subject = fields
                .next()
                .ok_or_else(|| anyhow::anyhow!("missing branch subject"))?;
            let name = reference
                .strip_prefix("refs/heads/")
                .ok_or_else(|| anyhow::anyhow!("unexpected branch ref: {reference}"))?;
            Ok(BranchInfo {
                name: name.to_owned(),
                is_current: head == "*",
                commit_hash: hash.to_owned(),
                commit_message: subject.to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_fields_preserve_hash_and_empty_subject() {
        let branches = parse_branch_list(
            "*\0refs/heads/main\0abc1234\0Initial commit\n \0refs/heads/loom/long-name\0def5678\0",
        )
        .unwrap();
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[0].name, "main");
        assert!(branches[0].is_current);
        assert_eq!(branches[0].commit_hash, "abc1234");
        assert_eq!(branches[0].commit_message, "Initial commit");
        assert_eq!(branches[1].name, "loom/long-name");
        assert!(!branches[1].is_current);
        assert_eq!(branches[1].commit_hash, "def5678");
        assert_eq!(branches[1].commit_message, "");
    }
}
