//! Control paths: files a stage branch may change only with a person's review.
//!
//! Changes to `.claude/`, `.mcp.json`, `.loom/`, or the tracked hooks directory
//! require review (`doc/plans/PLAN-loom-state-confinement.md` §9). The gate runs
//! on exact merge commits; early filters use [`control_path_violation`].

use std::path::{Component, Path};

use anyhow::{bail, Context, Result};

use crate::fs::lexical::normalize_lexically;
use crate::git::branch::branch_ref;
use crate::git::changes::{changed_names, merge_base, DiffNames};
use crate::git::runner::run_git;
use crate::git::worktree::WorktreeGit;

/// Whether `merge_stage` checks control paths on the commits it merges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeGate {
    /// Hold a branch that touches a control path for review.
    Enforce,
    /// Bypass after operator review (`loom stage human-review --force-complete`).
    Bypass,
}

/// A review reason when `branch` changed a control path since splitting from
/// `target`. Both resolve through `refs/heads/` to exclude same-named tags.
/// Merge guards refuse on errors; the reason names `branch`.
pub fn control_path_violation(
    repo_root: &Path,
    target: &str,
    branch: &str,
) -> Result<Option<String>> {
    control_path_violation_as(repo_root, &branch_ref(target), &branch_ref(branch), branch)
}

/// [`control_path_violation`] with the reason naming `label` instead of the revision.
pub fn control_path_violation_as(
    repo_root: &Path,
    target_rev: &str,
    branch_rev: &str,
    label: &str,
) -> Result<Option<String>> {
    let git = WorktreeGit::discovered(repo_root);
    let base = merge_base(&git, target_rev, branch_rev)?;
    let changed = changed_paths(repo_root, &base, branch_rev)?;
    violation_reason(repo_root, &changed, label)
}

/// Check the exact change from `old` to merged `tree`, avoiding ambiguity
/// when history has several merge bases.
pub fn tree_change_violation(
    repo_root: &Path,
    old: &str,
    tree: &str,
    label: &str,
) -> Result<Option<String>> {
    let changed = changed_paths(repo_root, old, tree)?;
    violation_reason(repo_root, &changed, label)
}

/// The human-review reason naming the control paths among `changed`, if any.
fn violation_reason(repo_root: &Path, changed: &[String], label: &str) -> Result<Option<String>> {
    let hooks_prefix = hooks_dir_prefix_checked(repo_root)?;
    let offending: Vec<String> = changed
        .iter()
        .filter(|path| is_control_path(path, hooks_prefix.as_deref()))
        // Debug-escaped, so a control character in a path cannot break the
        // one-line reason.
        .map(|path| format!("{path:?}"))
        .collect();
    if offending.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!(
        "branch {label} touches control path(s) requiring human review: {}",
        offending.join(", ")
    )))
}

/// Sorted UTF-8 paths differing between `base` and `branch_rev`, with whitespace
/// preserved and renames listed as deletion/addition. Gitlinks stay listed even
/// when `.gitmodules` ignores the submodule; revisions are separated from paths.
pub(crate) fn changed_paths(repo_root: &Path, base: &str, branch_rev: &str) -> Result<Vec<String>> {
    changed_paths_with(&WorktreeGit::discovered(repo_root), base, branch_rev)
}

pub(crate) fn changed_paths_with(
    git: &WorktreeGit,
    base: &str,
    branch_rev: &str,
) -> Result<Vec<String>> {
    let query = DiffNames {
        from: base,
        to: Some(branch_rev),
        filter: None,
        all_submodules: true,
        pathspecs: &[],
    };
    let paths = changed_names(git, &query)
        .with_context(|| format!("git diff {base} {branch_rev} failed"))?;
    Ok(paths.into_iter().collect())
}

