//! Permission fold-back from a worktree's settings into the loom-owned
//! approved-permissions list.
//!
//! When Claude Code sessions run in worktrees, they may be granted additional
//! permissions that are stored in the worktree's settings.local.json. This
//! module reads those permissions (and the main repository's own local allow
//! list, which Claude Code writes approvals into even for a capsule-launched
//! session — see `approved.rs`) and records the portable ones into the
//! loom-owned approved-permissions list, filtering out worktree-specific
//! paths. It never writes the main repository's or a worktree's
//! `.claude/settings.local.json` (owner decision 8,
//! `doc/plans/PLAN-loom-state-confinement.md`).

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::write_rules::is_inert_write_permission;

/// Patterns that indicate a worktree-specific permission that should not be synced.
///
/// Any permission whose path argument starts with `../` (single or double level)
/// is worktree-specific: the path resolves relative to the settings file location,
/// which in a worktree is `.worktrees/<stage-id>/`, so `../` already escapes the
/// worktree root. Merging such paths verbatim into the main-repo settings resolves
/// to a completely different — and usually dangerous — location.
const WORKTREE_PATH_PATTERNS: &[&str] = &["../", ".worktrees/"];

/// Fold a worktree's approved permissions back into the loom-owned
/// approved-permissions list.
///
/// This function:
/// 1. Reads the worktree's settings.local.json (from both worktree root and working_dir)
/// 2. Extracts the permissions.allow array
/// 3. Normalizes a single-leading-slash path to its cwd-relative form (see
///    `normalize_single_slash_path`), drops inert `Write(...)` rules (see
///    `is_inert_write_permission`), and rewrites worktree-specific paths
///    (containing ../../ or .worktrees/) to their portable form
/// 4. Records the portable rules, plus the main repository's own local allow
///    list, into `approved.rs`'s loom-owned list, through its control-surface
///    filter
///
/// # Arguments
/// * `worktree_path` - The root path of the worktree (`.worktrees/<stage-id>`)
/// * `main_repo_path` - The root path of the main repository
/// * `working_dir` - Optional working directory where Claude Code session ran
///   (e.g., `<worktree>/loom` for Rust projects). Claude Code writes permissions
///   to .claude/settings.local.json relative to the cwd, so we need to check
///   both the worktree root and the working directory for permissions.
pub fn sync_worktree_permissions(
    worktree_path: &Path,
    main_repo_path: &Path,
) -> Result<SyncResult> {
    sync_worktree_permissions_with_working_dir(worktree_path, main_repo_path, None)
}

/// The settings files a worktree session's permissions may live in: the
/// worktree root's; the session's working directory's when it differs
/// (Claude Code writes permissions relative to the cwd, so a session run from
/// `worktree/loom/` keeps them in `worktree/loom/.claude/settings.local.json`);
/// and those of the common subdirectories a session usually runs from, for
/// callers that pass no working directory.
fn settings_paths_to_check(worktree_path: &Path, working_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut paths_to_check = vec![worktree_path.join(".claude/settings.local.json")];
    if let Some(wd) = working_dir {
        if wd != worktree_path {
            let wd_settings = wd.join(".claude/settings.local.json");
            if wd_settings.exists() && !paths_to_check.contains(&wd_settings) {
                paths_to_check.push(wd_settings);
            }
        }
    }
    for subdir in ["loom", "src", "app", "packages", "workspace"] {
        let subdir_settings = worktree_path
            .join(subdir)
            .join(".claude/settings.local.json");
        if subdir_settings.exists() && !paths_to_check.contains(&subdir_settings) {
            paths_to_check.push(subdir_settings);
        }
    }
    paths_to_check
}

/// Feed the loom-owned approved list (`approved.rs`, owner decision 8): the
/// fold-back's own portable allow rules, plus the main repository's local
/// allow list. Claude Code records a "don't ask again" approval there
/// (destination `localSettings`, rooted at the operator-owned checkout) even
/// for a worktree session launched with `--setting-sources user,project`, so
/// the worktree's own file alone would miss it. Returns how many rules were
/// newly recorded.
fn record_approvals(
    main_repo_path: &Path,
    main_settings_path: &Path,
    worktree_allow: &[String],
) -> usize {
    let mut rules = worktree_allow.to_vec();
    match read_settings(main_settings_path) {
        Ok(settings) => rules.extend(portable_permissions(extract_permissions(&settings).0)),
        Err(error) => tracing::warn!(
            %error,
            "cannot read the main repository's local settings for approved permissions"
        ),
    }
    super::approved::record_fold_back(main_repo_path, &rules)
}

