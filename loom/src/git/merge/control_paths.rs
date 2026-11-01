//! Control paths: files a stage branch may change only with a person's review.
//!
//! A branch whose diff since it split from the target touches `.claude/`,
//! `.mcp.json`, `.loom/`, or the tracked git hooks directory
//! (`core.hooksPath`) is never merged automatically; owner decision 9,
//! `doc/plans/PLAN-loom-state-confinement.md` §9. [`merge_stage`] enforces
//! the gate on the exact commits it merges; the daemon's early filters call
//! [`control_path_violation`] too.
//!
//! [`merge_stage`]: super::merge_stage

use std::path::{Component, Path};

use anyhow::{bail, Context, Result};

use crate::git::branch::branch_ref;
use crate::git::runner::run_git;

/// Whether `merge_stage` checks the control-path gate on the commits it is
/// about to merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeGate {
    /// Refuse a branch that touches a control path ([`MergeResult::Held`]).
    ///
    /// [`MergeResult::Held`]: super::MergeResult::Held
    Enforce,
    /// Merge without the gate: the operator reviewed the branch
    /// (`loom stage human-review --force-complete`).
    Bypass,
}

/// A human-review reason when the diff of branch `branch` since it split from
/// branch `target` touches a control path, else `None`. The reason names
/// `branch`. Both are branch names, resolved as `refs/heads/<name>` so a tag
/// of the same name cannot stand in. An error means the diff could not be
/// computed; callers that guard a merge treat it as a refusal.
pub fn control_path_violation(
    repo_root: &Path,
    target: &str,
    branch: &str,
) -> Result<Option<String>> {
    control_path_violation_as(repo_root, &branch_ref(target), &branch_ref(branch), branch)
}

/// [`control_path_violation`] with the reason naming `label` (a branch name)
/// instead of the revision that was diffed.
pub fn control_path_violation_as(
    repo_root: &Path,
    target_rev: &str,
    branch_rev: &str,
    label: &str,
) -> Result<Option<String>> {
    let base = crate::git::run_git_checked(&["merge-base", target_rev, branch_rev], repo_root)?;
    let changed = changed_paths(repo_root, &base, branch_rev)?;
    violation_reason(repo_root, &changed, label)
}

/// [`control_path_violation_as`] over the exact change a merge would land:
/// the diff from `old` to the merged `tree`. The merge-base diff alone is
/// ambiguous on a history with several merge bases.
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

/// Paths that differ between `base` and `branch_rev`, unquoted: `-z` output is
/// NUL-separated and git never C-quotes it. Read untrimmed, so a path's own
/// leading or trailing whitespace survives. Gitlink changes are always listed:
/// `--ignore-submodules=none` overrides an `ignore` setting in the checkout's
/// `.gitmodules`, which a merged branch can set. The trailing `--` keeps a
/// file named like a revision from making git refuse the call as ambiguous.
pub(crate) fn changed_paths(repo_root: &Path, base: &str, branch_rev: &str) -> Result<Vec<String>> {
    let args = [
        "diff",
        "-z",
        "--name-only",
        "--no-renames",
        "--ignore-submodules=none",
        base,
        branch_rev,
        "--",
    ];
    let output = run_git(&args, repo_root)?;
    if !output.status.success() {
        bail!(
            "git diff {base} {branch_rev} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect())
}

/// Where the change a stage merge brought to `target` starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AcceptedBase {
    /// The first-parent history of `target` holds `merge`, whose second
    /// parent is the stage commit; `base` is `merge-base <merge>^1 <completed>`.
    Merged { merge: String, base: String },
    /// No such merge: `base` is `merge-base <completed> <target>`. That is
    /// the fork point when the stage has not merged yet (a dependency used
    /// as a base branch), and `completed` itself when the commit already
    /// reached `target` another way; the caller treats the second as unknown.
    Unmerged { base: String },
}

/// Locates the merge that brought stage commit `completed` to branch `target`
/// and the base its change starts from. `merge_stage` writes the merge as
/// `[old target tip, stage tip]`, so the match is on the second parent.
pub(crate) fn accepted_change_base(
    repo_root: &Path,
    completed: &str,
    target: &str,
) -> Result<AcceptedBase> {
    if !is_commit_id(completed) {
        bail!("{completed:?} is not a full commit id");
    }
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
            let base =
                crate::git::run_git_checked(&["merge-base", &first_parent, completed], repo_root)?;
            Ok(AcceptedBase::Merged { merge, base })
        }
        None => {
            let base =
                crate::git::run_git_checked(&["merge-base", completed, &target_ref], repo_root)?;
            Ok(AcceptedBase::Unmerged { base })
        }
    }
}

