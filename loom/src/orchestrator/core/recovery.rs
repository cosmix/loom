//! Error recovery and state synchronization

use anyhow::Result;
use std::io::{self, Write};

use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::retry::{calculate_backoff, is_backoff_elapsed, should_auto_retry};
use crate::parser::frontmatter::parse_from_markdown;

use super::clear_status_line;
use super::persistence::Persistence;
use super::Orchestrator;

/// Trait for recovery operations
pub(super) trait Recovery: Persistence {
    /// Reconcile any active main-repo merge with stage state on disk and
    /// update the in-memory graph if a stage was mutated.
    ///
    /// MUST run BEFORE `sync_graph_with_stage_files` AND BEFORE
    /// `recover_orphaned_sessions`:
    /// - Recovery deletes orphaned merge sessions; attribution depends on
    ///   their metadata.
    /// - Sync reads stage files into the graph; if reconcile flips
    ///   `Completed + merged=true` -> `MergeConflict + merged=false` AFTER
    ///   sync, the graph still has the stale view and would queue dependents
    ///   based on a phantom merge.
    fn reconcile_and_update_graph(&mut self) -> Result<()>;

    /// Sync the execution graph with existing stage file statuses.
    /// This syncs FROM files TO graph.
    fn sync_graph_with_stage_files(&mut self) -> Result<()>;

    /// Sync queued status from graph back to stage files.
    /// This ensures files reflect when dependencies are satisfied.
    /// This syncs FROM graph TO files.
    fn sync_queued_status_to_files(&mut self) -> Result<()>;

    /// Recover orphaned sessions (process died but session/stage files exist).
    fn recover_orphaned_sessions(&mut self) -> Result<usize>;

    /// Check if all stages are in a terminal state (for watch mode exit)
    fn all_stages_terminal(&self) -> bool;

    /// Print a status update showing current stage counts
    fn print_status_update(&self);
}

/// Check if a blocked stage is eligible for automatic retry.
///
/// A stage is eligible for retry if:
/// - It has failure_info with a retryable failure_type (SessionCrash or Timeout)
/// - retry_count < max_retries (default 3)
/// - Sufficient time has elapsed since the last failure (exponential backoff)
///
/// # Arguments
/// * `stage` - The stage to check
///
/// # Returns
/// `true` if the stage should be automatically retried, `false` otherwise
fn check_retry_eligibility(stage: &Stage) -> bool {
    let Some(ref info) = stage.failure_info else {
        return false;
    };

    let max = stage.max_retries.unwrap_or(3);
    if !should_auto_retry(&info.failure_type, stage.retry_count, max) {
        return false;
    }

    // Calculate backoff: base 30s, max 300s (5 minutes)
    let backoff = calculate_backoff(stage.retry_count, 30, 300);
    is_backoff_elapsed(stage.last_failure_at, backoff)
}

impl Orchestrator {
    /// Re-verify a `Completed + merged=true` non-knowledge stage at sync time.
    ///
    /// Derives `completed_commit` from `loom/<id>` HEAD when missing, then
    /// checks ancestry against the target branch. If the ancestry check fails
    /// OR the branch is also missing, reverts `merged=false` so dependents
    /// don't treat the stage as satisfied.
    pub(super) fn verify_merged_true_or_revert(&mut self, stage: &mut Stage) {
        // Derive completed_commit when missing.
        if stage.completed_commit.is_none() {
            let branch_name = crate::git::branch::branch_name_for_stage(&stage.id);
            match crate::git::get_branch_head(&branch_name, &self.config.repo_root) {
                Ok(head) => {
                    stage.completed_commit = Some(head);
                    if let Err(e) = self.save_stage(stage) {
                        tracing::warn!(
                            stage_id = %stage.id,
                            error = %e,
                            "Failed to persist derived completed_commit during merged=true verify"
                        );
                    }
                }
                Err(_) => {
                    // Branch missing AND no commit recorded — unverifiable
                    // phantom-merge candidate. Revert merged=false.
                    tracing::error!(
                        stage_id = %stage.id,
                        branch = %branch_name,
                        "Phantom-merge candidate at sync: merged=true with no commit \
                         and no branch. Reverting to merged=false."
                    );
                    stage.merged = false;
                    if let Err(e) = self.save_stage(stage) {
                        tracing::warn!(
                            stage_id = %stage.id,
                            error = %e,
                            "Failed to save merged=false revert"
                        );
                    }
                    return; // Nothing more to verify.
                }
            }
        }

        if let Some(commit) = stage.completed_commit.clone() {
            let target = crate::git::branch::resolve_target_branch(
                &self.config.base_branch,
                &self.config.repo_root,
            );
            if !crate::git::merge::verify_merge_succeeded(&commit, &target, &self.config.repo_root)
                .unwrap_or(false)
            {
                tracing::error!(
                    stage_id = %stage.id,
                    commit = %commit,
                    target = %target,
                    "Phantom merge detected at sync: merged=true but commit not in target. \
                     Reverting to merged=false; stage will need re-merge."
                );
                stage.merged = false;
                if let Err(e) = self.save_stage(stage) {
                    tracing::warn!(
                        stage_id = %stage.id,
                        error = %e,
                        "Failed to save merged=false revert"
                    );
                }
            }
        }
    }
}

