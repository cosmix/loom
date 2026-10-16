//! Stage execution logic - creating worktrees, spawning sessions

use anyhow::{Context, Result};
use chrono::Utc;

use crate::models::failure::{FailureInfo, FailureType};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::scheduling_report::{self, BlockReason, BlockedStage, SchedulingReport};
use crate::orchestrator::signals::DependencyStatus;

use super::persistence::Persistence;
use super::Orchestrator;

impl Orchestrator {
    pub(super) fn persist_blocked_stage(
        &self,
        stage_id: &str,
        failure_type: FailureType,
        evidence: Vec<String>,
    ) -> Result<()> {
        self.update_stage(stage_id, |current| {
            current.try_mark_blocked()?;
            current.failure_info = Some(FailureInfo {
                failure_type,
                detected_at: Utc::now(),
                evidence,
            });
            Ok(())
        })?;
        Ok(())
    }

    fn start_worktree_stage(&mut self, stage: Stage) -> Result<StartDisposition> {
        let stage_id = stage.id.as_str();
        // For worktree stages: attempt worktree creation BEFORE marking as Executing
        // This ensures we don't leave stages in Executing state if worktree creation fails
        let Some((resolved, worktree)) = self.resolve_worktree(stage_id, &stage)? else {
            return Ok(StartDisposition::Deferred);
        };

        // Run the before-stage checks on the pristine worktree, then provision
        // it. Either one failing blocks the stage.
        if !self.pre_spawn_gates_passed(&stage, &worktree.path, resolved.branch_name())? {
            return Ok(StartDisposition::Deferred);
        }

        // A v2 standard stage writes and freezes its contract tests before
        // anything implements them (DESIGN D8).
        let Some(kind) = self.first_agent_kind(&stage) else {
            return Ok(StartDisposition::Deferred);
        };
        let base = resolved.branch_name().to_string();
        self.spawn_stage_agent(stage, worktree, Some(base), kind)
    }

    /// Publish the current tick's "why isn't this stage running" snapshot to
    /// `.loom/work/scheduling.json` for the dashboards to read.
    ///
    /// Written every pass, including when nothing is blocked — an empty report
    /// is the signal that the previous complaint has cleared.
    fn publish_scheduling_report(&self) {
        let mut blocked: Vec<BlockedStage> = self
            .spawn_blocks
            .iter()
            .filter_map(|(stage_id, reason)| {
                Some(BlockedStage {
                    stage_id: stage_id.clone(),
                    queued_since: *self.queued_since.get(stage_id)?,
                    reason: reason.clone(),
                })
            })
            .collect();

        // Stable order so the dashboards do not reshuffle between frames.
        blocked.sort_by(|a, b| a.stage_id.cmp(&b.stage_id));

        scheduling_report::write(&self.config.work_dir, &SchedulingReport { blocked });
    }
}

/// Confirm the stage's Claude Code hooks directory exists, or fail.
///
/// Hooks are the stage's security boundary — the commit filter, git-add guard,
/// worktree file guard and subagent verify guard all arrive this way — not an
/// optional enhancement. A missing hooks directory is therefore an error, not a
/// silent skip: spawning without them would run the agent unguarded.
///
/// Every session now launches from a per-session capsule
/// (`native/session_settings.rs`) that embeds the hooks configuration
/// itself, so this no longer writes `.claude/settings.local.json` — it only
/// refuses the spawn when the directory backing those hooks is missing.
pub(super) fn install_required_hooks(
    hooks_dir: Option<std::path::PathBuf>,
    stage_id: &str,
) -> Result<()> {
    hooks_dir.ok_or_else(|| {
        anyhow::anyhow!(
            "Claude Code hooks directory not found; refusing to spawn an unhooked session for stage '{stage_id}'"
        )
    })?;
    Ok(())
}

/// Trait for stage execution operations
#[derive(Debug, PartialEq, Eq)]
pub(super) enum StartDisposition {
    Launched,
    Adopted,
    Deferred,
}

pub(super) trait StageExecutor: Persistence {
    /// Start ready stages (create worktrees, spawn sessions)
    fn start_ready_stages(&mut self) -> Result<usize>;