/// Where the change a stage merge brought to `target` starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AcceptedBase {
    /// An accepted second-parent merge; `base` is `merge-base <merge>^1 <completed>`.
    Merged { merge: String, base: String },
    /// No accepted merge: `base` is `merge-base <completed> <target>`, either
    /// the fork point or `completed` itself if it reached `target` another way.
    Unmerged { base: String },
}

/// Locate the accepted merge and change base for `completed` on `target`.
/// `merge_stage` writes `[old target tip, stage tip]`; match the second parent.
pub(crate) fn accepted_change_base(
    repo_root: &Path,
    completed: &str,
    target: &str,
) -> Result<AcceptedBase> {
    if !is_commit_id(completed) {
        bail!("{completed:?} is not a full commit id");
    }
    let git = WorktreeGit::discovered(repo_root);
    let target_ref = branch_ref(target);
    let range = format!("{completed}..{target_ref}");
    let listing = crate::git::run_git_checked(
        &[
            "rev-list",
            "--first-parent",
            "--merges",
            "--parents",
            "--ancestry-path",
            &range,
        ],
        repo_root,
    )?;
    let merge = listing.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let commit = fields.next()?;
        fields
            .nth(1)
            .filter(|parent| parent.eq_ignore_ascii_case(completed))
            .map(|_| commit.to_string())
    });
    match merge {
        Some(merge) => {
            let first_parent = format!("{merge}^1");
            let base = merge_base(&git, &first_parent, completed)?;
            Ok(AcceptedBase::Merged { merge, base })
        }
        None => {
            let base = merge_base(&git, completed, &target_ref)?;
            Ok(AcceptedBase::Unmerged { base })
        }
    }
}

/// Whether `value` is a full hex commit id, safe from git's option parser.
fn is_commit_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Whether a repository-relative, forward-slashed `path` controls `.claude`,
/// `.mcp.json`, `.loom`, or tracked hooks (including directory descendants).
/// Hook ancestors are controlled because a symlink can redirect them.
/// Match ASCII case-insensitively for case-insensitive filesystems.
pub fn is_control_path(path: &str, hooks_prefix: Option<&str>) -> bool {
    let path = path.to_ascii_lowercase();
    let at_or_under = |dir: &str| {
        path.strip_prefix(dir)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    };
    path == ".mcp.json"
        || at_or_under(".claude")
        || at_or_under(".loom")
        || hooks_prefix.is_some_and(|prefix| {
            let dir = prefix.trim_end_matches('/').to_ascii_lowercase();
            let dir = dir.strip_prefix("./").unwrap_or(&dir);
            dir.is_empty()
                || at_or_under(dir)
                || dir
                    .strip_prefix(&path)
                    .is_some_and(|rest| rest.starts_with('/'))
        })
}

/// The repository-relative hooks prefix, or `None` when unset or outside it.
/// Errors conservatively control every path; merge guards use the checked form.
/// Symlink ancestors require review. Worktree configuration outranks local.
pub fn hooks_dir_prefix(repo_root: &Path) -> Option<String> {
    hooks_dir_prefix_checked(repo_root).unwrap_or_else(|_| Some(String::new()))
}

/// Checked form used by gates so path and configuration errors hold the change.
pub(crate) fn hooks_dir_prefix_checked(repo_root: &Path) -> Result<Option<String>> {
    // The runner's own -c core.hooksPath=/dev/null precludes an unscoped read.
    // `--type=bool` applies git's own reading: yes, on and 1 enable the extension.
    let worktree = config_value(
        repo_root,
        &["--local", "--type=bool"],
        "extensions.worktreeConfig",
    )?
    .is_some_and(|value| value == "true");
    let scopes: &[&str] = if worktree {
        &["--worktree", "--local", "--global", "--system"]
    } else {
        &["--local", "--global", "--system"]
    };
    for scope in scopes {
        // `--type=path` expands `~/` as git does when it runs a hook.
        let typed = [*scope, "--type=path"];
        if let Some(value) = config_value(repo_root, &typed, "core.hooksPath")? {
            return resolve_hooks_dir_prefix_checked(repo_root, Some(&value), None, None);
        }
    }
    Ok(None)
}

