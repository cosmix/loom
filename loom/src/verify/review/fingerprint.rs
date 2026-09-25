//! Content fingerprint for a review round's changes from its target branch.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::git::branch::is_device_node;
use crate::git::worktree::is_worktree_scaffold_path;
use crate::git::{run_git, run_git_checked};

const MAX_REVIEW_FILE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeFingerprint {
    pub value: String,
    pub base: String,
    pub files: BTreeMap<String, String>,
}

/// `entries` are worktree-relative paths with content, or `None` for a deletion.
pub fn fingerprint_from(base: &str, entries: &[(String, Option<Vec<u8>>)]) -> ChangeFingerprint {
    let files: BTreeMap<String, String> = entries
        .iter()
        .map(|(path, content)| {
            let digest = content
                .as_ref()
                .map(|bytes| hex::encode(Sha256::digest(bytes)))
                .unwrap_or_else(|| "deleted".to_string());
            (path.clone(), digest)
        })
        .collect();

    let mut hasher = Sha256::new();
    hasher.update(b"base:");
    hasher.update(base.as_bytes());
    hasher.update(b"\n");
    for (path, digest) in &files {
        hasher.update(path.as_bytes());
        hasher.update(b"\t");
        hasher.update(digest.as_bytes());
        hasher.update(b"\n");
    }

    ChangeFingerprint {
        value: format!("sha256:{}", hex::encode(hasher.finalize())),
        base: base.to_string(),
        files,
    }
}

/// Fingerprint the worktree against `git merge-base HEAD <target_branch>`.
pub fn compute(worktree: &Path, target_branch: &str) -> Result<ChangeFingerprint> {
    let base = run_git_checked(&["merge-base", "HEAD", target_branch], worktree)
        .with_context(|| format!("finding merge base with {target_branch}"))?;
    let mut paths = git_paths(worktree, &["diff", "--name-only", "-z", &base, "--"])?;
    let untracked = git_paths(
        worktree,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    paths.extend(without_device_nodes(worktree, untracked));

    let mut entries = Vec::new();
    for path in paths {
        if is_worktree_scaffold_path(&path) {
            continue;
        }
        let content = match read_bounded(worktree, Path::new(&path), MAX_REVIEW_FILE_BYTES) {
            Ok(bytes) => Some(bytes),
            Err(error) if is_not_found(&error) => None,
            Err(error) => {
                return Err(error).with_context(|| format!("reading changed file {path}"));
            }
        };
        entries.push((path, content));
    }
    Ok(fingerprint_from(&base, &entries))
}

/// `untracked` without its device nodes ([`is_device_node`]). The Bash
/// sandbox's `/dev/null` mounts over worktree-root dotfiles are listed by git
/// inside it and absent on the host, so keeping them would refuse the read
/// and split the fingerprint a session computes from the host's.
fn without_device_nodes(
    worktree: &Path,
    untracked: BTreeSet<String>,
) -> impl Iterator<Item = String> + '_ {
    untracked
        .into_iter()
        .filter(move |path| !is_device_node(worktree, path))
}

fn git_paths(worktree: &Path, args: &[&str]) -> Result<BTreeSet<String>> {
    let output = run_git(args, worktree)?;
    if !output.status.success() {
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            worktree.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            std::str::from_utf8(path)
                .context("git returned a non-UTF-8 path")
                .map(str::to_owned)
        })
        .collect()
}

/// Sorted paths whose content hashes differ, including one-sided paths.
pub fn changed_since(
    previous: &BTreeMap<String, String>,
    current: &BTreeMap<String, String>,
) -> Vec<String> {
    previous
        .keys()
        .chain(current.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|path| previous.get(*path) != current.get(*path))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_from_records_deleted_entries() {
        let fingerprint = fingerprint_from(
            "base",
            &[
                ("gone.txt".to_string(), None),
                ("empty.txt".to_string(), Some(Vec::new())),
            ],
        );

        assert_eq!(fingerprint.base, "base");
        assert_eq!(fingerprint.files["gone.txt"], "deleted");
        assert_eq!(
            fingerprint.files["empty.txt"],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert!(fingerprint.value.starts_with("sha256:"));
    }

    #[test]
    fn fingerprint_from_sorts_paths_before_hashing() {
        let first = ("a.txt".to_string(), Some(b"a".to_vec()));
        let second = ("z.txt".to_string(), Some(b"z".to_vec()));

        let forward = fingerprint_from("base", &[first.clone(), second.clone()]);
        let reverse = fingerprint_from("base", &[second, first]);

        assert_eq!(forward, reverse);
    }

    #[test]
    fn changed_since_includes_one_sided_paths() {
        let previous = BTreeMap::from([
            ("a.txt".to_string(), "old".to_string()),
            ("b.txt".to_string(), "same".to_string()),
        ]);
        let current = BTreeMap::from([
            ("b.txt".to_string(), "same".to_string()),
            ("c.txt".to_string(), "new".to_string()),
        ]);

        assert_eq!(changed_since(&previous, &current), ["a.txt", "c.txt"]);
    }

    fn git_ok(root: &Path, args: &[&str]) -> Result<()> {
        run_git_checked(args, root)?;
        Ok(())
    }

    #[test]
    fn fingerprint_ignores_commits() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        git_ok(root, &["init", "-b", "main"])?;
        git_ok(root, &["config", "user.email", "review-test@example.com"])?;
        git_ok(root, &["config", "user.name", "Review Test"])?;
        std::fs::write(root.join("file.txt"), b"initial")?;
        git_ok(root, &["add", "file.txt"])?;
        git_ok(root, &["commit", "-m", "initial"])?;
        git_ok(root, &["switch", "-c", "feature"])?;

        std::fs::write(root.join("file.txt"), b"first change")?;
        let a = compute(root, "main")?;
        git_ok(root, &["commit", "-am", "x"])?;
        let b = compute(root, "main")?;
        assert_eq!(a.value, b.value);
        assert_eq!(a.files, b.files);

        std::fs::write(root.join("file.txt"), b"second change")?;
        let c = compute(root, "main")?;
        assert_ne!(b.value, c.value);
        assert_eq!(changed_since(&b.files, &c.files), ["file.txt"]);
        Ok(())
    }

    /// The sandbox's `/dev/null` mounts over worktree-root dotfiles are never
    /// read, never fingerprinted; git skips a FIFO on its own.
    #[test]
    fn fingerprint_leaves_out_untracked_device_nodes() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        git_ok(root, &["init", "-b", "main"])?;
        git_ok(root, &["config", "user.email", "review-test@example.com"])?;
        git_ok(root, &["config", "user.name", "Review Test"])?;
        std::fs::write(root.join("file.txt"), b"initial")?;
        git_ok(root, &["add", "file.txt"])?;
        git_ok(root, &["commit", "-m", "initial"])?;
        nix::unistd::mkfifo(&root.join(".bashrc"), nix::sys::stat::Mode::S_IRWXU)?;
        std::fs::write(root.join("notes.txt"), b"x")?;

        let fingerprint = compute(root, "main")?;

        assert_eq!(fingerprint.files.keys().collect::<Vec<_>>(), ["notes.txt"]);
        // Inside the sandbox the mount point is listed, so the filter is
        // checked on such a listing, `/dev/null` standing in for the mount.
        let listed = BTreeSet::from(["null".to_string(), "notes.txt".to_string()]);
        let kept: Vec<String> = without_device_nodes(Path::new("/dev"), listed).collect();
        assert_eq!(kept, ["notes.txt"]);
        Ok(())
    }
}