impl Recovery for Orchestrator {
    fn reconcile_and_update_graph(&mut self) -> Result<()> {
        use crate::orchestrator::merge_attribution::{
            reconcile_main_repo_active_merge, ReconciliationOutcome,
        };
        match reconcile_main_repo_active_merge(&self.config.repo_root, &self.config.work_dir)? {
            ReconciliationOutcome::NoActiveMerge
            | ReconciliationOutcome::UnattributedLogged
            | ReconciliationOutcome::AttributedNoOp { .. } => {}
            ReconciliationOutcome::StageMutated { stage_id, .. } => {
                // Disk was corrected; update the graph immediately so any
                // caller in this iteration sees the corrected state. The
                // next sync_graph_with_stage_files call will pick this up
                // again, which is harmless (idempotent).
                self.graph.set_node_merged(&stage_id, false);
                if let Err(e) = self
                    .graph
                    .mark_status(&stage_id, StageStatus::MergeConflict)
                {
                    tracing::warn!(
                        stage_id = %stage_id,
                        error = %e,
                        "Failed to update graph after phantom-merge revert; \
                         next sync will reconcile."
                    );
                }
            }
        }
        Ok(())
    }

    fn sync_graph_with_stage_files(&mut self) -> Result<()> {
        let stages_dir = self.config.work_dir.join("stages");
        if !stages_dir.exists() {
            return Ok(());
        }

        // Collect stage IDs that may need a one-shot auto-merge retry (Fix 11).
        // We iterate twice: first to sync state, then to retry stuck stages.
        // The two-phase approach avoids borrow-checker issues with calling
        // `self.try_auto_merge` while holding a loaded stage.
        let mut stuck_completed_stage_ids: Vec<String> = Vec::new();

        // Read all stage files and sync their status to the graph
        for entry in std::fs::read_dir(&stages_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            // Extract stage ID from filename (handles prefixed format like 01-stage-id.md)
            let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let stage_id = if let Some(rest) = filename.strip_prefix(|c: char| c.is_ascii_digit()) {
                // Remove leading digits and dash
                rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '-')
            } else {
                filename
            };

            if stage_id.is_empty() {
                continue;
            }

            // Load the stage and sync status
            // NOTE: We use stage.id (from YAML frontmatter) for graph operations,
            // not stage_id (from filename), because the graph is built using frontmatter IDs.
            if let Ok(mut stage) = self.load_stage(stage_id) {
                tracing::debug!(
                    stage_id = %stage.id,
                    status = ?stage.status,
                    merged = stage.merged,
                    "[sync_graph_with_stage_files] Loaded stage"
                );
                // Always sync outputs to the graph so they're available for dependent stages
                if !stage.outputs.is_empty() {
                    self.graph
                        .set_node_outputs(&stage.id, stage.outputs.clone());
                }

                match stage.status {
                    StageStatus::Completed => {
                        // Verify merged=true non-knowledge stages: derive
                        // commit from branch HEAD if missing, then check
                        // ancestry. Old force-unsafe routes set merged=true
                        // without ever populating completed_commit, so a
                        // sync guard that only runs "when a commit exists"
                        // misses exactly that bug class.
                        if stage.merged
                            && stage.stage_type != crate::models::stage::StageType::Knowledge
                        {
                            self.verify_merged_true_or_revert(&mut stage);
                        }

                        // If stage is Completed but not merged, try to verify the
                        // merge via git ancestry. NEVER assume merged without proof —
                        // doing so produces phantom merges and lost work (see
                        // doc/plans/PLAN-fix-phantom-merge.md).
                        if !stage.merged {
                            // If completed_commit is missing, try to derive it from
                            // the stage's branch head before attempting verification.
                            if stage.completed_commit.is_none() {
                                let branch_name =
                                    crate::git::branch::branch_name_for_stage(&stage.id);
                                match crate::git::get_branch_head(
                                    &branch_name,
                                    &self.config.repo_root,
                                ) {
                                    Ok(head) => {
                                        tracing::info!(
                                            stage_id = %stage.id,
                                            commit = %head,
                                            "Derived completed_commit from branch head for recovery"
                                        );
                                        stage.completed_commit = Some(head);
                                        if let Err(e) = self.save_stage(&stage) {
                                            tracing::warn!(
                                                stage_id = %stage.id,
                                                error = %e,
                                                "Failed to save derived completed_commit"
                                            );
                                        }
                                    }
                                    Err(_) => {
                                        // Branch is missing; cannot verify. Leave as
                                        // Completed + !merged. Do NOT save. This stage
                                        // is a candidate for the one-shot retry below
                                        // (Fix 11), in case the user ran `loom stage
                                        // complete --no-verify` before restart.
                                        tracing::error!(
                                            stage_id = %stage.id,
                                            branch = %branch_name,
                                            "Completed stage has no completed_commit and branch \
                                             is missing; cannot verify merge. Leaving as \
                                             Completed + !merged."
                                        );
                                        stuck_completed_stage_ids.push(stage.id.clone());
                                    }
                                }
                            }

                            // If we have a completed_commit (either pre-existing or
                            // just derived), run the ancestry check.
                            if let Some(ref completed_commit) = stage.completed_commit {
                                let target_branch = crate::git::branch::resolve_target_branch(
                                    &self.config.base_branch,
                                    &self.config.repo_root,
                                );
                                match crate::git::merge::verify_merge_succeeded(
                                    completed_commit,
                                    &target_branch,
                                    &self.config.repo_root,
                                ) {
                                    Ok(true) => {
                                        tracing::info!(
                                            stage_id = %stage.id,
                                            "Auto-verified merge for completed stage, \
                                             marking as merged"
                                        );
                                        stage.merged = true;
                                        if let Err(e) = self.save_stage(&stage) {
                                            tracing::warn!(
                                                error = %e,
                                                "Failed to save auto-verified merge state"
                                            );
                                        }
                                    }
                                    Ok(false) => {
                                        // Commit is not in target branch. Do NOT
                                        // write merged=true. Mark as a retry candidate
                                        // so the daemon makes a one-shot attempt.
                                        tracing::error!(
                                            stage_id = %stage.id,
                                            commit = %completed_commit,
                                            target = %target_branch,
                                            "Completed stage commit is not an ancestor of target \
                                             branch; leaving as Completed + !merged. \
                                             Run `loom stage merge {}` to retry.",
                                            stage.id
                                        );
                                        stuck_completed_stage_ids.push(stage.id.clone());
                                    }
                                    Err(e) => {
                                        // Verification failed (e.g., transient git
                                        // error). Do NOT write merged=true. Also a
                                        // retry candidate.
                                        tracing::error!(
                                            stage_id = %stage.id,
                                            error = %e,
                                            "Merge verification errored for completed stage; \
                                             leaving as Completed + !merged"
                                        );
                                        stuck_completed_stage_ids.push(stage.id.clone());
                                    }
                                }
                            }
                        }

                        // IMPORTANT: Set merged status FIRST, before mark_completed().
                        // mark_completed() triggers update_ready_status() which needs the
                        // correct merged value to determine if dependent stages are ready.
                        tracing::debug!(
                            stage_id = %stage.id,
                            merged = stage.merged,
                            "[sync_graph_with_stage_files] Completed stage"
                        );
                        self.graph.set_node_merged(&stage.id, stage.merged);
                        // Now mark as completed - this triggers update_ready_status() which
                        // will see the correct merged value set above
                        if let Err(e) = self.graph.mark_completed(&stage.id) {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::Queued => {
                        // Sync Ready status from stage files to graph
                        // This handles stages marked Ready by `loom verify` -> trigger_dependents()
                        if let Err(e) = self.graph.mark_queued(&stage.id) {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::Executing => {
                        // Mark as executing in graph to track active sessions
                        if let Err(e) = self.graph.mark_executing(&stage.id) {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::Blocked => {
                        // Check if the blocked stage is eligible for automatic retry
                        if check_retry_eligibility(&stage) {
                            // Re-queue the stage for retry
                            if stage.try_mark_queued().is_ok() {
                                clear_status_line();
                                tracing::warn!(
                                    stage_id = %stage.id,
                                    attempt = stage.retry_count + 1,
                                    "Auto-retrying stage"
                                );

                                // ATOMIC UPDATE PATTERN:
                                // 1. Save original graph state for potential rollback
                                // 2. Update graph first (tentatively)
                                // 3. Save file
                                // 4. If save fails, rollback graph to original state
                                let original_graph_status =
                                    self.graph.get_node(&stage.id).map(|n| n.status.clone());

                                // Update graph first
                                if let Err(e) = self.graph.mark_queued(&stage.id) {
                                    tracing::warn!(
                                        "Failed to sync graph status for stage {}: {}",
                                        stage.id,
                                        e
                                    );
                                }

                                // Now save the file
                                if let Err(e) = self.save_stage(&stage) {
                                    tracing::warn!(error = %e, "Failed to save stage during retry");
                                    // Rollback graph to original state
                                    if let Some(StageStatus::Blocked) = original_graph_status {
                                        let _ =
                                            self.graph.mark_status(&stage.id, StageStatus::Blocked);
                                    }
                                }
                            }
                        } else {
                            // Not eligible for retry, just mark as blocked in graph
                            if let Err(e) = self.graph.mark_status(&stage.id, StageStatus::Blocked)
                            {
                                tracing::warn!(
                                    "Failed to sync graph status for stage {}: {}",
                                    stage.id,
                                    e
                                );
                            }
                        }
                    }
                    StageStatus::WaitingForInput => {
                        if let Err(e) = self
                            .graph
                            .mark_status(&stage.id, StageStatus::WaitingForInput)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::NeedsHandoff => {
                        if let Err(e) = self.graph.mark_status(&stage.id, StageStatus::NeedsHandoff)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::MergeConflict => {
                        if let Err(e) = self
                            .graph
                            .mark_status(&stage.id, StageStatus::MergeConflict)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::CompletedWithFailures => {
                        if let Err(e) = self
                            .graph
                            .mark_status(&stage.id, StageStatus::CompletedWithFailures)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::MergeBlocked => {
                        if let Err(e) = self.graph.mark_status(&stage.id, StageStatus::MergeBlocked)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::Skipped => {
                        if let Err(e) = self.graph.mark_status(&stage.id, StageStatus::Skipped) {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::WaitingForDeps => {
                        if let Err(e) = self
                            .graph
                            .mark_status(&stage.id, StageStatus::WaitingForDeps)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::NeedsHumanReview => {
                        if let Err(e) = self
                            .graph
                            .mark_status(&stage.id, StageStatus::NeedsHumanReview)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                }
            }
        }

        // Fix 11: one-shot auto-merge retry for stuck Completed + !merged stages.
        //
        // The `loom stage complete --no-verify` flow legitimately produces a
        // Completed + !merged + !completed_commit state. Normally the
        // StageCompleted event triggers `try_auto_merge`, but after a daemon
        // restart no event fires for already-Completed stages — leaving them
        // permanently stuck. Retry once per daemon session to unstick them.
        //
        // This also retries the case where a commit was derived from the branch
        // HEAD but ancestry reports Ok(false): the commit is on the stage branch
        // but not yet in the target branch. That scenario is exactly what
        // `try_auto_merge` is designed to resolve — it runs the merge command.
        //
        // `merge_retry_attempted` is in-memory only. If the retry fails, the
        // entry stays in the set so we don't re-attempt every 5-second poll.
        // User-driven `loom stage merge` is independent of this set.
        for stuck_id in stuck_completed_stage_ids {
            if self.merge_retry_attempted.contains(&stuck_id) {
                continue;
            }
            self.merge_retry_attempted.insert(stuck_id.clone());
            tracing::info!(
                stage_id = %stuck_id,
                "one-shot merge retry for stuck Completed + !merged stage"
            );
            // Ignore return value — even if the retry fails, we've logged it
            // and won't retry again this session.
            let _ = self.try_auto_merge(&stuck_id);
        }

        // After syncing all stage statuses, refresh ready status to ensure
        // dependent stages get marked as Queued when their dependencies complete.
        // This handles cases where stages are processed out of topological order.
        self.graph.refresh_ready_status();

        Ok(())
    }

    fn sync_queued_status_to_files(&mut self) -> Result<()> {
        // Get all nodes that are Queued in the graph
        let queued_stage_ids: Vec<String> = self
            .graph
            .all_nodes()
            .iter()
            .filter(|node| node.status == StageStatus::Queued)
            .map(|node| node.id.clone())
            .collect();

        // For each queued stage, update the file if it's still WaitingForDeps
        for stage_id in queued_stage_ids {
            if let Ok(mut stage) = self.load_stage(&stage_id) {
                // Only update if the file says WaitingForDeps but graph says Queued
                if stage.status == StageStatus::WaitingForDeps {
                    // Use validated transition
                    if stage.try_mark_queued().is_ok() {
                        self.save_stage(&stage)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn recover_orphaned_sessions(&mut self) -> Result<usize> {
        let sessions_dir = self.config.work_dir.join("sessions");
        if !sessions_dir.exists() {
            return Ok(0);
        }

        let mut recovered = 0;

        for entry in std::fs::read_dir(&sessions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            // Load session from file
            let content = std::fs::read_to_string(&path)?;
            let session: Session = match parse_from_markdown(&content, "Session") {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Check if session is still running
            let is_running = self.backend.is_session_alive(&session).unwrap_or(false);

            if !is_running {
                // Orphaned session - get stage ID and reset it
                if let Some(stage_id) = &session.stage_id {
                    // Load the stage
                    if let Ok(mut stage) = self.load_stage(stage_id) {
                        // Recover if stage was Executing, NeedsHandoff, or Blocked due to crash
                        if matches!(
                            stage.status,
                            StageStatus::Executing
                                | StageStatus::NeedsHandoff
                                | StageStatus::Blocked
                        ) {
                            clear_status_line();
                            tracing::warn!(
                                stage_id = %stage_id,
                                status = ?stage.status,
                                "Recovering orphaned stage"
                            );

                            // Reset stage to Ready using validated transition
                            // NeedsHandoff -> Queued and Blocked -> Queued are valid transitions
                            // Executing -> Queued is not valid, so we go through Blocked first
                            if stage.status == StageStatus::Executing {
                                // Executing -> Blocked (intermediate step for recovery)
                                if let Err(e) = stage.try_mark_blocked() {
                                    tracing::warn!(error = %e, "Failed to transition Executing -> Blocked during recovery");
                                }
                            }
                            // Now Blocked/NeedsHandoff -> Queued is valid
                            if let Err(e) = stage.try_mark_queued() {
                                // Log a warning that validation was bypassed for recovery
                                tracing::warn!(
                                    error = %e,
                                    status = ?stage.status,
                                    "State transition validation failed during orphaned session recovery, bypassing"
                                );
                                stage.status = StageStatus::Queued;
                            }
                            stage.session = None;
                            stage.close_reason = Some("Session crashed/orphaned".to_string());
                            stage.updated_at = chrono::Utc::now();

                            // ATOMIC UPDATE PATTERN:
                            // 1. Save original graph state for potential rollback
                            // 2. Update graph first (tentatively)
                            // 3. Save file
                            // 4. If save fails, rollback graph to original state
                            let original_graph_status =
                                self.graph.get_node(stage_id).map(|n| n.status.clone());

                            // Update graph first - only if not in terminal state
                            let graph_updated = if let Some(node) = self.graph.get_node(stage_id) {
                                if node.status != StageStatus::Completed {
                                    if let Err(e) = self.graph.mark_queued(stage_id) {
                                        tracing::warn!(
                                            "Failed to sync graph status for stage {}: {}",
                                            stage_id,
                                            e
                                        );
                                    }
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            };

                            // Now save the file
                            if let Err(e) = self.save_stage(&stage) {
                                // Rollback graph to original state if we updated it
                                if graph_updated {
                                    if let Some(original_status) = original_graph_status {
                                        let _ = self.graph.mark_status(stage_id, original_status);
                                    }
                                }
                                return Err(e);
                            }

                            recovered += 1;
                        } else if matches!(
                            stage.status,
                            StageStatus::MergeConflict | StageStatus::MergeBlocked
                        ) {
                            // Merge session died without resolving - clean up session reference
                            // but keep the stage in its current status so spawn_merge_resolution_sessions()
                            // can detect it and spawn a fresh merge session
                            clear_status_line();
                            tracing::warn!(
                                stage_id = %stage_id,
                                status = ?stage.status,
                                "Recovering orphaned merge session"
                            );

                            stage.session = None;
                            stage.close_reason = Some("Merge session crashed/orphaned".to_string());
                            stage.updated_at = chrono::Utc::now();

                            // Save the updated stage - no graph status change needed since we keep the status
                            if let Err(e) = self.save_stage(&stage) {
                                tracing::warn!(error = %e, "Failed to save stage during merge session recovery");
                            }

                            recovered += 1;
                        }
                    }
                }

                // Remove the orphaned session file
                let _ = std::fs::remove_file(&path);

                // Remove the orphaned signal file
                let signal_path = self
                    .config
                    .work_dir
                    .join("signals")
                    .join(format!("{}.md", session.id));
                let _ = std::fs::remove_file(&signal_path);
            }
        }

        Ok(recovered)
    }

    fn all_stages_terminal(&self) -> bool {
        // Don't exit while merge resolution sessions are running — the daemon
        // needs to stay alive to monitor them and handle their completion.
        if !self.active_sessions.is_empty() {
            return false;
        }

        let stages_dir = self.config.work_dir.join("stages");
        if !stages_dir.exists() {
            return true;
        }

        for node in self.graph.all_nodes() {
            // Check graph status first
            match node.status {
                StageStatus::Completed => continue,
                StageStatus::Blocked => continue,
                StageStatus::Skipped => continue,
                StageStatus::MergeConflict => continue, // Terminal until resolved
                StageStatus::CompletedWithFailures => continue, // Terminal until retried
                StageStatus::MergeBlocked => continue,  // Terminal until fixed
                StageStatus::NeedsHumanReview => continue, // Waiting for human review
                StageStatus::WaitingForDeps
                | StageStatus::Queued
                | StageStatus::Executing
                | StageStatus::WaitingForInput
                | StageStatus::NeedsHandoff => {
                    // Need to check the actual stage file for held status
                    if let Ok(stage) = self.load_stage(&node.id) {
                        match stage.status {
                            StageStatus::Blocked | StageStatus::Completed => {
                                continue;
                            }
                            StageStatus::Queued | StageStatus::WaitingForDeps if stage.held => {
                                continue
                            }
                            _ => return false,
                        }
                    } else {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn print_status_update(&self) {
        let nodes = self.graph.all_nodes();
        let mut running = 0;
        let mut pending = 0;
        let mut completed = 0;
        let mut blocked = 0;

        for node in nodes {
            match node.status {
                StageStatus::Executing => running += 1,
                StageStatus::WaitingForDeps | StageStatus::Queued => pending += 1,
                StageStatus::Completed => completed += 1,
                StageStatus::Blocked => blocked += 1,
                StageStatus::Skipped => completed += 1, // Count skipped as completed for status display
                StageStatus::WaitingForInput => running += 1, // Paused but still active
                StageStatus::NeedsHandoff => running += 1, // Needs continuation but still in progress
                StageStatus::MergeConflict => blocked += 1, // Blocked on conflict resolution
                StageStatus::CompletedWithFailures => blocked += 1, // Failed acceptance, needs retry
                StageStatus::MergeBlocked => blocked += 1,          // Blocked on merge error
                StageStatus::NeedsHumanReview => blocked += 1,      // Waiting for human review
            }
        }

        let mut status_parts = vec![
            format!("{running} running"),
            format!("{pending} pending"),
            format!("{completed} completed"),
        ];

        if blocked > 0 {
            status_parts.push(format!("{blocked} blocked"));
        }

        print!(
            "\r[Polling... {}] (Ctrl+C to detach, 'loom status' for details)    ",
            status_parts.join(", ")
        );
        // Flush stdout to ensure the status line appears immediately
        let _ = io::stdout().flush();
    }
}

#[cfg(test)]
mod tests {
    //! Focused unit tests for the building blocks of the recovery-path
    //! phantom-merge fix (PLAN-fix-phantom-merge.md Fix 1).
    //!
    //! Full integration coverage of `sync_graph_with_stage_files` requires a
    //! live `Orchestrator` with a real `Backend` and `Monitor`, which is too
    //! heavy for a unit test. Instead, we exercise the exact helper calls the
    //! recovery path makes in sequence:
    //!
    //! 1. `get_branch_head(&loom/<id>, repo_root)` to derive HEAD when
    //!    `completed_commit` is missing.
    //! 2. `is_ancestor_of(commit, target, repo_root)` to verify the derived
    //!    commit actually landed in the target branch.
    //!
    //! End-to-end recovery behavior (including the one-shot retry and stuck
    //! stage handling) is exercised by `loom/tests/phantom_merge.rs`.
    use std::process::Command;
    use tempfile::TempDir;

    use crate::git::branch::{branch_name_for_stage, get_branch_head, is_ancestor_of};

    fn init_repo() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(root)
            .output()
            .unwrap();
        std::fs::write(root.join("seed.txt"), "seed").unwrap();
        Command::new("git")
            .args(["add", "seed.txt"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "seed"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(root)
            .output()
            .unwrap();

        tmp
    }

    /// Fix 1: when `completed_commit` is missing but the loom branch exists,
    /// recovery derives HEAD and checks ancestry. If the commit is NOT in the
    /// target branch, `merged` must stay false.
    ///
    /// This test stands in for the recovery-path decision: it verifies the
    /// helpers produce the exact (branch_head, is_ancestor=false) pair that
    /// the production code relies on to REFUSE to set merged=true.
    #[test]
    fn derive_head_and_ancestry_reports_not_ancestor_when_unmerged() {
        let repo = init_repo();
        let root = repo.path();

        // Create loom/oauth-hardening branch with a commit that stays off main.
        Command::new("git")
            .args(["checkout", "-b", "loom/oauth-hardening"])
            .current_dir(root)
            .output()
            .unwrap();
        std::fs::write(root.join("oauth.rs"), "hardened").unwrap();
        Command::new("git")
            .args(["add", "oauth.rs"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "hardening"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(root)
            .output()
            .unwrap();

        let branch = branch_name_for_stage("oauth-hardening");
        let head = get_branch_head(&branch, root).expect("HEAD derivable");
        assert!(
            !head.is_empty(),
            "branch HEAD should be derivable when branch exists"
        );

        let is_anc = is_ancestor_of(&head, "main", root).expect("ancestry check");
        assert!(
            !is_anc,
            "derived HEAD must NOT be an ancestor of main when branch is unmerged — \
             this is the exact signal that recovery uses to refuse merged=true (Fix 1)"
        );
    }

    /// Fix 1: when `completed_commit` is missing AND the loom branch is
    /// missing, recovery has no way to derive HEAD. The helper must surface
    /// the failure so the caller leaves the stage at Completed + !merged.
    #[test]
    fn derive_head_fails_when_branch_missing() {
        let repo = init_repo();
        let root = repo.path();

        let branch = branch_name_for_stage("nonexistent-stage");
        let result = get_branch_head(&branch, root);
        assert!(
            result.is_err(),
            "branch HEAD derivation must fail when branch does not exist — \
             recovery relies on this to log an error and leave stage as Completed + !merged"
        );
    }

    /// Fix 1 happy path: if the loom branch exists AND has been merged into
    /// main, the ancestry check returns true. The recovery path is allowed
    /// to set `merged = true` only in this case.
    #[test]
    fn ancestry_true_after_branch_merged_into_main() {
        let repo = init_repo();
        let root = repo.path();

        // Create a branch with a commit.
        Command::new("git")
            .args(["checkout", "-b", "loom/landed-stage"])
            .current_dir(root)
            .output()
            .unwrap();
        std::fs::write(root.join("landed.rs"), "done").unwrap();
        Command::new("git")
            .args(["add", "landed.rs"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "landed"])
            .current_dir(root)
            .output()
            .unwrap();

        // Merge it into main with --no-ff.
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "merge",
                "--no-ff",
                "-m",
                "merge landed",
                "loom/landed-stage",
            ])
            .current_dir(root)
            .output()
            .unwrap();

        let branch = branch_name_for_stage("landed-stage");
        let head = get_branch_head(&branch, root).expect("HEAD derivable");
        let is_anc = is_ancestor_of(&head, "main", root).expect("ancestry check");
        assert!(
            is_anc,
            "after branch is merged into main, ancestry must be true — \
             recovery is then allowed to set merged=true"
        );
    }
}
