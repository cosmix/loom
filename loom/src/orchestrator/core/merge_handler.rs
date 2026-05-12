//! Merge session handling and auto-merge logic

use anyhow::{Context, Result};

use crate::git::branch::branch_name_for_stage;
use crate::git::merge::{check_merge_state, MergeState};
use crate::git::merge::{get_conflicting_files_from_status, verify_merge_succeeded};
use crate::models::session::{Session, SessionType};
use crate::models::stage::StageStatus;
use crate::orchestrator::auto_merge::{attempt_auto_merge, is_auto_merge_enabled, AutoMergeResult};
use crate::orchestrator::signals::{
    find_live_merge_session_for_stage, generate_merge_signal, remove_signal,
};
use crate::process::is_process_alive;
use crate::verify::transitions::load_stage;

use super::persistence::Persistence;
use super::{clear_status_line, Orchestrator};

impl Orchestrator {
    pub(super) fn handle_merge_session_completed(
        &mut self,
        session_id: &str,
        stage_id: &str,
    ) -> Result<()> {
        clear_status_line();
        eprintln!("Merge session '{session_id}' completed for stage '{stage_id}'");

        // Check if the merge was successful and update stage accordingly
        let mut stage = self.load_stage(stage_id)?;

        // Determine the merge point to check against
        let merge_point = crate::git::branch::resolve_target_branch(
            &self.config.base_branch,
            &self.config.repo_root,
        );

        // If stage is already marked as merged, do NOT trust the flag blindly
        // for non-knowledge stages. Derive commit if missing, then verify
        // ancestry; only treat as merged if the verification passes. Trust
        // merged=true only for knowledge stages (no branch by design).
        if stage.merged {
            let actually_merged = if stage.stage_type == crate::models::stage::StageType::Knowledge
            {
                true
            } else {
                let commit = match stage.completed_commit.clone() {
                    Some(c) => Some(c),
                    None => crate::git::get_branch_head(
                        &branch_name_for_stage(stage_id),
                        &self.config.repo_root,
                    )
                    .ok(),
                };
                commit
                    .map(|c| {
                        verify_merge_succeeded(&c, &merge_point, &self.config.repo_root)
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
            };

            if actually_merged {
                // Merge resolved - clean up signal and active session
                if let Err(e) = remove_signal(session_id, &self.config.work_dir) {
                    eprintln!("Warning: Failed to remove merge signal: {e}");
                }
                self.active_sessions.remove(stage_id);
                clear_status_line();
                eprintln!("Stage '{stage_id}' merge completed successfully");
                return Ok(());
            }

            tracing::error!(
                stage_id = %stage_id,
                "Merge session ended with merged=true but ancestry verification failed; \
                 falling through to verify_and_finalize_merge to revert."
            );
            stage.merged = false;
            if let Err(e) = self.save_stage(&stage) {
                tracing::warn!(
                    stage_id = %stage_id,
                    error = %e,
                    "Failed to revert merged=true after failed verification"
                );
            }
            // Fall through to the verify_and_finalize logic below.
        }

        // Check if the merge was actually successful by examining git state.
        // check_merge_state uses completed_commit + git ancestry (primary) and
        // falls back to metadata flags. With the reordered check, git ancestry
        // takes priority over the merge_conflict flag.
        let merge_state = check_merge_state(&stage, &merge_point, &self.config.repo_root);

        // If check_merge_state couldn't determine success (Conflict/Unknown — e.g.,
        // completed_commit was never set), fall back to checking the branch HEAD directly.
        let merge_state = match merge_state {
            Ok(MergeState::Conflict) | Ok(MergeState::Unknown) => {
                let branch_name = branch_name_for_stage(stage_id);
                match crate::git::get_branch_head(&branch_name, &self.config.repo_root) {
                    Ok(head) => {
                        match crate::git::branch::is_ancestor_of(
                            &head,
                            &merge_point,
                            &self.config.repo_root,
                        ) {
                            Ok(true) => Ok(MergeState::Merged),
                            _ => merge_state,
                        }
                    }
                    Err(_) => {
                        // Branch doesn't exist — may have been cleaned up after merge
                        if !crate::git::branch_exists(&branch_name, &self.config.repo_root)
                            .unwrap_or(true)
                        {
                            Ok(MergeState::BranchMissing)
                        } else {
                            merge_state
                        }
                    }
                }
            }
            other => other,
        };

        match merge_state {
            Ok(MergeState::Merged) => {
                self.finalize_merge_resolution(
                    &mut stage,
                    session_id,
                    stage_id,
                    "merge verified and marked as complete",
                );
            }
            Ok(MergeState::BranchMissing) => {
                self.finalize_merge_resolution(
                    &mut stage,
                    session_id,
                    stage_id,
                    "branch cleaned up, marking as merged",
                );
            }
            Ok(MergeState::Pending) | Ok(MergeState::Conflict) | Ok(MergeState::Unknown) => {
                // PID dead but merge not resolved - remove active session but KEEP signal
                // file as guard against respawning every poll cycle
                self.active_sessions.remove(stage_id);

                // Merge not complete - log next steps for the user
                eprintln!("Merge may not be complete. To finish:");
                eprintln!("  1. Verify the merge was successful: git status");
                eprintln!("  2. If merge is complete, run: loom worktree remove {stage_id}");
                eprintln!("  3. If issues remain, run: loom stage merge {stage_id}");
            }
            Err(e) => {
                // PID dead but merge state unknown - remove active session but KEEP signal
                // file as guard against respawning every poll cycle
                self.active_sessions.remove(stage_id);

                eprintln!("Warning: Failed to verify merge state: {e}");
                eprintln!("To complete:");
                eprintln!("  1. Verify the merge was successful: git status");
                eprintln!("  2. If merge is complete, run: loom worktree remove {stage_id}");
                eprintln!("  3. If issues remain, run: loom stage merge {stage_id}");
            }
        }

        Ok(())
    }

    /// Common logic for resolving a merge session (Merged or BranchMissing outcomes).
    ///
    /// Transitions the stage to Completed, updates the graph, and cleans up
    /// the signal file and active session tracking.
    fn finalize_merge_resolution(
        &mut self,
        stage: &mut crate::models::stage::Stage,
        session_id: &str,
        stage_id: &str,
        log_message: &str,
    ) {
        stage.merged = true;
        stage.merge_conflict = false;

        if stage.status == StageStatus::MergeConflict || stage.status == StageStatus::MergeBlocked {
            if let Err(e) = stage.status.try_transition(StageStatus::Completed) {
                eprintln!("Warning: Failed to transition stage to Completed: {e}");
            } else {
                stage.status = StageStatus::Completed;
            }
        }

        if let Err(e) = self.save_stage(stage) {
            eprintln!("Warning: Failed to save stage after merge resolution: {e}");
        }

        self.graph.set_node_merged(stage_id, true);
        if let Err(e) = self.graph.mark_completed(stage_id) {
            eprintln!("Warning: Failed to mark stage as completed in graph: {e}");
        }

        if let Err(e) = remove_signal(session_id, &self.config.work_dir) {
            eprintln!("Warning: Failed to remove merge signal: {e}");
        }
        self.active_sessions.remove(stage_id);

        clear_status_line();
        eprintln!("Stage '{stage_id}' {log_message}");
    }

    /// Verify merge succeeded and update stage state accordingly.
    ///
    /// This helper encapsulates the common pattern of verifying a merge via git ancestry
    /// check and updating stage/graph state based on the result.
    ///
    /// Behavior depends on the stage's current status:
    /// - Non-Completed stage with failed verification: transitions to `MergeBlocked`.
    /// - Completed stage with failed verification: leaves the stage at
    ///   `Completed + !merged` without writing `merged=true`. This breaks the original
    ///   respawn loop (see `spawn_merge_resolution_sessions` which only acts on
    ///   `MergeConflict | MergeBlocked`) without lying about merge status.
    ///
    /// Returns `true` if the merge was verified successful via git ancestry.
    /// Returns `false` otherwise. Caller must NOT assume `merged=true` on false.
    fn verify_and_finalize_merge(
        &mut self,
        stage: &mut crate::models::stage::Stage,
        stage_id: &str,
        target_branch: &str,
    ) -> bool {
        // If stage has no completed_commit, try to derive it from the branch HEAD.
        // If we can derive one, save the stage and fall through to the ancestry check.
        // If we can't, leave the stage as Completed + !merged (phantom-merge prevention).
        if stage.completed_commit.is_none() {
            let branch_name = branch_name_for_stage(stage_id);
            match crate::git::get_branch_head(&branch_name, &self.config.repo_root) {
                Ok(head) => {
                    stage.completed_commit = Some(head);
                    if let Err(e) = self.save_stage(stage) {
                        tracing::warn!(
                            stage_id = %stage_id,
                            error = %e,
                            "Failed to save derived completed_commit"
                        );
                    }
                }
                Err(_) => {
                    // Branch is missing and no commit recorded - we cannot verify the
                    // merge. Do NOT write merged=true. Leave the stage as Completed +
                    // !merged; spawn_merge_resolution_sessions will not pick it up
                    // (it acts only on MergeConflict | MergeBlocked), so no respawn.
                    clear_status_line();
                    tracing::error!(
                        stage_id = %stage_id,
                        branch = %branch_name,
                        "Cannot verify merge: stage has no completed_commit and branch is missing; \
                         leaving stage as Completed + !merged"
                    );
                    return false;
                }
            }
        }

        // Safe to unwrap: just ensured Some above.
        let completed_commit = stage.completed_commit.clone().unwrap();
        match verify_merge_succeeded(&completed_commit, target_branch, &self.config.repo_root) {
            Ok(true) => {
                // Verification passed - mark as merged
                stage.merged = true;
                if let Err(e) = self.save_stage(stage) {
                    tracing::warn!(
                        stage_id = %stage_id,
                        error = %e,
                        "Failed to save stage after merge"
                    );
                }
                true
            }
            Ok(false) => {
                // If stage is already Completed (terminal state), we cannot safely
                // transition to MergeBlocked. The old code force-wrote merged=true
                // here, which is exactly the phantom-merge bug. Leave as
                // Completed + !merged instead.
                if stage.status == StageStatus::Completed {
                    clear_status_line();
                    tracing::error!(
                        stage_id = %stage_id,
                        commit = %completed_commit,
                        target = %target_branch,
                        "Merge verification failed for completed stage: commit not in target branch. \
                         Leaving stage as Completed + !merged (will NOT auto-respawn); \
                         run `loom stage merge {}` manually.",
                        stage_id
                    );
                    return false;
                }
                // Non-Completed path: transition to MergeBlocked as before.
                clear_status_line();
                tracing::error!(
                    stage_id = %stage_id,
                    "merge verification failed: commit not in target branch"
                );
                if let Err(e) = stage.try_mark_merge_blocked() {
                    tracing::warn!(
                        stage_id = %stage_id,
                        error = %e,
                        "Failed to transition to MergeBlocked"
                    );
                    stage.status = StageStatus::MergeBlocked;
                }
                if let Err(e) = self.save_stage(stage) {
                    tracing::warn!(
                        stage_id = %stage_id,
                        error = %e,
                        "Failed to save stage"
                    );
                }
                if let Err(e) = self.graph.mark_status(stage_id, StageStatus::MergeBlocked) {
                    tracing::warn!(
                        stage_id = %stage_id,
                        error = %e,
                        "Failed to mark stage as merge blocked in graph"
                    );
                }
                false
            }
            Err(e) => {
                // Same logic as Ok(false): if Completed, leave alone; otherwise
                // transition to MergeBlocked.
                if stage.status == StageStatus::Completed {
                    clear_status_line();
                    tracing::error!(
                        stage_id = %stage_id,
                        error = %e,
                        "Merge verification error for completed stage. \
                         Leaving stage as Completed + !merged (will NOT auto-respawn); \
                         run `loom stage merge {}` manually.",
                        stage_id
                    );
                    return false;
                }
                clear_status_line();
                tracing::error!(
                    stage_id = %stage_id,
                    error = %e,
                    "merge verification error"
                );
                if let Err(e) = stage.try_mark_merge_blocked() {
                    tracing::warn!(
                        stage_id = %stage_id,
                        error = %e,
                        "Failed to transition to MergeBlocked"
                    );
                    stage.status = StageStatus::MergeBlocked;
                }
                if let Err(e) = self.save_stage(stage) {
                    tracing::warn!(
                        stage_id = %stage_id,
                        error = %e,
                        "Failed to save stage"
                    );
                }
                if let Err(e) = self.graph.mark_status(stage_id, StageStatus::MergeBlocked) {
                    tracing::warn!(
                        stage_id = %stage_id,
                        error = %e,
                        "Failed to mark stage as merge blocked in graph"
                    );
                }
                false
            }
        }
    }

    /// Attempt auto-merge for a completed stage.
    ///
    /// Returns `true` if the merge succeeded or was not needed (stage can be marked Completed).
    /// Returns `false` if the merge failed with conflicts (stage should be marked MergeConflict).
    pub(super) fn try_auto_merge(&mut self, stage_id: &str) -> bool {
        // Load the stage to check auto_merge setting
        let mut stage = match load_stage(stage_id, &self.config.work_dir) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: Failed to load stage for auto-merge check: {e}");
                // If we can't load the stage, allow completion to proceed
                return true;
            }
        };

        // If stage is already merged (e.g., by `loom stage complete`), skip auto-merge.
        // Without this guard, the daemon would redundantly attempt to merge an already-merged
        // stage. If cleanup partially removed the branch but not the worktree directory,
        // the redundant merge would fail and force-overwrite the Completed status to
        // MergeConflict/MergeBlocked — even though Completed is a terminal state.
        // This would spawn a spurious resolver session while the dependent stage was
        // already started by sync_graph_with_stage_files.
        if stage.merged {
            return true;
        }

        // Load plan-level auto_merge setting from config
        let plan_auto_merge = (|| -> Option<bool> {
            let config = crate::fs::load_config(&self.config.work_dir).ok()??;
            let source_path = config.source_path()?;
            // source_path is relative to project root
            let plan_path = self.config.repo_root.join(&source_path);
            let plan_content = std::fs::read_to_string(&plan_path).ok()?;

            // Extract YAML metadata from plan content
            let yaml_content = crate::plan::parser::extract_yaml_metadata(&plan_content).ok()?;
            let metadata = crate::plan::parser::parse_and_validate(&yaml_content).ok()?;

            metadata.loom.auto_merge
        })();

        if !is_auto_merge_enabled(&stage, self.config.auto_merge, plan_auto_merge) {
            // Auto-merge disabled - skip the merge attempt and leave the stage as
            // Completed + !merged. The user will run `loom stage merge <id>`
            // manually when ready. DO NOT write merged=true here — that would
            // silently satisfy downstream dependency checks without the work
            // actually being merged.
            tracing::info!(
                stage_id = %stage_id,
                "auto-merge disabled; leaving stage as Completed + !merged \
                 (run `loom stage merge {}` to merge manually)",
                stage_id
            );
            return true;
        }

        // Get target branch (from config or default branch of the repo)
        let target_branch = crate::git::branch::resolve_target_branch(
            &self.config.base_branch,
            &self.config.repo_root,
        );

        // Phantom-merge guard: refuse to auto-merge a stage whose branch
        // EXISTS but has zero commits beyond the merge target. Without this
        // check, an empty branch (HEAD == target HEAD) silently "merges" as a
        // no-op: `completed_commit` is filled from branch HEAD (which equals
        // target HEAD), `attempt_auto_merge` returns AlreadyUpToDate /
        // FastForward, and `is_ancestor_of(target_HEAD, target)` trivially
        // passes — resulting in `merged: true` for work that was never
        // committed. Leave the stage at Completed + !merged so dependents do
        // not unblock. Skip the guard when the branch is missing — that path
        // already lands in the `NoBranch` / `Err` arms below with their own
        // recovery handling.
        let stage_branch = branch_name_for_stage(stage_id);
        let stage_branch_exists =
            crate::git::branch::branch_exists(&stage_branch, &self.config.repo_root)
                .unwrap_or(false);
        if stage_branch_exists {
            match crate::git::branch::commits_ahead_of(
                &stage_branch,
                &target_branch,
                &self.config.repo_root,
            ) {
                Ok(0) => {
                    tracing::error!(
                        stage_id = %stage_id,
                        branch = %stage_branch,
                        target = %target_branch,
                        "Stage branch has zero commits beyond target; refusing auto-merge \
                         to prevent phantom merge. Leaving stage as Completed + !merged. \
                         The agent never committed work for this stage — re-queue or \
                         redo the stage manually."
                    );
                    return false;
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(
                        stage_id = %stage_id,
                        branch = %stage_branch,
                        error = %e,
                        "commits_ahead_of probe failed; proceeding with merge attempt"
                    );
                }
            }
        }

        // Capture completed_commit before merge attempt so the orchestrator can
        // later verify merge resolution via git ancestry even if conflicts occur.
        if stage.completed_commit.is_none() {
            let branch_name = branch_name_for_stage(stage_id);
            if let Ok(head) = crate::git::get_branch_head(&branch_name, &self.config.repo_root) {
                stage.completed_commit = Some(head);
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save completed_commit: {e}");
                }
            }
        }

