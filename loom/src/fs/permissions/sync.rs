//! Permission fold-back from the main checkout's own local settings into the
//! loom-owned approved-permissions list.
//!
//! Claude Code writes a user's "don't ask again" approvals into the main
//! checkout's `.claude/settings.local.json`, even for a capsule-launched
//! session (see `approved.rs`). That one file is the only source: a
//! worktree's settings file, or any subdirectory's under the main checkout,
//! is writable by a session, so reading it would let a session grant itself
//! rules every later capsule allows. This module never writes the main
//! repository's or a worktree's `.claude/settings.local.json` (owner
//! decision 8, `doc/plans/PLAN-loom-state-confinement.md`).

use serde_json::Value;
use std::path::Path;

use super::write_rules::is_inert_write_permission;

/// Patterns that indicate a worktree-specific permission that should not be synced.
///
/// Any permission whose path argument starts with `../` (single or double level)
/// is worktree-specific: the path resolves relative to the settings file location,
/// which in a worktree is `.worktrees/<stage-id>/`, so `../` already escapes the
/// worktree root. Merging such paths verbatim into the main-repo settings resolves
/// to a completely different — and usually dangerous — location.
const WORKTREE_PATH_PATTERNS: &[&str] = &["../", ".worktrees/"];

/// Upper bound on the main checkout's local settings file; anything larger is
/// refused and contributes nothing.
const MAX_SETTINGS_BYTES: usize = 1 << 20;

/// The main checkout's local settings file, relative to its root.
const MAIN_SETTINGS: &str = ".claude/settings.local.json";

/// Fold the main checkout's own local allow rules into the loom-owned
/// approved-permissions list and return how many were newly recorded.
///
/// Reads only `<main_repo_path>/.claude/settings.local.json`, never a
/// worktree's file or a subdirectory's, since those are writable by a
/// session. The read follows no symlink, refuses a non-regular file (a FIFO
/// cannot block it) and is capped at `MAX_SETTINGS_BYTES`. A missing file
/// means no rules; any other read or parse failure is logged and also means
/// no rules, so the fold-back never fails. The rules are normalized (see
/// `normalize_single_slash_path`), inert `Write(...)` rules are dropped (see
/// `is_inert_write_permission`), worktree-specific paths are rewritten to
/// their portable form, and the result is recorded through `approved.rs`'s
/// control-surface filter. Never writes either settings file.
pub fn fold_back_approvals(main_repo_path: &Path) -> usize {
    let mut rules =
        portable_permissions(extract_permissions(&read_main_settings(main_repo_path)).0);
    rules.sort();
    rules.dedup();
    super::approved::record_fold_back(main_repo_path, &rules)
}

/// The main checkout's parsed local settings, or `Value::Null` when the file
/// is absent, unreadable or not JSON.
fn read_main_settings(main_repo_path: &Path) -> Value {
    let relative = Path::new(MAIN_SETTINGS);
    let parsed =
        crate::fs::safe_read::read_to_string_bounded(main_repo_path, relative, MAX_SETTINGS_BYTES)
            .and_then(|text| serde_json::from_str::<Value>(&text).map_err(anyhow::Error::from));
    match parsed {
        Ok(settings) => settings,
        Err(error) if crate::fs::safe_read::is_not_found(&error) => Value::Null,
        Err(error) => {
            tracing::warn!(
                path = %main_repo_path.join(relative).display(),
                error = %format!("{error:#}"),
                "cannot read the main checkout's local settings for approved permissions"
            );
            Value::Null
        }
    }
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