/// Sync permissions with an explicit working directory.
///
/// Reads the worktree's `.claude/settings.local.json` (and the working
/// directory's, and the main repository's local allow list), and records the
/// portable allow rules into the loom-owned approved-permissions list
/// (`approved.rs`). Never writes `main_repo_path`'s or the worktree's
/// `.claude/settings.local.json` — Claude Code owns those files.
pub fn sync_worktree_permissions_with_working_dir(
    worktree_path: &Path,
    main_repo_path: &Path,
    working_dir: Option<&Path>,
) -> Result<SyncResult> {
    let main_settings_path = main_repo_path.join(".claude/settings.local.json");

    // Collect allow permissions from every settings file a session's
    // approvals could have landed in.
    let mut all_allow_perms: Vec<String> = Vec::new();
    for settings_path in &settings_paths_to_check(worktree_path, working_dir) {
        let worktree_settings = read_settings(settings_path)?;
        all_allow_perms.extend(extract_permissions(&worktree_settings).0);
    }

    // Deduplicate
    all_allow_perms.sort();
    all_allow_perms.dedup();

    let filtered_allow = portable_permissions(all_allow_perms);
    let allow_added = record_approvals(main_repo_path, &main_settings_path, &filtered_allow);

    Ok(SyncResult {
        allow_added,
        // No destination for a propagated deny rule since loom stopped
        // writing `.claude/settings.local.json`; kept for API compatibility.
        deny_added: 0,
        // No sibling-worktree propagation left to do: the loom-owned
        // approved list is read fresh into every capsule instead.
        worktrees_updated: 0,
    })
}

/// Result of a permission sync operation
#[derive(Debug, Default)]
pub struct SyncResult {
    /// Number of allow permissions newly recorded into the loom-owned
    /// approved-permissions list.
    pub allow_added: usize,
    /// Always 0: no destination remains for a propagated deny rule.
    pub deny_added: usize,
    /// Always 0: sibling worktrees now read approvals fresh from the
    /// loom-owned list at spawn instead of being propagated to directly.
    pub worktrees_updated: usize,
}

/// Read and parse a settings.json file
fn read_settings(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }

    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {} as JSON", path.display()))
}

