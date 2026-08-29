//! Worktree resolution and knowledge-stage sandbox/hook setup for spawn.
//!
//! Extracted from `stage_executor.rs` to keep that file under the
//! maintainability limit. Behavior is unchanged from before the move.

use anyhow::{Context, Result};

use crate::git;
use crate::git::BaseBranchError;
use crate::hooks::find_hooks_dir;
use crate::models::failure::FailureType;
use crate::models::stage::Stage;
use crate::orchestrator::scheduling_report::BlockReason;

use super::stage_executor::{install_required_hooks, write_required_sandbox_settings};
use super::Orchestrator;

impl Orchestrator {
    /// Resolve the base branch for a worktree-backed stage.
    ///
    /// Routes on the *typed* `BaseBranchError` variant rather than matching
    /// substrings of the error text (A-13): rewording a message can no
    /// longer silently reclassify a handled condition into a propagated
    /// error that exits the orchestrator loop.
    ///
    /// Returns `Ok(None)` for a transient scheduling condition — retry next
    /// poll, not an error. A real `BaseBranchError::Other` still propagates.
    pub(super) fn resolve_stage_base_branch(
        &mut self,
        stage_id: &str,
        stage: &Stage,
    ) -> Result<Option<git::ResolvedBase>> {
        match git::resolve_base_branch(
            stage_id,
            &stage.dependencies,
            &self.graph,
            &self.config.repo_root,
            self.config.base_branch.as_deref(),
        ) {
            Ok(resolved) => Ok(Some(resolved)),
            Err(BaseBranchError::SchedulingNotReady(msg)) => {
                // Transient — skip this cycle, retry on the next poll.
                // Logged once per stage per daemon run: this fires on every
                // 5-second poll for as long as the condition holds, and an
                // unbounded print here buries the log (and the operator) under
                // thousands of identical lines while the stage sits Queued.
                if self.spawn_skip_logged.insert(stage_id.to_string()) {
                    tracing::warn!(
                        stage_id = %stage_id,
                        reason = %msg,
                        "Stage skipped due to scheduling error; will retry each poll"
                    );
                }
                self.spawn_blocks.insert(
                    stage_id.to_string(),
                    BlockReason::SchedulingNotReady { detail: msg },
                );
                Ok(None)
            }
            Err(BaseBranchError::Other(e)) => Err(e)
                .with_context(|| format!("Failed to resolve base branch for stage: {stage_id}")),
        }
    }

    /// Resolve the base branch and create (or reuse) the worktree for a
    /// worktree-backed stage, BEFORE the stage is marked Executing.
    ///
    /// Returns `Ok(None)` if the spawn should stop here without an error —
    /// either a transient scheduling condition (see
    /// `resolve_stage_base_branch`) or a worktree failure that has already
    /// marked the stage Blocked.
    pub(super) fn resolve_worktree(
        &mut self,
        stage_id: &str,
        stage: &Stage,
    ) -> Result<Option<(git::ResolvedBase, crate::models::worktree::Worktree)>> {
        let Some(resolved) = self.resolve_stage_base_branch(stage_id, stage)? else {
            return Ok(None);
        };

        let worktree = match git::get_or_create_worktree(
            stage_id,
            &self.config.repo_root,
            Some(resolved.branch_name()),
        ) {
            Ok(wt) => wt,
            Err(e) => {
                let err_msg = format!("{e:#}");
                eprintln!("Stage '{stage_id}' blocked due to worktree error: {err_msg}");

                // Stage is Queued here and may transition directly to Blocked.
                let _ = self.persist_blocked_stage(
                    stage_id,
                    FailureType::InfrastructureError,
                    vec![err_msg],
                );
                return Ok(None);
            }
        };

        Ok(Some((resolved, worktree)))
    }

