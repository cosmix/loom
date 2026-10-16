//! Worktree spawn preflight and hook gates.
use super::persistence::Persistence;
use super::stage_executor::install_required_hooks;
use super::Orchestrator;
use crate::git;
use crate::hooks::find_hooks_dir;
use crate::models::failure::FailureType;
use crate::models::session::Session;
use crate::models::stage::Stage;
use anyhow::Result;
/// Helpers shared by the worktree spawn path. Write-ahead session handling,
/// live-session adoption, and Blocked-transition cleanup live in
/// `session_lifecycle.rs`, and the spawn tail that calls these in
/// `stage_spawn.rs`; this impl keeps what is specific to the spawn sequence.
impl Orchestrator {
    /// Merge, validate and expand this stage's sandbox config at spawn time.
    /// Mirrors `spawn_setup.rs::validate_knowledge_sandbox`. Writes nothing:
    /// the capsule (`native/session_settings.rs`) carries the resolved
    /// settings into the session, not `T/.claude/settings.local.json`.
    ///
    /// Returns `false` if the stage was blocked instead (invalid config).
    pub(super) fn validate_stage_sandbox(
        &mut self,
        stage: &Stage,
        stage_id: &str,
        session_id: &str,
    ) -> bool {
        let mut merged_sandbox = crate::sandbox::merge_config(
            &self.config.sandbox_config,
            &stage.sandbox,
            stage.stage_type,
            &stage.implementers,
        );
        // Defense-in-depth: re-validate at spawn time in case the on-disk
        // config became invalid after `loom init` last accepted it.
        if let Err(e) = crate::sandbox::validate_config(&merged_sandbox) {
            self.block_and_undo_session(
                stage_id,
                session_id,
                FailureType::SandboxSetupFailure,
                format!("invalid sandbox config at spawn: {e:#}"),
            );
            return false;
        }
        crate::sandbox::expand_paths(&mut merged_sandbox);
        crate::sandbox::warn_missing_grants(&merged_sandbox, stage_id);
        true
    }

    /// Require the Claude Code hooks directory to exist for a stage spawn.
    /// Mirrors `spawn_setup.rs::require_knowledge_hooks`. Writes nothing: the
    /// capsule already embeds the hooks configuration itself.
    ///
    /// Returns `false` if the stage was blocked instead (hook install failure).
    pub(super) fn require_stage_hooks(&mut self, stage_id: &str, session_id: &str) -> bool {
        if let Err(e) = install_required_hooks(find_hooks_dir(), stage_id) {
            self.block_and_undo_session(
                stage_id,
                session_id,
                FailureType::SandboxSetupFailure,
                format!("{e:#}"),
            );
            return false;
        }
        true
    }

    /// Write ahead the record of a v2 stage's `Contract` session, mirroring
    /// `write_ahead_session` for the `Stage` session (see its invariant doc).
    ///
    /// Returns `None` if the write-ahead failed; the stage has already been
    /// marked Blocked and the caller should return without spawning.
    pub(super) fn write_ahead_contract_session(&mut self, stage_id: &str) -> Option<Session> {
        let mut session = Session::new_contract(stage_id);
        session.backend = self.backend.resolve_lane();
        if let Err(e) = self.save_session(&session) {
            let err_msg = format!(
                "Failed to write contract session record ahead of spawn for {stage_id}: {e:#}"
            );
            let _ = self.persist_blocked_stage(
                stage_id,
                FailureType::InfrastructureError,
                vec![err_msg],
            );
            return None;
        }
        Some(session)
    }

    /// Run the stage's `before_stage` pre-condition gate before spawning.
    ///
    /// The gate is a delta-proof: it asserts the feature does NOT exist yet, so
    /// it only holds on the stage's first attempt. Every later spawn — orphan
    /// recovery, `loom stage retry`, crash retry — reuses the same worktree and
    /// branch, where the previous attempt's work is still sitting. Re-running
    /// the gate there fails on that work and marks the stage `Blocked` *before*
    /// a session is spawned, so nothing can finish the work and the next retry
    /// repeats the failure forever. Skip the gate once the workspace holds work.
    ///
    /// # Returns
    /// `Ok(true)` if the spawn may proceed, `Ok(false)` if the stage was marked
    /// `Blocked` because a pre-condition did not hold.
    pub(super) fn before_stage_gate_passed(
        &mut self,
        stage: &Stage,
        worktree_path: &std::path::Path,
        base_branch: &str,
    ) -> Result<bool> {
        if stage.before_stage.is_empty() {
            return Ok(true);
        }

        if self.has_prior_stage_work(stage, worktree_path, base_branch) {
            return Ok(true);
        }
        let stage_id = stage.id.clone();

        let check_dir = match &stage.working_dir {
            Some(wd) if wd != "." && !wd.is_empty() => worktree_path.join(wd),
            _ => worktree_path.to_path_buf(),
        };

        println!("  Running before-stage checks for '{stage_id}'...");
        match crate::verify::before_after::run_before_stage_checks(&stage.before_stage, &check_dir)
        {
            Ok(gaps) if !gaps.is_empty() => {
                for gap in &gaps {
                    eprintln!("  ✗ Before-stage: {}", gap.description);
                    eprintln!("    → {}", gap.suggestion);
                }
                eprintln!(
                    "Before-stage verification failed for '{stage_id}' - pre-conditions not met"
                );

                let _ = self.persist_blocked_stage(
                    &stage_id,
                    FailureType::TestFailure,
                    gaps.iter().map(|gap| gap.description.clone()).collect(),
                );
                Ok(false)
            }
            Ok(_) => {
                println!("  ✓ Before-stage checks passed for '{stage_id}'");
                Ok(true)
            }
            Err(e) => {
                eprintln!("Warning: Before-stage checks errored for '{stage_id}': {e}");
                // Continue anyway - before-stage is advisory, don't block on errors
                Ok(true)
            }
        }
    }
    fn has_prior_stage_work(
        &self,
        stage: &Stage,
        worktree_path: &std::path::Path,
        base_branch: &str,
    ) -> bool {
        let stage_id = stage.id.clone();
        let stage_branch = git::branch_name_for_stage(&stage_id);
        if let Some(evidence) = crate::verify::before_after::find_prior_stage_work(
            &stage_branch,
            base_branch,
            &self.config.repo_root,
            worktree_path,
        ) {
            println!("  Skipping before-stage checks for '{stage_id}': {evidence}");
            tracing::info!(
                stage_id = %stage_id,
                evidence = %evidence,
                "Skipping before-stage pre-conditions: workspace already holds work from a previous attempt"
            );
            return true;
        }

        false
    }
}
