//! Stage execution logic - creating worktrees, spawning sessions

use std::ops::ControlFlow;

use anyhow::Result;
use chrono::Utc;

use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::scheduling_report::{self, BlockReason, BlockedStage, SchedulingReport};
use crate::orchestrator::signals::DependencyStatus;

use super::lifecycle_snapshot::StageSnapshot;
use super::persistence::Persistence;
use super::session_adoption::AdoptionResult;
use super::Orchestrator;

impl Orchestrator {
    fn adoption_disposition(&mut self, stage_id: &str) -> Result<Option<StartDisposition>> {
        Ok(match self.adopt_live_session_if_present(stage_id)? {
            AdoptionResult::Adopted => Some(StartDisposition::Adopted),
            AdoptionResult::Unknown => Some(StartDisposition::Deferred),
            AdoptionResult::Absent => None,
        })
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

    /// Clear a launched stage's scheduling bookkeeping and republish the
    /// report at once: a launch can take minutes (brief generation), and the
    /// started stage must not sit in the report until the pass ends.
    fn record_started(&mut self, stage_id: &str) {
        self.queued_since.remove(stage_id);
        self.spawn_blocks.remove(stage_id);
        self.publish_scheduling_report();
    }

    /// Readiness comes from the stage files, base-branch resolution from the
    /// graph. A dependency completed and merged by an out-of-process write
    /// (`loom stage complete` on a knowledge stage) after this tick's graph
    /// sync would otherwise be ready by its file but `Executing` in the graph,
    /// and every child would be refused as `SchedulingNotReady`.
    fn project_ready_dependencies(&mut self, snapshot: &StageSnapshot, ready_ids: &[String]) {
        for stage_id in ready_ids {
            let Some((stage, _)) = snapshot.records.get(stage_id) else {
                continue;
            };
            for dep in &stage.dependencies {
                self.graph.set_node_merged(dep, true);
                if let Err(error) = self.graph.mark_completed(dep) {
                    tracing::warn!(dep = %dep, %error, "Cannot project completed dependency");
                }
            }
        }
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

        let snapshot = self.stage_snapshot()?;
        let ready_ids = snapshot.ready_ids();
        self.project_ready_dependencies(&snapshot, &ready_ids);
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
            match self.start_stage(stage_id) {
                Ok(StartDisposition::Deferred) => {}
                Ok(StartDisposition::Launched | StartDisposition::Adopted) => {
                    started += 1;
                    self.record_started(stage_id);
                }
                // One stage's lock or file must not end the daemon for the rest.
                Err(error) => {
                    let detail = format!("failed to start: {error:#}");
                    tracing::warn!(stage_id = %stage_id, %detail, "Stage not started; retrying next tick");
                    self.spawn_blocks
                        .insert(stage_id.clone(), BlockReason::OwnershipUncertain { detail });
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
        // Reset holds this same per-stage lock from its liveness scan through
        // retirement and publication. Keep it through launch so reset cannot
        // inspect a write-ahead session before its process exists.
        let _admission =
            crate::fs::stage_admission::lock_for_daemon(&self.config.work_dir, stage_id)?;
        let mut stage = match self.startable_stage(stage_id)? {
            ControlFlow::Continue(stage) => stage,
            ControlFlow::Break(disposition) => return Ok(disposition),
        };

        if !self.spawn_dependencies_satisfied(&stage) {
            return Ok(StartDisposition::Deferred);
        }

        if stage.status == StageStatus::WaitingForDeps {
            match self.queue_waiting_stage(stage_id)? {
                ControlFlow::Continue(queued) => stage = queued,
                ControlFlow::Break(disposition) => return Ok(disposition),
            }
        }

        // Both adoption points are behind us: a handle still tracked now is a
        // session no adoption claimed, and a spawn must not run beside it.
        if !self.tracked_incumbent_cleared(stage_id) {
            return Ok(StartDisposition::Deferred);
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

impl Orchestrator {
    /// Load `stage_id` and settle what needs no dependency check: a stage the
    /// graph handed over but whose file says otherwise, a held stage, and a
    /// still-live agent worth adopting. Only a Queued stage adopts here; a
    /// WaitingForDeps one is offered adoption by `queue_waiting_stage`. `Break`
    /// carries the disposition. Runs under the admission lock `start_stage` holds.
    fn startable_stage(&mut self, stage_id: &str) -> Result<ControlFlow<StartDisposition, Stage>> {
        let stage = self.load_stage(stage_id)?;
        // The graph handed this stage over as Queued; its file decides.
        if !matches!(
            stage.status,
            StageStatus::Queued | StageStatus::WaitingForDeps
        ) {
            self.sync_unstartable_node(stage_id, &stage.status);
            return Ok(ControlFlow::Break(StartDisposition::Deferred));
        }
        // Skip a held stage, and a knowledge stage while the target is held
        if let Some(reason) = self.spawn_hold_reason(&stage) {
            self.spawn_blocks.insert(stage_id.to_string(), reason);
            return Ok(ControlFlow::Break(StartDisposition::Deferred));
        }
        // Adopt a still-live agent instead of spawning a duplicate into its worktree.
        if stage.status == StageStatus::Queued {
            if let Some(disposition) = self.adoption_disposition(stage_id)? {
                return Ok(ControlFlow::Break(disposition));
            }
        }
        Ok(ControlFlow::Continue(stage))
    }

    /// Transition a WaitingForDeps stage through Queued to reduce the race
    /// window, then offer the second adoption point. `Break` carries the
    /// disposition when an agent was adopted instead. Runs under the admission
    /// lock `start_stage` holds.
    fn queue_waiting_stage(
        &mut self,
        stage_id: &str,
    ) -> Result<ControlFlow<StartDisposition, Stage>> {
        let queued = self.update_stage(stage_id, |current| current.try_mark_queued())?;
        Ok(match self.adoption_disposition(stage_id)? {
            Some(disposition) => ControlFlow::Break(disposition),
            None => ControlFlow::Continue(queued),
        })
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
