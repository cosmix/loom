//! Merge gate: refuses to auto-merge, or hand to a merge-resolution session,
//! a stage branch whose diff since it split from the target touches a
//! control path — `.claude/`, `.mcp.json`, `.loom/`, or the tracked git
//! hooks directory. Owner decision 9, `doc/plans/PLAN-loom-state-confinement.md`
//! §9. Call sites: `merge_handler.rs`'s `try_auto_merge` (before ever
//! attempting the merge, so a conflicting branch never gets that far either)
//! and `spawn_merge_resolution_sessions` (for a stage that reached
//! `MergeConflict`/`MergeBlocked` some other way, e.g. `loom stage complete`).

use std::path::Path;

use anyhow::Result;

use crate::git::branch::branch_name_for_stage;
use crate::models::session::SessionType;
use crate::orchestrator::core::persistence::Persistence;
use crate::orchestrator::core::Orchestrator;
use crate::orchestrator::signals::remove_signal;
use crate::process::is_process_alive;

impl Orchestrator {
    /// Returns true — after routing `stage_id` to `NeedsHumanReview` — when
    /// `stage_branch`'s diff since it split from `target_branch` touches a
    /// control path.
    pub(super) fn merge_gate_blocks(
        &mut self,
        stage_id: &str,
        stage_branch: &str,
        target_branch: &str,
    ) -> bool {
        let violation =
            match control_path_violation(&self.config.repo_root, target_branch, stage_branch) {
                Ok(violation) => violation,
                Err(error) => {
                    tracing::warn!(
                        stage_id = %stage_id,
                        %error,
                        "merge gate: failed to compute changed paths; proceeding with merge attempt"
                    );
                    return false;
                }
            };
        let Some(reason) = violation else {
            return false;
        };
        self.route_to_human_review(stage_id, reason, None);
        true
    }

    /// Persists `completed_commit` from branch HEAD before a merge attempt,
    /// so ancestry can be verified even if the merge later conflicts.
    /// Relocated out of `try_auto_merge` to keep it within its
    /// maintainability-ledger line budget — unrelated to the gate itself.
    pub(super) fn capture_completed_commit(
        &mut self,
        stage: &mut crate::models::stage::Stage,
        stage_id: &str,
    ) {
        if stage.completed_commit.is_some() {
            return;
        }
        let branch_name = branch_name_for_stage(stage_id);
        let Ok(head) = crate::git::get_branch_head(&branch_name, &self.config.repo_root) else {
            return;
        };
        match self.update_stage(stage_id, |current| {
            if current.completed_commit.is_none() {
                current.completed_commit = Some(head);
            }
            Ok(())
        }) {
            Ok(updated) => *stage = updated,
            Err(error) => eprintln!("Warning: Failed to save completed_commit: {error}"),
        }
    }

    /// Cleans up a stale/dead tracked session for `stage_id` (relocated out
    /// of `spawn_merge_resolution_sessions` to stay within its
    /// maintainability-ledger line budget — unrelated to the gate itself).
    /// Returns true if a live merge/base-conflict resolver session is
    /// already running and the caller should skip spawning a new one.
    ///
    /// When `loom stage complete` detects a merge conflict, the stage
    /// transitions to MergeConflict and `spawn_merge_resolver()` returns
    /// DaemonManaged (no session spawned). However, the original execution
    /// session (`SessionType::Stage`) may still be alive in
    /// `active_sessions` — it hasn't exited yet — and will never resolve the
    /// merge conflict, so it must not block merge resolver spawning: a
    /// tracked Stage session is always stale here and is torn down; a
    /// tracked Merge (or base-conflict) session is left alone while its
    /// process is still alive.
    pub(super) fn cleanup_stale_merge_session(&mut self, stage_id: &str) -> bool {
        let Some(session) = self.active_sessions.get(stage_id) else {
            return false;
        };
        if session.session_type != SessionType::Stage
            && session.pid.map(is_process_alive).unwrap_or(false)
        {
            return true;
        }
        let stale_session = self.active_sessions.remove(stage_id).unwrap();
        let stale_session_id = stale_session.id.clone();
        // Kill the original session to prevent zombie processes.
        if let Err(e) = self.backend.kill_session(&stale_session) {
            tracing::debug!(
                session_id = %stale_session_id,
                error = %e,
                "Failed to kill stale session (may already be dead)"
            );
        }
        // Remove the old signal file so it doesn't block respawning.
        if let Err(e) = remove_signal(&stale_session_id, &self.config.work_dir) {
            eprintln!(
                "Warning: Failed to remove stale signal for session '{stale_session_id}': {e}"
            );
        }
        false
    }
}