    /// Process a single ready stage
    fn start_stage(&mut self, stage_id: &str) -> Result<StartDisposition>;

    /// Start a knowledge stage (runs in main repo without worktree)
    fn start_knowledge_stage(&mut self, stage: Stage) -> Result<()>;
}

impl StageExecutor for Orchestrator {
    fn start_ready_stages(&mut self) -> Result<usize> {
        // Watchdog + runtime-safety pass; see `coherence::begin_scheduling_pass`.
        self.begin_scheduling_pass();

        let ready_ids = self.stage_snapshot()?.ready_ids();
        self.spawn_blocks.clear();
        let now = Utc::now();
        let mut started = 0;
        for stage_id in &ready_ids {
            self.queued_since.entry(stage_id.clone()).or_insert(now);
            let running = self.active_sessions.len();
            if running >= self.config.max_parallel_sessions {
                self.spawn_blocks.insert(
                    stage_id.clone(),
                    BlockReason::ConcurrencyLimit {
                        running,
                        max: self.config.max_parallel_sessions,
                    },
                );
                continue;
            }
            match self
                .start_stage(stage_id)
                .with_context(|| format!("Failed to start stage: {stage_id}"))?
            {
                StartDisposition::Deferred => {}
                StartDisposition::Launched | StartDisposition::Adopted => {
                    started += 1;
                    self.queued_since.remove(stage_id);
                    self.spawn_blocks.remove(stage_id);
                }
            }
        }

        // Drop bookkeeping for stages that are no longer ready (started,
        // completed, blocked, or re-parked) so "queued for X" never counts
        // time from a previous life.
        self.queued_since.retain(|id, _| ready_ids.contains(id));

        self.publish_scheduling_report();

        Ok(started)
    }

    fn start_stage(&mut self, stage_id: &str) -> Result<StartDisposition> {
        let mut stage = self.load_stage(stage_id)?;

        // The graph handed this stage over as Queued; its file decides.
        if !matches!(
            stage.status,
            StageStatus::Queued | StageStatus::WaitingForDeps
        ) {
            self.sync_unstartable_node(stage_id, &stage.status);
            return Ok(StartDisposition::Deferred);
        }

        // Skip a held stage, and a knowledge stage while the target is held
        if let Some(reason) = self.spawn_hold_reason(&stage) {
            self.spawn_blocks.insert(stage_id.to_string(), reason);
            return Ok(StartDisposition::Deferred);
        }

        // Adopt a still-live agent instead of spawning a duplicate into its
        // worktree. Only a Queued stage adopts; a WaitingForDeps one is
        // offered adoption once the dependency check below has queued it.
        if stage.status == StageStatus::Queued && self.adopt_live_session_if_present(stage_id)? {
            return Ok(StartDisposition::Adopted);
        }

        if !self.spawn_dependencies_satisfied(&stage) {
            return Ok(StartDisposition::Deferred);
        }

        // Transition through Queued if currently WaitingForDeps to reduce race window
        if stage.status == StageStatus::WaitingForDeps {
            stage = self.update_stage(stage_id, |current| current.try_mark_queued())?;
            if self.adopt_live_session_if_present(stage_id)? {
                return Ok(StartDisposition::Adopted);
            }
        }

        if stage.stage_type == StageType::Knowledge {
            return self.start_guarded_knowledge(stage);
        }

        self.start_worktree_stage(stage)
    }

    fn start_knowledge_stage(&mut self, stage: Stage) -> Result<()> {
        self.launch_knowledge_stage(stage)
    }
}

/// Get dependency status for signal generation
pub(super) fn get_dependency_status(
    stage: &Stage,
    graph: &crate::plan::ExecutionGraph,
) -> Vec<DependencyStatus> {
    stage
        .dependencies
        .iter()
        .map(|dep_id| {
            let (status, outputs) = if let Some(node) = graph.get_node(dep_id) {
                (format!("{:?}", node.status), node.outputs.clone())
            } else {
                ("Unknown".to_string(), Vec::new())
            };

            DependencyStatus {
                stage_id: dep_id.clone(),
                name: dep_id.clone(),
                status,
                outputs,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "stage_executor_tests.rs"]
mod stage_executor_tests;
