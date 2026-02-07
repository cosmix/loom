//! Merge session handling and auto-merge logic

use anyhow::{Context, Result};

use crate::git::branch::{branch_name_for_stage, default_branch};
use crate::git::merge::{check_merge_state, MergeState};
use crate::git::merge::{get_conflicting_files_from_status, verify_merge_succeeded};
use crate::models::session::Session;
use crate::models::stage::StageStatus;
use crate::orchestrator::auto_merge::{attempt_auto_merge, is_auto_merge_enabled, AutoMergeResult};
use crate::orchestrator::signals::{generate_merge_signal, read_merge_signal, remove_signal};
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

        // If stage is already marked as merged (e.g., agent ran `loom worktree remove`), we're done
        if stage.merged {
            // Merge resolved - clean up signal and active session
            if let Err(e) = remove_signal(session_id, &self.config.work_dir) {
                eprintln!("Warning: Failed to remove merge signal: {e}");
            }
            self.active_sessions.remove(stage_id);
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
                eprintln!("  3. If issues remain, run: loom merge {stage_id}");
            }
            Err(e) => {
                // PID dead but merge state unknown - remove active session but KEEP signal
                // file as guard against respawning every poll cycle
                self.active_sessions.remove(stage_id);

                eprintln!("Warning: Failed to verify merge state: {e}");
                eprintln!("To complete:");
                eprintln!("  1. Verify the merge was successful: git status");
                eprintln!("  2. If merge is complete, run: loom worktree remove {stage_id}");
                eprintln!("  3. If issues remain, run: loom merge {stage_id}");
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

        if stage.status == StageStatus::MergeConflict {
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
    /// Returns `true` if the merge was verified successful (or no verification needed for legacy stages).
    /// Returns `false` if verification failed (stage marked as MergeBlocked).
    fn verify_and_finalize_merge(
        &mut self,
        stage: &mut crate::models::stage::Stage,
        stage_id: &str,
        target_branch: &str,
    ) -> bool {
        // If stage has a completed_commit, verify it's in the target branch
        if let Some(ref completed_commit) = stage.completed_commit {
            match verify_merge_succeeded(completed_commit, target_branch, &self.config.repo_root) {
                Ok(true) => {
                    // Verification passed - mark as merged
                    stage.merged = true;
                    if let Err(e) = self.save_stage(stage) {
                        eprintln!("Warning: Failed to save stage after merge: {e}");
                    }
                    true
                }
                Ok(false) => {
                    // Merge reported success but commit not in target - phantom merge!
                    clear_status_line();
                    eprintln!(
                        "Stage '{stage_id}' merge verification failed: commit not in target branch"
                    );
                    if let Err(e) = stage.try_mark_merge_blocked() {
                        eprintln!("Warning: Failed to transition to MergeBlocked: {e}");
                        stage.status = StageStatus::MergeBlocked;
                    }
                    if let Err(e) = self.save_stage(stage) {
                        eprintln!("Warning: Failed to save stage: {e}");
                    }
                    if let Err(e) = self.graph.mark_status(stage_id, StageStatus::MergeBlocked) {
                        eprintln!("Warning: Failed to mark stage as merge blocked in graph: {e}");
                    }
                    false
                }
                Err(e) => {
                    // Verification failed - do NOT mark as merged to prevent phantom merges
                    clear_status_line();
                    eprintln!("Stage '{stage_id}' merge verification error: {e}");
                    if let Err(e) = stage.try_mark_merge_blocked() {
                        eprintln!("Warning: Failed to transition to MergeBlocked: {e}");
                        stage.status = StageStatus::MergeBlocked;
                    }
                    if let Err(e) = self.save_stage(stage) {
                        eprintln!("Warning: Failed to save stage: {e}");
                    }
                    if let Err(e) = self.graph.mark_status(stage_id, StageStatus::MergeBlocked) {
                        eprintln!("Warning: Failed to mark stage as merge blocked in graph: {e}");
                    }
                    false
                }
            }
        } else {
            // No completed_commit - legacy stage, mark as merged without verification
            stage.merged = true;
            if let Err(e) = self.save_stage(stage) {
                eprintln!("Warning: Failed to save stage: {e}");
            }
            true
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
            // Auto-merge disabled - mark as merged without attempting merge
            stage.merged = true;
            if let Err(e) = self.save_stage(&stage) {
                eprintln!("Warning: Failed to save stage after skipping auto-merge: {e}");
            }
            return true;
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
                eprintln!("Auto-merge failed for '{stage_id}': {e}");
                // On error, transition to MergeBlocked status
                if let Err(transition_err) = stage.try_mark_merge_blocked() {
                    eprintln!("Warning: Failed to transition stage to MergeBlocked status: {transition_err}");
                    stage.status = StageStatus::MergeBlocked;
                }
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage after merge error: {e}");
                }
                if let Err(e) = self.graph.mark_status(stage_id, StageStatus::MergeBlocked) {
                    eprintln!("Warning: Failed to mark stage as merge blocked in graph: {e}");
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

            // Skip if there's already an active session for this stage
            if self.active_sessions.contains_key(&stage_id) {
                continue;
            }

            // Skip if there's already a merge signal for this stage
            // (indicates a merge session was previously spawned)
            if self.has_merge_signal_for_stage(&stage_id) {
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
    ///
    /// Uses the structured `read_merge_signal` parser rather than raw string
    /// matching, so this stays correct if the signal file format changes.
    fn has_merge_signal_for_stage(&self, stage_id: &str) -> bool {
        let signal_ids = match crate::orchestrator::signals::list_signals(&self.config.work_dir) {
            Ok(ids) => ids,
            Err(e) => {
                eprintln!("Warning: Failed to list signals while checking for merge signal: {e}");
                return false;
            }
        };

        for signal_id in &signal_ids {
            if let Ok(Some(merge_signal)) = read_merge_signal(signal_id, &self.config.work_dir) {
                if merge_signal.stage_id == stage_id {
                    return true;
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
        let source_branch = branch_name_for_stage(&stage.id);

        // Get target branch
        let target_branch = self.config.base_branch.clone().unwrap_or_else(|| {
            default_branch(&self.config.repo_root).unwrap_or_else(|_| "main".to_string())
        });

        // Get conflicting files (test merge to see what conflicts)
        let conflicting_files = get_conflicting_files_from_status(
            &source_branch,
            &target_branch,
            &self.config.repo_root,
            &self.config.work_dir,
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

#[cfg(test)]
mod tests {
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
