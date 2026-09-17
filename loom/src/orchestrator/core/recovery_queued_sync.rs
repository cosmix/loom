//! Sync queued status from the execution graph back to stage files, split
//! out of `recovery.rs` so `Orchestrator::sync_queued_status_to_files` (the
//! `Recovery` trait method) stays a one-line delegation to
//! [`Orchestrator::sync_queued_files`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::update_stage_at_path;

use super::recovery::{load_stage_at_path, scan_stage_paths, StageScanCounter};
use super::recovery_guards;
use super::Orchestrator;

impl Orchestrator {
    /// Sync queued status from graph back to stage files. This ensures
    /// files reflect when dependencies are satisfied. Syncs FROM graph TO
    /// files.
    pub(super) fn sync_queued_files(&mut self) -> Result<()> {
        // Get all nodes that are Queued in the graph
        let queued_stage_ids: Vec<String> = self
            .graph
            .all_nodes()
            .iter()
            .filter(|node| node.status == StageStatus::Queued)
            .map(|node| node.id.clone())
            .collect();

        let stages_dir = self.config.work_dir.join("stages");
        if !stages_dir.exists() {
            return Ok(());
        }
        let mut scan = StageScanCounter::default();
        let stage_paths: HashMap<String, PathBuf> = scan_stage_paths(&stages_dir, &mut scan)?
            .into_iter()
            .filter_map(|path| {
                let filename = path.file_name()?.to_str()?;
                crate::fs::stage_files::extract_stage_id(filename).map(|id| (id, path))
            })
            .collect();

        // Resolve once per pass, matching `sync_graph_with_stage_files` (P-3):
        // otherwise a Completed-stage git call would run once per queued
        // stage every tick.
        let target_branch = crate::git::branch::resolve_target_branch(
            &self.config.base_branch,
            &self.config.repo_root,
        );

        // For each queued stage, update the file if it's still WaitingForDeps
        for stage_id in queued_stage_ids {
            let Some(stage_path) = stage_paths.get(&stage_id) else {
                tracing::error!(
                    stage_id = %stage_id,
                    "Failed to locate stage during queued-status sync"
                );
                continue;
            };
            self.sync_one_queued_stage(&stage_id, stage_path, &target_branch);
        }

        Ok(())
    }

    /// Reconcile a single queued stage's file against the graph's verdict,
    /// re-checking its own dependencies before writing `Queued`.
    fn sync_one_queued_stage(&mut self, stage_id: &str, stage_path: &Path, target_branch: &str) {
        // A stale graph can consider a stage ready while the FILE's own
        // dependencies are unmet (e.g. after `.loom/work` was recreated
        // for a different plan under a running daemon). Load once here
        // to re-check with the same guard the spawn path uses — the
        // `update_stage_at_path` closure below cannot borrow `self` to
        // run that check itself.
        let current = match load_stage_at_path(stage_path) {
            Ok(stage) => stage,
            Err(error) => {
                tracing::error!(
                    stage_id = %stage_id,
                    %error,
                    "Failed to load stage during queued-status sync; skipping (corrupt stage file?)"
                );
                return;
            }
        };

        if current.status == StageStatus::WaitingForDeps
            && !self.park_if_dependencies_unmet(stage_id, &current, target_branch)
        {
            return;
        }

        let updated = update_stage_at_path(stage_id, stage_path, &self.config.work_dir, |stage| {
            if stage.status == StageStatus::WaitingForDeps {
                stage.try_mark_queued()?;
            }
            Ok(())
        });
        match updated {
            Ok(stage) if stage.status != StageStatus::Queued => {
                if let Err(error) = self.graph.mark_status(stage_id, stage.status.clone()) {
                    tracing::warn!(
                        stage_id = %stage_id,
                        %error,
                        "Failed to sync concurrently updated queued-stage status"
                    );
                }
            }
            Ok(_) => {}
            Err(error) => tracing::error!(
                stage_id = %stage_id,
                %error,
                "Failed to update stage during queued-status sync; skipping (corrupt stage file?)"
            ),
        }
    }

    /// Re-check `current`'s own dependencies before letting the caller write
    /// `Queued` into its file. Returns `true` when it is safe to proceed
    /// with the write; `false` when the stage was parked at `WaitingForDeps`
    /// (dependencies unmet) or the check itself failed.
    fn park_if_dependencies_unmet(
        &mut self,
        stage_id: &str,
        current: &Stage,
        target_branch: &str,
    ) -> bool {
        match recovery_guards::queued_writeback_verdict(
            current,
            &self.config.work_dir,
            &self.config.repo_root,
            target_branch,
        ) {
            recovery_guards::QueuedWriteback::Write => true,
            recovery_guards::QueuedWriteback::DependenciesUnmet => {
                if self
                    .spawn_skip_logged
                    .insert(format!("queued-writeback:{stage_id}"))
                {
                    tracing::warn!(
                        stage_id = %stage_id,
                        "Execution graph considers stage ready but its stage file's \
                         dependencies are not complete and merged; leaving the file \
                         WaitingForDeps"
                    );
                }
                if let Err(error) = self
                    .graph
                    .force_status(stage_id, StageStatus::WaitingForDeps)
                {
                    tracing::warn!(
                        stage_id = %stage_id,
                        %error,
                        "Failed to park stage at WaitingForDeps after a failed \
                         queued-writeback dependency re-check"
                    );
                }
                false
            }
            recovery_guards::QueuedWriteback::CheckFailed(error) => {
                tracing::warn!(
                    stage_id = %stage_id,
                    %error,
                    "Dependency re-check errored during queued-status sync; skipping \
                     this stage this tick"
                );
                false
            }
        }
    }
}