    /// Merge and validate this knowledge stage's sandbox config, then write
    /// the settings into the main repo (knowledge stages run on the host
    /// directly, so there is no worktree to write into instead).
    ///
    /// Returns `Ok(None)` if the stage was marked Blocked instead (invalid
    /// config); the caller should return without spawning.
    pub(super) fn write_knowledge_sandbox_settings(
        &mut self,
        stage: &Stage,
        stage_id: &str,
    ) -> Result<Option<crate::sandbox::MergedSandboxConfig>> {
        let mut merged_sandbox = crate::sandbox::merge_config(
            &self.config.sandbox_config,
            &stage.sandbox,
            stage.stage_type,
            &stage.implementers,
        );
        // Defense-in-depth: re-validate at spawn time even for knowledge stages.
        if let Err(e) = crate::sandbox::validate_config(&merged_sandbox) {
            let err_msg = format!("{e:#}");
            eprintln!(
                "Knowledge stage '{stage_id}' blocked: invalid sandbox config at spawn: {err_msg}"
            );
            let _ = self.persist_blocked_stage(
                stage_id,
                FailureType::InfrastructureError,
                vec![err_msg],
            );
            return Ok(None);
        }
        crate::sandbox::expand_paths(&mut merged_sandbox);
        // Knowledge stages share the host's main-repo `.claude/settings.local.json`
        // (the agent runs on the host directly), so the sandbox/permissions settings
        // must be written there.
        write_required_sandbox_settings(&merged_sandbox, &self.config.repo_root, stage_id)?;
        Ok(Some(merged_sandbox))
    }

    /// Install Claude Code hooks into the main repo for a knowledge-stage
    /// spawn (it has no worktree of its own; the main repo root is the
    /// install target), and drop `.claude/settings.local.json` from the main
    /// repo's gitignore so it cannot be accidentally committed.
    ///
    /// Session identity is deliberately NOT written into the hooks config:
    /// this file is shared by every main-repo session (later knowledge
    /// stages, interactive user sessions), so persisted stage/session IDs
    /// would go stale and shadow the wrapper script's fresh exports.
    ///
    /// Returns `Ok(false)` if the stage was marked Blocked instead (hook
    /// install failure); the caller should return without spawning.
    pub(super) fn install_knowledge_hooks(
        &mut self,
        stage_id: &str,
        session_id: &str,
        permission_mode: crate::plan::schema::PermissionMode,
    ) -> Result<bool> {
        // Claude Code hooks are the knowledge stage's security boundary, not
        // an optional enhancement: a session is never spawned without them.
        // Contain a setup failure as Blocked rather than propagating it, which
        // would kill the daemon while the stage sits Executing with no session.
        if let Err(e) = install_required_hooks(
            find_hooks_dir(),
            &self.config.repo_root,
            &self.config.work_dir,
            permission_mode,
            stage_id,
        ) {
            self.block_and_undo_session(
                stage_id,
                session_id,
                FailureType::SandboxSetupFailure,
                format!("{e:#}"),
            );
            return Ok(false);
        }

        // Exclude .claude/settings.local.json from the main repo's gitignore so knowledge-stage
        // hook configs cannot be accidentally committed.
        if let Err(e) =
            crate::git::worktree::add_settings_local_to_main_gitignore(&self.config.repo_root)
        {
            eprintln!("Warning: Failed to add settings.local.json to main repo gitignore: {e}");
        }

        Ok(true)
    }

    /// Write and install this knowledge stage's sandbox settings and hooks.
    ///
    /// Returns `Ok(false)` if the stage was marked Blocked instead (invalid
    /// sandbox config or hook install failure); the caller should return
    /// without spawning.
    pub(super) fn setup_knowledge_sandbox_and_hooks(
        &mut self,
        stage: &Stage,
        stage_id: &str,
        session_id: &str,
    ) -> Result<bool> {
        let Some(merged_sandbox) = self.write_knowledge_sandbox_settings(stage, stage_id)? else {
            return Ok(false);
        };
        self.install_knowledge_hooks(stage_id, session_id, merged_sandbox.permission_mode)
    }
}