/// Whether `value` is a full SHA-1 or SHA-256 commit id (hex only, so it can
/// never reach git's option parser).
fn is_commit_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Whether `path` (repository-relative, forward-slashed, as `git diff -z
/// --name-only` reports it) is a control path: `.claude`, `.mcp.json`,
/// `.loom`, the tracked git hooks directory, or anything under a directory
/// among them. An ancestor of the hooks directory is also controlled because
/// replacing it with a symlink can redirect hooks. ASCII-case-insensitive,
/// since a case-insensitive filesystem
/// resolves `.CLAUDE/settings.json` to `.claude/settings.json`.
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

/// The repo-relative prefix of the tracked git hooks directory
/// (`core.hooksPath`), or `None` when it is unset at every scope or resolves
/// outside the repository. On a query error, returns the root prefix so a
/// caller using this legacy `Option` API classifies every path as controlled.
/// The stage and target guards use the checked form and hold on that error.
/// Existing symlink ancestors are errors because one prefix cannot safely
/// cover both a link and its destination.
///
/// A `config.worktree` value outranks the local one, as in
/// `configured_hooks_path`: git runs the hooks it names.
pub fn hooks_dir_prefix(repo_root: &Path) -> Option<String> {
    hooks_dir_prefix_checked(repo_root).unwrap_or_else(|_| Some(String::new()))
}

/// Checked form used by gates so path and configuration errors hold the change.
pub(crate) fn hooks_dir_prefix_checked(repo_root: &Path) -> Result<Option<String>> {
    // The runner's own -c core.hooksPath=/dev/null precludes an unscoped read.
    let worktree = config_value(repo_root, "--local", "extensions.worktreeConfig")?
        .is_some_and(|value| value.eq_ignore_ascii_case("true"));
    let scopes: &[&str] = if worktree {
        &["--worktree", "--local", "--global", "--system"]
    } else {
        &["--local", "--global", "--system"]
    };
    for scope in scopes {
        if let Some(value) = config_value(repo_root, scope, "core.hooksPath")? {
            return resolve_hooks_dir_prefix_checked(repo_root, Some(&value), None, None);
        }
    }
    Ok(None)
}

fn config_value(repo_root: &Path, scope: &str, key: &str) -> Result<Option<String>> {
    let output = run_git(&["config", scope, "--includes", "--get", key], repo_root)?;
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    if !output.status.success() {
        bail!(
            "git config {scope} --get {key} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let value = String::from_utf8(output.stdout).context("git config returned invalid UTF-8")?;
    Ok(Some(value.trim().to_string()).filter(|value| !value.is_empty()))
}

/// Resolves `core.hooksPath` from the three config scopes to a repo-relative
/// prefix ending in `/`, taking the first of `local`, `global`, `system` that
/// is set — git's own precedence order, so a global value (even a relative
/// one, which applies inside every repository) is used only when local is
/// unset. `None` when every scope is unset, or the winning value is an
/// absolute path outside `repo_root`. An empty prefix denotes the repository
/// root, where every tracked path is in the hook directory. Query errors also
/// produce that conservative root prefix; use the checked form to distinguish.
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
    let Ok(relative) = resolved.strip_prefix(root) else {
        return Ok(None);
    };
    let relative = relative.to_str().context("core.hooksPath is not UTF-8")?;
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