/// A human-review reason when `stage_branch`'s diff since it split from
/// `target_branch` touches a control path, else `None`.
fn control_path_violation(
    repo_root: &Path,
    target_branch: &str,
    stage_branch: &str,
) -> Result<Option<String>> {
    let merge_base =
        crate::git::run_git_checked(&["merge-base", target_branch, stage_branch], repo_root)?;
    let diff = crate::git::run_git_checked(
        &[
            "diff",
            "--name-only",
            "--no-renames",
            merge_base.as_str(),
            stage_branch,
        ],
        repo_root,
    )?;
    let hooks_prefix = hooks_dir_prefix(repo_root);
    let offending: Vec<&str> = diff
        .lines()
        .filter(|path| is_control_path(path, hooks_prefix.as_deref()))
        .collect();
    if offending.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!(
        "branch {stage_branch} touches control path(s) requiring human review: {}",
        offending.join(", ")
    )))
}

/// Whether `path` (repository-relative, forward-slashed, as `git diff
/// --name-only` reports it) is a control path: `.claude/`, `.mcp.json`,
/// `.loom/`, or under the tracked git hooks directory.
fn is_control_path(path: &str, hooks_prefix: Option<&str>) -> bool {
    path.starts_with(".claude/")
        || path == ".mcp.json"
        || path.starts_with(".loom/")
        || hooks_prefix.is_some_and(|prefix| path.starts_with(prefix))
}

/// The repo-relative prefix of the tracked git hooks directory
/// (`core.hooksPath`), or `None` when it is unset at every scope or resolves
/// outside the repository.
fn hooks_dir_prefix(repo_root: &Path) -> Option<String> {
    // Scoped reads, in git's own precedence order. An unscoped `--get` would
    // let `run_git_checked`'s `NO_HOOKS_ARGS` (`-c core.hooksPath=/dev/null`,
    // applied to every git command loom runs) win on precedence, and this
    // query would always read back `/dev/null` instead of the real value.
    let local = read_hooks_path_scope(repo_root, "--local");
    let global = read_hooks_path_scope(repo_root, "--global");
    let system = read_hooks_path_scope(repo_root, "--system");
    resolve_hooks_dir_prefix(
        repo_root,
        local.as_deref(),
        global.as_deref(),
        system.as_deref(),
    )
}

/// `git config <scope> --get core.hooksPath` in `repo_root`, or `None` when
/// unset at that scope or the read fails.
fn read_hooks_path_scope(repo_root: &Path, scope: &str) -> Option<String> {
    let configured =
        crate::git::run_git_checked(&["config", scope, "--get", "core.hooksPath"], repo_root)
            .ok()?;
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Resolves `core.hooksPath` from the three config scopes to a repo-relative
/// prefix ending in `/`, taking the first of `local`, `global`, `system` that
/// is set — git's own precedence order, so a global value (even a relative
/// one, which applies inside every repository) is used only when local is
/// unset. `None` when every scope is unset, or the winning value is an
/// absolute path outside `repo_root`.
fn resolve_hooks_dir_prefix(
    repo_root: &Path,
    local: Option<&str>,
    global: Option<&str>,
    system: Option<&str>,
) -> Option<String> {
    let configured = local.or(global).or(system)?;
    let path = Path::new(configured);
    let relative = if path.is_absolute() {
        path.strip_prefix(repo_root).ok()?.to_str()?.to_string()
    } else {
        configured.to_string()
    };
    Some(format!("{}/", relative.trim_end_matches('/')))
}

#[cfg(test)]
#[path = "merge_gate_tests.rs"]
mod tests;