        clear_status_line();
        eprintln!("Auto-merging stage '{stage_id}'...");

        match attempt_auto_merge(
            &stage,
            &self.config.repo_root,
            &self.config.work_dir,
            &target_branch,
            &self.dispatcher,
            self.config.backend_type,
        ) {
            Ok(AutoMergeResult::Success {
                files_changed,
                insertions,
                deletions,
                ..
            }) => {
                let success = self.verify_and_finalize_merge(&mut stage, stage_id, &target_branch);
                if success {
                    clear_status_line();
                    eprintln!(
                        "Stage '{stage_id}' merged: {files_changed} files, +{insertions} -{deletions}"
                    );
                }
                success
            }
            Ok(AutoMergeResult::FastForward { .. }) => {
                let success = self.verify_and_finalize_merge(&mut stage, stage_id, &target_branch);
                if success {
                    clear_status_line();
                    eprintln!("Stage '{stage_id}' merged (fast-forward)");
                }
                success
            }
            Ok(AutoMergeResult::AlreadyUpToDate { .. }) => {
                let success = self.verify_and_finalize_merge(&mut stage, stage_id, &target_branch);
                if success {
                    clear_status_line();
                    eprintln!("Stage '{stage_id}' already up to date");
                }
                success
            }
            Ok(AutoMergeResult::ConflictResolutionSpawned {
                session,
                conflicting_files,
            }) => {
                // CRITICAL: Transition stage to MergeConflict status to prevent dependent stages
                // from starting before conflicts are resolved
                stage.merge_conflict = true;
                if let Err(e) = stage.try_mark_merge_conflict() {
                    eprintln!("Warning: Failed to transition stage to MergeConflict status: {e}");
                    // Fallback: force the status (this should not fail based on transitions.rs)
                    stage.status = StageStatus::MergeConflict;
                }
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage merge conflict status: {e}");
                }

                // Also update the graph to reflect MergeConflict status
                if let Err(e) = self.graph.mark_status(stage_id, StageStatus::MergeConflict) {
                    eprintln!("Warning: Failed to mark stage as merge conflict in graph: {e}");
                }

                // Track the merge session so the monitor can detect its lifecycle
                let session = *session;
                let session_id = session.id.clone();
                self.active_sessions
                    .insert(stage_id.to_string(), session.clone());
                if let Err(e) = self.save_session(&session) {
                    eprintln!("Warning: Failed to save merge session: {e}");
                    // Remove from active_sessions to avoid tracking a session
                    // that the monitor can't reload from disk after restart
                    self.active_sessions.remove(stage_id);
                }

                clear_status_line();
                eprintln!(
                    "Stage '{stage_id}' has {} conflict(s). Spawned resolution session: {session_id}",
                    conflicting_files.len()
                );

                // Return false to indicate merge did not succeed - stage should NOT be marked Completed
                false
            }
            Ok(AutoMergeResult::NoWorktree) => {
                // Nothing to merge - stage may have been created without worktree
                self.verify_and_finalize_merge(&mut stage, stage_id, &target_branch)
            }
            Err(e) => {
                clear_status_line();
                tracing::error!(
                    stage_id = %stage_id,
                    error = %e,
                    "Auto-merge failed"
                );
                // If stage is already Completed (terminal state), leave as
                // Completed + !merged. The old code force-wrote merged=true here,
                // which is the phantom-merge bug. The respawn loop is already
                // broken structurally (spawn_merge_resolution_sessions only
                // acts on MergeConflict | MergeBlocked), so lying about merge
                // status is unnecessary and dangerous.
                if stage.status == StageStatus::Completed {
                    tracing::error!(
                        stage_id = %stage_id,
                        "Stage is already Completed; leaving as Completed + !merged \
                         despite auto-merge error. Run `loom stage merge {}` manually.",
                        stage_id
                    );
                    return false;
                }
                // On error, transition to MergeBlocked status
                if let Err(transition_err) = stage.try_mark_merge_blocked() {
                    tracing::warn!(
                        stage_id = %stage_id,
                        error = %transition_err,
                        "Failed to transition stage to MergeBlocked status"
                    );
                    stage.status = StageStatus::MergeBlocked;
                }
                if let Err(e) = self.save_stage(&stage) {
                    tracing::warn!(
                        stage_id = %stage_id,
                        error = %e,
                        "Failed to save stage after merge error"
                    );
                }
                if let Err(e) = self.graph.mark_status(stage_id, StageStatus::MergeBlocked) {
                    tracing::warn!(
                        stage_id = %stage_id,
                        error = %e,
                        "Failed to mark stage as merge blocked in graph"
                    );
                }
                // Return false - merge failed, stage should not be marked Completed
                false
            }
        }
    }

    /// Spawn merge resolution sessions for stages in MergeConflict or MergeBlocked status.
    ///
    /// Called during the main loop to detect stages that need merge resolution
    /// and spawn Claude Code sessions to resolve them.
    pub fn spawn_merge_resolution_sessions(&mut self) -> Result<usize> {
        let stages_dir = self.config.work_dir.join("stages");
        if !stages_dir.exists() {
            return Ok(0);
        }

        let mut spawned = 0;

        // Read all stage files
        for entry in std::fs::read_dir(&stages_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            // Extract stage ID from filename using the canonical parser
            let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let stage_id = match crate::fs::stage_files::extract_stage_id(filename) {
                Some(id) => id,
                None => continue,
            };

            // Load stage and check status
            let stage = match self.load_stage(&stage_id) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Only handle MergeConflict and MergeBlocked statuses
            if !matches!(
                stage.status,
                StageStatus::MergeConflict | StageStatus::MergeBlocked
            ) {
                continue;
            }

            // Skip if there's already an active merge session for this stage.
            //
            // When `loom stage complete` detects a merge conflict, the stage transitions
            // to MergeConflict and `spawn_merge_resolver()` returns DaemonManaged (no
            // session spawned). However, the original execution session (SessionType::Stage)
            // may still be alive in active_sessions — it hasn't exited yet. That stage
            // session will never resolve the merge conflict, so we must not let it block
            // merge resolver spawning.
            //
            // Logic:
            // - If the active session is a Stage session (not Merge), it's stale in the
            //   context of merge resolution. Remove it and its signal, then fall through.
            // - If it IS a Merge session, only skip if the process is still alive.
            if self.active_sessions.contains_key(&stage_id) {
                let session = self.active_sessions.get(&stage_id).unwrap();
                if session.session_type != SessionType::Stage {
                    // It's a merge (or base-conflict) session — only skip if still alive
                    let is_alive = session.pid.map(is_process_alive).unwrap_or(false);
                    if is_alive {
                        continue;
                    }
                }
                // Either a stale Stage session or a dead Merge session — clean up
                let stale_session = self.active_sessions.remove(&stage_id).unwrap();
                let stale_session_id = stale_session.id.clone();
                // Kill the original session to prevent zombie processes.
                // When loom stage complete detected a merge conflict, the Stage session
                // may still be running -- actively terminate it.
                let kill_result = self
                    .dispatcher
                    .for_session(&stale_session)
                    .kill_session(&stale_session);
                if let Err(e) = &kill_result {
                    tracing::debug!(
                        session_id = %stale_session_id,
                        error = %e,
                        "Failed to kill stale session (may already be dead)"
                    );
                }
                if kill_result.is_ok()
                    && stale_session.backend
                        == crate::plan::schema::execution::BackendType::Container
                {
                    let mut updated_session = stale_session.clone();
                    updated_session.clear_container_identity();
                    if let Err(e) = self.save_session(&updated_session) {
                        eprintln!(
                            "Warning: failed to clear container identity for stale session '{stale_session_id}': {e}"
                        );
                    }
                }
                // Remove the old signal file so it doesn't block respawning
                if let Err(e) = remove_signal(&stale_session_id, &self.config.work_dir) {
                    eprintln!("Warning: Failed to remove stale signal for session '{stale_session_id}': {e}");
                }
            }

            // Use the shared helper that checks signal + PID liveness and
            // cleans up stale signals atomically.
            match find_live_merge_session_for_stage(&stage_id, &self.config.work_dir) {
                Ok(Some(_)) => continue, // Live resolver already running — skip
                Ok(None) => { /* fall through to spawn */ }
                Err(e) => {
                    tracing::warn!(
                        stage_id = %stage_id,
                        error = %e,
                        "Failed to check for existing merge signal; falling through to spawn"
                    );
                }
            }

            // Spawn a merge resolution session
            if let Err(e) = self.spawn_merge_resolution_session(&stage) {
                clear_status_line();
                eprintln!(
                    "Warning: Failed to spawn merge resolution session for '{stage_id}': {e}"
                );
            } else {
                spawned += 1;
            }
        }

        Ok(spawned)
    }

    /// Spawn a merge resolution session for a stage with merge issues.
    fn spawn_merge_resolution_session(
        &mut self,
        stage: &crate::models::stage::Stage,
    ) -> Result<()> {
        let source_branch = branch_name_for_stage(&stage.id);

        // Get target branch
        let target_branch = crate::git::branch::resolve_target_branch(
            &self.config.base_branch,
            &self.config.repo_root,
        );

        // Get conflicting files. If MERGE_HEAD is already set,
        // get_conflicting_files_from_status refuses (helper-level guard) — in
        // that case we fall back to reading the active merge's unmerged paths
        // directly. Only run the probe merge when there is NO active merge.
        let conflicting_files =
            if crate::git::merge::merge_head_exists(&self.config.repo_root).unwrap_or(false) {
                Vec::new()
            } else {
                get_conflicting_files_from_status(
                    &source_branch,
                    &target_branch,
                    &self.config.repo_root,
                    &self.config.work_dir,
                )
                .unwrap_or_default()
            };

        // Create a merge session. Merge sessions always run on the
        // project-default backend (we don't currently support per-stage
        // merge backends).
        let mut session = Session::new_merge(source_branch.clone(), target_branch.clone());
        session.set_backend(self.config.backend_type);

        // Detect any active merge in the main repo so the signal can branch
        // between "start a fresh merge" and "continue the existing one".
        // If MERGE_HEAD is set, get_conflicting_files_from_status will refuse
        // (helper-level guard); fall back to reading the active merge's
        // unmerged paths directly.
        let in_progress = crate::git::merge::detect_in_progress_merge_at(&self.config.repo_root)
            .ok()
            .flatten();
        let conflicting_files = if conflicting_files.is_empty() {
            match in_progress.as_ref().map(|m| &m.state) {
                Some(crate::git::merge::ActiveMergeState::HasUnmergedPaths(paths)) => paths.clone(),
                _ => conflicting_files,
            }
        } else {
            conflicting_files
        };

        // Generate merge signal
        let signal_path = generate_merge_signal(
            &session,
            stage,
            &source_branch,
            &target_branch,
            &conflicting_files,
            in_progress.as_ref(),
            &self.config.work_dir,
        )
        .context("Failed to generate merge signal")?;

        // Spawn the merge resolution session via the project's backend.
        let spawned_session = self
            .dispatcher
            .for_stage(self.config.backend_type)
            .spawn_merge_session(stage, session, &signal_path, &self.config.repo_root)
            .context("Failed to spawn merge resolution session")?;

        clear_status_line();
        eprintln!(
            "Spawned merge resolution session for stage '{}': {}",
            stage.id, spawned_session.id
        );

        if !conflicting_files.is_empty() {
            eprintln!("  Conflicting files:");
            for file in &conflicting_files {
                eprintln!("    - {file}");
            }
        }

        // Track the session
        self.active_sessions
            .insert(stage.id.clone(), spawned_session.clone());

        // Save the session file
        self.save_session(&spawned_session)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_merge_blocked_to_completed_transition_is_valid() {
        use crate::models::stage::StageStatus;

        let status = StageStatus::MergeBlocked;
        assert!(
            status.can_transition_to(&StageStatus::Completed),
            "MergeBlocked -> Completed should be a valid transition"
        );

        let result = status.try_transition(StageStatus::Completed);
        assert!(
            result.is_ok(),
            "MergeBlocked -> Completed transition should succeed"
        );
    }

    #[test]
    fn test_merge_conflict_to_completed_transition_is_valid() {
        use crate::models::stage::StageStatus;

        let status = StageStatus::MergeConflict;
        assert!(
            status.can_transition_to(&StageStatus::Completed),
            "MergeConflict -> Completed should be a valid transition"
        );
    }

    #[test]
    fn test_plan_auto_merge_extraction_true() {
        let plan_content = r#"# Test Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  auto_merge: true
  stages:
    - id: test-stage
      name: "Test"
      stage_type: knowledge
      working_dir: "."
      dependencies: []
      acceptance: []
```

<!-- END loom METADATA -->
"#;
        let yaml_content = crate::plan::parser::extract_yaml_metadata(plan_content).unwrap();
        let metadata = crate::plan::parser::parse_and_validate(&yaml_content).unwrap();
        assert_eq!(metadata.loom.auto_merge, Some(true));
    }

    #[test]
    fn test_plan_auto_merge_extraction_false() {
        let plan_content = r#"# Test Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  auto_merge: false
  stages:
    - id: test-stage
      name: "Test"
      stage_type: knowledge
      working_dir: "."
      dependencies: []
      acceptance: []
```

<!-- END loom METADATA -->
"#;
        let yaml_content = crate::plan::parser::extract_yaml_metadata(plan_content).unwrap();
        let metadata = crate::plan::parser::parse_and_validate(&yaml_content).unwrap();
        assert_eq!(metadata.loom.auto_merge, Some(false));
    }

    #[test]
    fn test_plan_auto_merge_default_none() {
        let plan_content = r#"# Test Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: test-stage
      name: "Test"
      stage_type: knowledge
      working_dir: "."
      dependencies: []
      acceptance: []
```

<!-- END loom METADATA -->
"#;
        let yaml_content = crate::plan::parser::extract_yaml_metadata(plan_content).unwrap();
        let metadata = crate::plan::parser::parse_and_validate(&yaml_content).unwrap();
        assert_eq!(metadata.loom.auto_merge, None);
    }
}
