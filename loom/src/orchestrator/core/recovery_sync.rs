//! Project complete stage snapshots into the disposable graph and apply guarded retries.
use super::{recovery::check_retry_eligibility, Orchestrator};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::verify::transitions::{load_stage, update_stage_at_path};
use anyhow::{Context, Result};
use std::path::Path;

impl Orchestrator {
    pub(super) fn sync_authoritative_stages(&mut self) -> Result<()> {
        let snapshot = self.stage_snapshot()?;
        let target = crate::git::branch::resolve_target_branch(
            &self.config.base_branch,
            &self.config.repo_root,
        );
        let mut merge_candidates = Vec::new();
        for (mut stage, path) in snapshot.records.into_values() {
            self.abort_if_foreign_plan(&stage);
            if stage.status == StageStatus::Completed {
                self.recover_completed_evidence(&mut stage, &path, &target);
                if stage.status == StageStatus::Completed && !stage.merged {
                    merge_candidates.push(stage.id.clone());
                }
            } else if stage.status == StageStatus::Blocked {
                stage = self.apply_retry_if_due(stage, &path)?;
            }
            self.project_stage(&stage)?;
        }
        for id in merge_candidates {
            if self.merge_retry_attempted.insert(id.clone()) {
                let _ = self.try_auto_merge(&id);
                self.reproject_from_file(&id)?;
            }
        }
        self.graph.refresh_ready_status();
        Ok(())
    }

    /// Re-project a stage whose file changed during this pass (auto-merge
    /// writes `merged` and may change status) so readiness is computed from
    /// the same state the file-based schedulers see. A failed reload is
    /// logged and skipped; the next pass re-projects it.
    fn reproject_from_file(&mut self, stage_id: &str) -> Result<()> {
        match load_stage(stage_id, &self.config.work_dir) {
            Ok(stage) => self.project_stage(&stage),
            Err(error) => {
                tracing::warn!(stage_id, %error, "Cannot reload stage after auto-merge");
                Ok(())
            }
        }
    }

    /// Bring the graph node to Executing after the stage file already is. The
    /// file is authoritative and a session is about to run, so a node that is
    /// not Queued (stale projection) is forced rather than killing the daemon
    /// with the file Executing and no live session.
    pub(super) fn mark_graph_executing(&mut self, stage_id: &str) -> Result<()> {
        if let Err(error) = self.graph.mark_executing(stage_id) {
            tracing::warn!(stage_id, %error, "Graph disagrees with Executing stage file; forcing node");
            self.graph
                .force_status(stage_id, StageStatus::Executing)
                .context("Failed to mark stage as executing in graph")?;
        }
        Ok(())
    }

    fn apply_retry_if_due(&self, stage: Stage, path: &Path) -> Result<Stage> {
        if !check_retry_eligibility(&stage) {
            return Ok(stage);
        }
        update_stage_at_path(&stage.id, path, &self.config.work_dir, |current| {
            if current.status == StageStatus::Blocked && check_retry_eligibility(current) {
                current.try_mark_queued()?;
            }
            Ok(())
        })
    }

    fn project_stage(&mut self, stage: &Stage) -> Result<()> {
        self.graph
            .set_node_outputs(&stage.id, stage.outputs.clone());
        self.graph.set_node_merged(&stage.id, stage.merged);
        match stage.status {
            StageStatus::Queued => {
                if self.graph.mark_queued(&stage.id).is_err() {
                    self.graph
                        .force_status(&stage.id, StageStatus::WaitingForDeps)?;
                }
            }
            StageStatus::Executing => self.sync_resumed_node(&stage.id),
            StageStatus::Completed => {
                self.graph.mark_completed(&stage.id)?;
            }
            _ => self.graph.mark_status(&stage.id, stage.status.clone())?,
        }
        Ok(())
    }

    fn recover_completed_evidence(&mut self, stage: &mut Stage, path: &Path, target: &str) {
        if stage.merged
            && stage.stage_type != StageType::Knowledge
            && !self.verified_merged.contains(&stage.id)
        {
            if self.verify_merged_true_or_revert(stage, path, target) {
                self.verified_merged.insert(stage.id.clone());
            } else {
                self.verified_merged.remove(&stage.id);
            }
        }
        if stage.merged || stage.status != StageStatus::Completed {
            return;
        }
        let Some(commit) = stage.completed_commit.clone() else {
            return;
        };
        if !self
            .probe_accepted(&stage.id, &commit, target)
            .unwrap_or(false)
        {
            return;
        }
        let result = update_stage_at_path(&stage.id, path, &self.config.work_dir, |current| {
            if current.status == StageStatus::Completed
                && current.completed_commit.as_deref() == Some(commit.as_str())
            {
                current.merged = true;
            }
            Ok(())
        });
        match result {
            Ok(updated) => {
                *stage = updated;
                if stage.merged {
                    self.verified_merged.insert(stage.id.clone());
                }
            }
            Err(error) => tracing::warn!(%error, "Cannot save verified merge state"),
        }
    }
}
