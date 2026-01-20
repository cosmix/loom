//! Merge session handling and auto-merge logic

use anyhow::{Context, Result};

use crate::commands::status::merge_status::{check_merge_state, MergeState};
use crate::git::branch::default_branch;
use crate::git::merge::get_conflicting_files_from_status;
use crate::models::session::Session;
use crate::models::stage::StageStatus;
use crate::orchestrator::auto_merge::{attempt_auto_merge, is_auto_merge_enabled, AutoMergeResult};
use crate::orchestrator::signals::{generate_merge_signal, remove_signal};
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

        // Remove the merge signal file
        remove_signal(session_id, &self.config.work_dir)?;

        // Clean up the active session
        self.active_sessions.remove(stage_id);

        // Check if the merge was successful and update stage accordingly
        let mut stage = self.load_stage(stage_id)?;

        // If stage is already marked as merged (e.g., agent ran `loom worktree remove`), we're done
        if stage.merged {
            clear_status_line();
            eprintln!("Stage '{stage_id}' merge completed successfully");
            return Ok(());
        }

        // Determine the merge point to check against
        let merge_point = self.config.base_branch.clone().unwrap_or_else(|| {
            default_branch(&self.config.repo_root).unwrap_or_else(|_| "main".to_string())
        });

        // Check if the merge was actually successful by examining git state
        match check_merge_state(&stage, &merge_point, &self.config.repo_root) {
            Ok(MergeState::Merged) => {
                // Merge succeeded - update stage
                stage.merged = true;
                stage.merge_conflict = false;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!(
                        "Warning: Failed to save stage after detecting successful merge: {e}"
                    );
                }
                clear_status_line();
                eprintln!("Stage '{stage_id}' merge verified and marked as complete");
            }
            Ok(MergeState::BranchMissing) => {
                // Branch was deleted (likely by `loom worktree remove`) - assume merge succeeded
                stage.merged = true;
                stage.merge_conflict = false;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage after branch cleanup: {e}");
                }
                clear_status_line();
                eprintln!("Stage '{stage_id}' branch cleaned up, marking as merged");
            }
            Ok(MergeState::Pending) | Ok(MergeState::Conflict) | Ok(MergeState::Unknown) => {
                // Merge not complete - log next steps for the user
                eprintln!("Merge may not be complete. To finish:");
                eprintln!("  1. Verify the merge was successful: git status");
                eprintln!("  2. If merge is complete, run: loom worktree remove {stage_id}");
                eprintln!("  3. If issues remain, run: loom merge {stage_id}");
            }
            Err(e) => {
                eprintln!("Warning: Failed to verify merge state: {e}");
                eprintln!("To complete:");
                eprintln!("  1. Verify the merge was successful: git status");
                eprintln!("  2. If merge is complete, run: loom worktree remove {stage_id}");
                eprintln!("  3. If issues remain, run: loom merge {stage_id}");
            }
        }

        Ok(())
    }

    pub(super) fn try_auto_merge(&self, stage_id: &str) {
        // Load the stage to check auto_merge setting
        let mut stage = match load_stage(stage_id, &self.config.work_dir) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: Failed to load stage for auto-merge check: {e}");
                return;
            }
        };

        // Check if auto-merge is enabled for this stage
        // TODO: In the future, load plan_auto_merge from config file
        let plan_auto_merge = None;

        if !is_auto_merge_enabled(&stage, self.config.auto_merge, plan_auto_merge) {
            return;
        }

        // Get target branch (from config or default branch of the repo)
        let target_branch = self.config.base_branch.clone().unwrap_or_else(|| {
            default_branch(&self.config.repo_root).unwrap_or_else(|_| "main".to_string())
        });

        clear_status_line();
        eprintln!("Auto-merging stage '{stage_id}'...");

        match attempt_auto_merge(
            &stage,
            &self.config.repo_root,
            &self.config.work_dir,
            &target_branch,
            self.backend.as_ref(),
        ) {
            Ok(AutoMergeResult::Success {
                files_changed,
                insertions,
                deletions,
                ..
            }) => {
                // Mark stage as merged and save
                stage.merged = true;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage after merge: {e}");
                }
                clear_status_line();
                eprintln!(
                    "Stage '{stage_id}' merged: {files_changed} files, +{insertions} -{deletions}"
                );
            }
            Ok(AutoMergeResult::FastForward { .. }) => {
                // Mark stage as merged and save
                stage.merged = true;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage after merge: {e}");
                }
                clear_status_line();
                eprintln!("Stage '{stage_id}' merged (fast-forward)");
            }
            Ok(AutoMergeResult::AlreadyUpToDate { .. }) => {
                // Mark stage as merged and save (no changes needed, but branch is up to date)
                stage.merged = true;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage after merge: {e}");
                }
                clear_status_line();
                eprintln!("Stage '{stage_id}' already up to date");
            }
            Ok(AutoMergeResult::ConflictResolutionSpawned {
                session_id,
                conflicting_files,
            }) => {
                // Mark stage as having merge conflicts
                stage.merge_conflict = true;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage merge conflict status: {e}");
                }
                clear_status_line();
                eprintln!(
                    "Stage '{stage_id}' has {} conflict(s). Spawned resolution session: {session_id}",
                    conflicting_files.len()
                );
            }
            Ok(AutoMergeResult::NoWorktree) => {
                // Nothing to merge - stage may have been created without worktree
                // Mark as merged since there's nothing to merge
                stage.merged = true;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage after no-worktree merge: {e}");
                }
            }
            Err(e) => {
                clear_status_line();
                eprintln!("Auto-merge failed for '{stage_id}': {e}");
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

            // Extract stage ID from filename
            let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let stage_id = if let Some(rest) = filename.strip_prefix(|c: char| c.is_ascii_digit()) {
                rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '-')
            } else {
                filename
            };

            if stage_id.is_empty() {
                continue;
            }

            // Load stage and check status
            let stage = match self.load_stage(stage_id) {
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

            // Skip if there's already an active session for this stage
            if self.active_sessions.contains_key(stage_id) {
                continue;
            }

            // Skip if there's already a merge signal for this stage
            // (indicates a merge session was previously spawned)
            let has_existing_signal = self.has_merge_signal_for_stage(stage_id);
            if has_existing_signal {
                continue;
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

    /// Check if there's already a merge signal for a stage.
    fn has_merge_signal_for_stage(&self, stage_id: &str) -> bool {
        let signals_dir = self.config.work_dir.join("signals");
        if !signals_dir.exists() {
            return false;
        }

        // Check all signal files to see if any is a merge signal for this stage
        if let Ok(entries) = std::fs::read_dir(&signals_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("md") {
                    continue;
                }

                if let Ok(content) = std::fs::read_to_string(&path) {
                    // Check if this is a merge signal for our stage
                    if content.contains("# Merge Signal:")
                        && content.contains(&format!("- **Stage**: {stage_id}"))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Spawn a merge resolution session for a stage with merge issues.
    fn spawn_merge_resolution_session(
        &mut self,
        stage: &crate::models::stage::Stage,
    ) -> Result<()> {
        let source_branch = format!("loom/{}", stage.id);

        // Get target branch
        let target_branch = self.config.base_branch.clone().unwrap_or_else(|| {
            default_branch(&self.config.repo_root).unwrap_or_else(|_| "main".to_string())
        });

        // Get conflicting files (test merge to see what conflicts)
        let conflicting_files = get_conflicting_files_from_status(
            &source_branch,
            &target_branch,
            &self.config.repo_root,
        )
        .unwrap_or_default();

        // Create a merge session
        let session = Session::new_merge(source_branch.clone(), target_branch.clone());

        // Generate merge signal
        let signal_path = generate_merge_signal(
            &session,
            stage,
            &source_branch,
            &target_branch,
            &conflicting_files,
            &self.config.work_dir,
        )
        .context("Failed to generate merge signal")?;

        // Spawn the merge resolution session
        let spawned_session = self
            .backend
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