fn config_value(repo_root: &Path, scope: &[&str], key: &str) -> Result<Option<String>> {
    let mut args = vec!["config"];
    args.extend_from_slice(scope);
    args.extend(["--includes", "--get", key]);
    let output = run_git(&args, repo_root)?;
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    if !output.status.success() {
        bail!(
            "git config {} --get {key} failed: {}",
            scope.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let value = String::from_utf8(output.stdout).context("git config returned invalid UTF-8")?;
    Ok(Some(value.trim().to_string()).filter(|value| !value.is_empty()))
}

/// Resolve the first configured `local`, `global`, or `system` hooks path to
/// a repository-relative prefix ending in `/`. Unset or outside paths give
/// `None`; the repository root and errors give an empty prefix, controlling
/// every path. Use the checked form to distinguish errors.
pub fn resolve_hooks_dir_prefix(
    repo_root: &Path,
    local: Option<&str>,
    global: Option<&str>,
    system: Option<&str>,
) -> Option<String> {
    resolve_hooks_dir_prefix_checked(repo_root, local, global, system)
        .unwrap_or_else(|_| Some(String::new()))
}

pub(crate) fn resolve_hooks_dir_prefix_checked(
    repo_root: &Path,
    local: Option<&str>,
    global: Option<&str>,
    system: Option<&str>,
) -> Result<Option<String>> {
    let Some(configured) = local.or(global).or(system) else {
        return Ok(None);
    };
    let path = Path::new(configured);
    let root = std::fs::canonicalize(repo_root)
        .with_context(|| format!("Failed to resolve repository root: {}", repo_root.display()))?;
    let relative = if path.is_absolute() {
        if let Ok(relative) = path.strip_prefix(repo_root) {
            relative
        } else if let Ok(relative) = path.strip_prefix(&root) {
            relative
        } else {
            return Ok(None);
        }
    } else {
        path
    };
    normalized_hooks_path(&root, relative)
}

fn normalized_hooks_path(root: &Path, relative: &Path) -> Result<Option<String>> {
    let normalized = normalize_lexically(relative)
        .context("core.hooksPath climbs out of the repository; review it before merging")?;
    // Inspect the original components too: `link/..` must still refuse the link.
    let mut resolved = root.to_path_buf();
    for part in relative.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => {
                resolved.push(name);
                match std::fs::symlink_metadata(&resolved) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        bail!(
                            "core.hooksPath has a symlink ancestor at {}; review it before merging",
                            resolved.display()
                        );
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!("Failed to inspect core.hooksPath at {}", resolved.display())
                        })
                    }
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                bail!("invalid core.hooksPath: {}", relative.display())
            }
        }
    }
    let relative = normalized.to_str().context("core.hooksPath is not UTF-8")?;
    Ok(Some(if relative.is_empty() {
        String::new()
    } else {
        format!("{relative}/")
    }))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "control_paths/tests_accepted.rs"]
mod tests_accepted;

#[cfg(test)]
#[path = "control_paths/tests_worktree_config.rs"]
mod tests_worktree_config;

#[cfg(test)]
mod containment_tests {
    use super::*;

    #[test]
    fn hooks_paths_that_climb_out_require_review() {
        let root = tempfile::tempdir().unwrap();
        for relative in ["../hooks", "x/../../hooks", "../repo/hooks"] {
            let error = normalized_hooks_path(root.path(), Path::new(relative)).unwrap_err();
            assert_eq!(
                error.to_string(),
                "core.hooksPath climbs out of the repository; review it before merging"
            );
        }
    }

    #[test]
    fn lexical_normalization_keeps_cancelled_symlink_checks() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
        let error = normalized_hooks_path(root.path(), Path::new("link/../hooks")).unwrap_err();
        assert!(error.to_string().contains("symlink ancestor"));
    }
}
