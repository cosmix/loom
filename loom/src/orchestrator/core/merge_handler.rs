//! Merge session handling and auto-merge logic

use anyhow::{Context, Result};

use crate::git::branch::{branch_name_for_stage, default_branch};
use crate::git::merge::{check_merge_state, MergeState};
use crate::git::merge::{get_conflicting_files_from_status, verify_merge_succeeded};
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
                // Merge succeeded - transition stage to Completed and mark as merged
                stage.merged = true;
                stage.merge_conflict = false;

                // Transition from MergeConflict to Completed (valid per transitions.rs)
                if stage.status == StageStatus::MergeConflict {
                    if let Err(e) = stage.status.try_transition(StageStatus::Completed) {
                        eprintln!("Warning: Failed to transition stage to Completed: {e}");
                    } else {
                        stage.status = StageStatus::Completed;
                    }
                }

                if let Err(e) = self.save_stage(&stage) {
                    eprintln!(
                        "Warning: Failed to save stage after detecting successful merge: {e}"
                    );
                }

                // Update graph to mark as completed and merged
                self.graph.set_node_merged(stage_id, true);
                if let Err(e) = self.graph.mark_completed(stage_id) {
                    eprintln!("Warning: Failed to mark stage as completed in graph: {e}");
                }

                clear_status_line();
                eprintln!("Stage '{stage_id}' merge verified and marked as complete");
            }
            Ok(MergeState::BranchMissing) => {
                // Branch was deleted (likely by `loom worktree remove`) - assume merge succeeded
                stage.merged = true;
                stage.merge_conflict = false;

                // Transition from MergeConflict to Completed (valid per transitions.rs)
                if stage.status == StageStatus::MergeConflict {
                    if let Err(e) = stage.status.try_transition(StageStatus::Completed) {
                        eprintln!("Warning: Failed to transition stage to Completed: {e}");
                    } else {
                        stage.status = StageStatus::Completed;
                    }
                }

                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage after branch cleanup: {e}");
                }

                // Update graph to mark as completed and merged
                self.graph.set_node_merged(stage_id, true);
                if let Err(e) = self.graph.mark_completed(stage_id) {
                    eprintln!("Warning: Failed to mark stage as completed in graph: {e}");
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
                // Verify the merge actually succeeded before marking as merged
                if let Some(ref completed_commit) = stage.completed_commit {
                    match verify_merge_succeeded(
                        completed_commit,
                        &target_branch,
                        &self.config.repo_root,
                    ) {
                        Ok(true) => {
                            // Verification passed - mark as merged
                            stage.merged = true;
                            if let Err(e) = self.save_stage(&stage) {
                                eprintln!("Warning: Failed to save stage after merge: {e}");
                            }
                            clear_status_line();
                            eprintln!(
                                "Stage '{stage_id}' merged: {files_changed} files, +{insertions} -{deletions}"
                            );
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
                            if let Err(e) = self.save_stage(&stage) {
                                eprintln!("Warning: Failed to save stage: {e}");
                            }
                            if let Err(e) =
                                self.graph.mark_status(stage_id, StageStatus::MergeBlocked)
                            {
                                eprintln!(
                                    "Warning: Failed to mark stage as merge blocked in graph: {e}"
                                );
                            }
                            false
                        }
                        Err(e) => {
                            // Verification failed - do NOT mark as merged to prevent phantom merges
                            clear_status_line();
                            eprintln!(
                                "Stage '{stage_id}' merge verification error: {e}"
                            );
                            if let Err(e) = stage.try_mark_merge_blocked() {
                                eprintln!("Warning: Failed to transition to MergeBlocked: {e}");
                                stage.status = StageStatus::MergeBlocked;
                            }
                            if let Err(e) = self.save_stage(&stage) {
                                eprintln!("Warning: Failed to save stage: {e}");
                            }
                            if let Err(e) =
                                self.graph.mark_status(stage_id, StageStatus::MergeBlocked)
                            {
                                eprintln!(
                                    "Warning: Failed to mark stage as merge blocked in graph: {e}"
                                );
                            }
                            false
                        }
                    }
                } else {
                    // No completed_commit - legacy stage, mark as merged without verification
                    stage.merged = true;
                    if let Err(e) = self.save_stage(&stage) {
                        eprintln!("Warning: Failed to save stage after merge: {e}");
                    }
                    clear_status_line();
                    eprintln!(
                        "Stage '{stage_id}' merged: {files_changed} files, +{insertions} -{deletions}"
                    );
                    true
                }
            }
            Ok(AutoMergeResult::FastForward { .. }) => {
                // Verify the merge actually succeeded before marking as merged
                if let Some(ref completed_commit) = stage.completed_commit {
                    match verify_merge_succeeded(
                        completed_commit,
                        &target_branch,
                        &self.config.repo_root,
                    ) {
                        Ok(true) => {
                            stage.merged = true;
                            if let Err(e) = self.save_stage(&stage) {
                                eprintln!("Warning: Failed to save stage after merge: {e}");
                            }
                            clear_status_line();
                            eprintln!("Stage '{stage_id}' merged (fast-forward)");
                            true
                        }
                        Ok(false) => {
                            clear_status_line();
                            eprintln!(
                                "Stage '{stage_id}' merge verification failed: commit not in target branch"
                            );
                            if let Err(e) = stage.try_mark_merge_blocked() {
                                eprintln!("Warning: Failed to transition to MergeBlocked: {e}");
                                stage.status = StageStatus::MergeBlocked;
                            }
                            if let Err(e) = self.save_stage(&stage) {
                                eprintln!("Warning: Failed to save stage: {e}");
                            }
                            if let Err(e) =
                                self.graph.mark_status(stage_id, StageStatus::MergeBlocked)
                            {
                                eprintln!(
                                    "Warning: Failed to mark stage as merge blocked in graph: {e}"
                                );
                            }
                            false
                        }
                        Err(e) => {
                            clear_status_line();
                            eprintln!(
                                "Stage '{stage_id}' fast-forward merge verification error: {e}"
                            );
                            if let Err(e) = stage.try_mark_merge_blocked() {
                                eprintln!("Warning: Failed to transition to MergeBlocked: {e}");
                                stage.status = StageStatus::MergeBlocked;
                            }
                            if let Err(e) = self.save_stage(&stage) {
                                eprintln!("Warning: Failed to save stage: {e}");
                            }
                            if let Err(e) =
                                self.graph.mark_status(stage_id, StageStatus::MergeBlocked)
                            {
                                eprintln!(
                                    "Warning: Failed to mark stage as merge blocked in graph: {e}"
                                );
                            }
                            false
                        }
                    }
                } else {
                    stage.merged = true;
                    if let Err(e) = self.save_stage(&stage) {
                        eprintln!("Warning: Failed to save stage after merge: {e}");
                    }
                    clear_status_line();
                    eprintln!("Stage '{stage_id}' merged (fast-forward)");
                    true
                }
            }
            Ok(AutoMergeResult::AlreadyUpToDate { .. }) => {
                // Verify the stage's commit is actually in the target branch
                if let Some(ref completed_commit) = stage.completed_commit {
                    match verify_merge_succeeded(
                        completed_commit,
                        &target_branch,
                        &self.config.repo_root,
                    ) {
                        Ok(true) => {
                            stage.merged = true;
                            if let Err(e) = self.save_stage(&stage) {
                                eprintln!("Warning: Failed to save stage after merge: {e}");
                            }
                            clear_status_line();
                            eprintln!("Stage '{stage_id}' already up to date");
                            true
                        }
                        Ok(false) => {
                            // Git says up-to-date but commit not in target - suspicious
                            clear_status_line();
                            eprintln!(
                                "Stage '{stage_id}' verification failed: commit not in target branch"
                            );
                            if let Err(e) = stage.try_mark_merge_blocked() {
                                eprintln!("Warning: Failed to transition to MergeBlocked: {e}");
                                stage.status = StageStatus::MergeBlocked;
                            }
                            if let Err(e) = self.save_stage(&stage) {
                                eprintln!("Warning: Failed to save stage: {e}");
                            }
                            if let Err(e) =
                                self.graph.mark_status(stage_id, StageStatus::MergeBlocked)
                            {
                                eprintln!(
                                    "Warning: Failed to mark stage as merge blocked in graph: {e}"
                                );
                            }
                            false
                        }
                        Err(e) => {
                            clear_status_line();
                            eprintln!(
                                "Stage '{stage_id}' up-to-date merge verification error: {e}"
                            );
                            if let Err(e) = stage.try_mark_merge_blocked() {
                                eprintln!("Warning: Failed to transition to MergeBlocked: {e}");
                                stage.status = StageStatus::MergeBlocked;
                            }
                            if let Err(e) = self.save_stage(&stage) {
                                eprintln!("Warning: Failed to save stage: {e}");
                            }
                            if let Err(e) =
                                self.graph.mark_status(stage_id, StageStatus::MergeBlocked)
                            {
                                eprintln!(
                                    "Warning: Failed to mark stage as merge blocked in graph: {e}"
                                );
                            }
                            false
                        }
                    }
                } else {
                    stage.merged = true;
                    if let Err(e) = self.save_stage(&stage) {
                        eprintln!("Warning: Failed to save stage after merge: {e}");
                    }
                    clear_status_line();
                    eprintln!("Stage '{stage_id}' already up to date");
                    true
                }
            }
            Ok(AutoMergeResult::ConflictResolutionSpawned {
                session_id,
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
                // If there's a completed_commit, verify it's in the target branch
                if let Some(ref completed_commit) = stage.completed_commit {
                    match verify_merge_succeeded(
                        completed_commit,
                        &target_branch,
                        &self.config.repo_root,
                    ) {
                        Ok(true) => {
                            stage.merged = true;
                            if let Err(e) = self.save_stage(&stage) {
                                eprintln!("Warning: Failed to save stage: {e}");
                            }
                            true
                        }
                        Ok(false) => {
                            // Commit exists but not in target - should not happen for no-worktree
                            clear_status_line();
                            eprintln!(
                                "Stage '{stage_id}' verification failed: commit not in target branch"
                            );
                            if let Err(e) = stage.try_mark_merge_blocked() {
                                eprintln!("Warning: Failed to transition to MergeBlocked: {e}");
                                stage.status = StageStatus::MergeBlocked;
                            }
                            if let Err(e) = self.save_stage(&stage) {
                                eprintln!("Warning: Failed to save stage: {e}");
                            }
                            if let Err(e) =
                                self.graph.mark_status(stage_id, StageStatus::MergeBlocked)
                            {
                                eprintln!(
                                    "Warning: Failed to mark stage as merge blocked in graph: {e}"
                                );
                            }
                            false
                        }
                        Err(e) => {
                            clear_status_line();
                            eprintln!(
                                "Stage '{stage_id}' no-worktree merge verification error: {e}"
                            );
                            if let Err(e) = stage.try_mark_merge_blocked() {
                                eprintln!("Warning: Failed to transition to MergeBlocked: {e}");
                                stage.status = StageStatus::MergeBlocked;
                            }
                            if let Err(e) = self.save_stage(&stage) {
                                eprintln!("Warning: Failed to save stage: {e}");
                            }
                            if let Err(e) =
                                self.graph.mark_status(stage_id, StageStatus::MergeBlocked)
                            {
                                eprintln!(
                                    "Warning: Failed to mark stage as merge blocked in graph: {e}"
                                );
                            }
                            false
                        }
                    }
                } else {
                    // No completed_commit - knowledge stage or similar, mark as merged
                    stage.merged = true;
                    if let Err(e) = self.save_stage(&stage) {
                        eprintln!("Warning: Failed to save stage after no-worktree merge: {e}");
                    }
                    true
                }
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