/// Extract allow and deny permission arrays from settings
fn extract_permissions(settings: &Value) -> (Vec<String>, Vec<String>) {
    let permissions = settings.get("permissions");

    let allow = permissions
        .and_then(|p| p.get("allow"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let deny = permissions
        .and_then(|p| p.get("deny"))
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    (allow, deny)
}

/// Normalize a single-leading-slash path to its cwd-relative form, drop inert
/// `Write(...)` rules, then rewrite worktree-specific paths to their portable
/// form, keeping every other rule as it is.
fn portable_permissions(perms: Vec<String>) -> Vec<String> {
    perms
        .into_iter()
        .map(|p| normalize_single_slash_path(&p))
        .filter(|p| !is_inert_write_permission(p))
        .filter_map(|p| {
            if is_worktree_specific_permission(&p) {
                transform_worktree_path(&p)
            } else {
                Some(p)
            }
        })
        .collect()
}

/// Permission types whose argument is a filesystem path, per Claude Code's
/// path-syntax rules (`permissions.md#read-and-edit`).
const PATH_ARG_PERMISSION_TYPES: &[&str] = &["Edit", "Read", "Write", "NotebookEdit"];

/// Rewrite a path-argument rule whose path starts with exactly one `/` into
/// its cwd-relative form, dropping that slash: `Edit(/src/**)` becomes
/// `Edit(src/**)`.
///
/// A single leading `/` resolves relative to the settings file that recorded
/// it — the recording checkout's root, not wherever the rule is rendered
/// later. Copied verbatim into a capsule under `W/capsules/`, it would
/// resolve there instead and silently grant the wrong path, with the
/// approving session prompted again on a later one. Dropping the slash makes
/// the rule cwd-relative, so a later session applies it to its own checkout.
///
/// `//path` (absolute from the filesystem root), `~/path`, a bare or `./`
/// relative path, and any rule whose type is not in
/// `PATH_ARG_PERMISSION_TYPES` (e.g. `Bash(cargo test:*)`) are returned
/// unchanged.
fn normalize_single_slash_path(permission: &str) -> String {
    let Some(open_paren) = permission.find('(') else {
        return permission.to_string();
    };
    let Some(close_paren) = permission.rfind(')') else {
        return permission.to_string();
    };
    if close_paren <= open_paren {
        return permission.to_string();
    }
    let perm_type = &permission[..open_paren];
    if !PATH_ARG_PERMISSION_TYPES.contains(&perm_type) {
        return permission.to_string();
    }
    let path_str = &permission[open_paren + 1..close_paren];
    match path_str.strip_prefix('/') {
        Some(rest) if !rest.starts_with('/') => format!("{perm_type}({rest})"),
        _ => permission.to_string(),
    }
}

/// Check if a permission string contains worktree-specific path patterns
fn is_worktree_specific_permission(permission: &str) -> bool {
    WORKTREE_PATH_PATTERNS
        .iter()
        .any(|pattern| permission.contains(pattern))
}

/// Transform a worktree-specific permission path to a portable path
///
/// Returns Some(transformed_permission) if the path was transformed,
/// None if the permission doesn't contain worktree-specific patterns or can't be transformed.
///
/// # Examples
/// - `Read(/home/x/.worktrees/s1/loom/src/**)` → `Read(loom/src/**)`
/// - `Read(../../../.loom/work/**)` → `Read(.loom/work/**)`
/// - `Write(../../doc/plans/**)` → `Write(doc/plans/**)`
fn transform_worktree_path(permission: &str) -> Option<String> {
    // Only transform if it contains worktree-specific patterns
    if !is_worktree_specific_permission(permission) {
        return None;
    }

    // Extract permission type and path: "Read(path)" -> ("Read", "path")
    let open_paren = permission.find('(')?;
    let close_paren = permission.rfind(')')?;
    if close_paren <= open_paren {
        return None;
    }

    let perm_type = &permission[..open_paren];
    let path_str = &permission[open_paren + 1..close_paren];

    // Try to transform the path
    let transformed_path = if let Some(worktrees_idx) = path_str.find(".worktrees/") {
        // Only transform if .worktrees/ is NOT at the start of the path.
        // If at start (position 0), it's a relative path to the worktrees directory
        // (e.g., .worktrees/stage-1/**) which references sibling worktrees and
        // doesn't map cleanly to main repo paths - filter these out.
        if worktrees_idx == 0 {
            None
        } else {
            // Handle absolute path with .worktrees/stage-id/
            // e.g., /home/user/.worktrees/stage-1/loom/src/** -> loom/src/**
            let after_worktrees = &path_str[worktrees_idx + ".worktrees/".len()..];
            // Skip stage-id (everything up to next /)
            if let Some(stage_sep) = after_worktrees.find('/') {
                let portable_path = &after_worktrees[stage_sep + 1..];
                if portable_path.is_empty() {
                    None
                } else {
                    Some(portable_path.to_string())
                }
            } else {
                None
            }
        }
    } else if path_str.starts_with("../") {
        // Handle any relative path that starts with ../ (single or multiple levels).
        // Resolve by stripping all ../ prefixes.
        // ../doc/**     -> doc/**
        // ../../../.loom/work/** -> .loom/work/**
        let mut path = path_str;
        while path.starts_with("../") {
            path = &path[3..];
        }
        if path.is_empty() || path == "**" {
            // Filter out bare "**" — this results from stripping ../
            // prefixes from escape-prevention rules like "../../**",
            // and would match everything if synced to the main repo.
            None
        } else {
            Some(path.to_string())
        }
    } else {
        // Contains .worktrees/ pattern but not at start of path or ../../
        // This might be a more complex case, return None to keep original behavior
        None
    };

    transformed_path.map(|p| format!("{}({})", perm_type, p))
}

#[cfg(test)]
mod tests;
