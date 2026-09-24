//! Stage execution logic - creating worktrees, spawning sessions

use anyhow::{Context, Result};
use chrono::Utc;

use crate::git;
use crate::hooks::find_hooks_dir;
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::scheduling_report::{self, BlockReason, BlockedStage, SchedulingReport};
use crate::orchestrator::signals::{generate_knowledge_signal, DependencyStatus};

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
pub(super) trait StageExecutor: Persistence {
    /// Start ready stages (create worktrees, spawn sessions)
    fn start_ready_stages(&mut self) -> Result<usize>;

    /// Process a single ready stage
    fn start_stage(&mut self, stage_id: &str) -> Result<()>;

    /// Start a knowledge stage (runs in main repo without worktree)
    fn start_knowledge_stage(&mut self, stage: Stage) -> Result<()>;
}

impl StageExecutor for Orchestrator {
    fn start_ready_stages(&mut self) -> Result<usize> {
        // Watchdog + runtime-safety pass; see `coherence::begin_scheduling_pass`.
        self.begin_scheduling_pass();

        let running = self.active_sessions.len();
        let available_slots = self.config.max_parallel_sessions.saturating_sub(running);

        // Every ready stage, in scheduling order — not just the ones that fit
        // in the available slots. The overflow is what the concurrency-limit
        // reason is built from, and it was previously invisible: `.take()`
        // silently dropped it, so a stage held back by a busy slot looked
        // exactly like a stage held back by a broken dependency.
        let ready_ids: Vec<String> = self
            .graph
            .ready_stages()
            .iter()
            .map(|node| node.id.clone())
            .collect();

        // Start a fresh pass: reasons are re-derived every tick so a cleared
        // condition disappears from the report immediately.
        self.spawn_blocks.clear();
        let now = Utc::now();
        for stage_id in &ready_ids {
            self.queued_since.entry(stage_id.clone()).or_insert(now);
        }

        let (schedulable, overflow) = ready_ids.split_at(available_slots.min(ready_ids.len()));

        for stage_id in overflow {
            self.spawn_blocks.insert(
                stage_id.clone(),
                BlockReason::ConcurrencyLimit {
                    running,
                    max: self.config.max_parallel_sessions,
                },
            );
        }

        let mut started = 0;
        for stage_id in schedulable {
            let before = self.active_sessions.len();
            self.start_stage(stage_id)
                .with_context(|| format!("Failed to start stage: {stage_id}"))?;

            // `start_stage` returns Ok(()) whether it spawned or declined, so
            // the session count is what distinguishes the two. Knowledge
            // stages register a session too, so this holds for every path that
            // actually launched an agent.
            if self.active_sessions.len() > before {
                started += 1;
                self.queued_since.remove(stage_id.as_str());
                self.spawn_blocks.remove(stage_id.as_str());
            }
        }

        // Drop bookkeeping for stages that are no longer ready (started,
        // completed, blocked, or re-parked) so "queued for X" never counts
        // time from a previous life.
        self.queued_since.retain(|id, _| ready_ids.contains(id));

        self.publish_scheduling_report();

        Ok(started)
    }

    fn start_stage(&mut self, stage_id: &str) -> Result<()> {
        let mut stage = self.load_stage(stage_id)?;

        // Skip if stage is already executing or completed
        if matches!(
            stage.status,
            StageStatus::Executing | StageStatus::Completed
        ) {
            return Ok(());
        }

        // Skip if stage is held
        if stage.held {
            self.spawn_blocks
                .insert(stage_id.to_string(), BlockReason::Held);
            return Ok(());
        }

        // Refuse to spawn a second agent over one that is still alive. A
        // daemon crash can leave a stage `Executing` with a session that is
        // unreachable (e.g. an orphaned tmux server) but still running; if
        // the stage is later requeued (`loom stage reset`, or any other path
        // that walks it back to `Queued`), scheduling it again here would
        // spawn a duplicate agent into the same worktree alongside the first.
        // Adopt the live session instead of spawning a duplicate.
        if self.adopt_live_session_if_present(stage_id)? {
            return Ok(());
        }

        // Refuse phantom-merge propagation without blocking an unattempted stage.
        // The cached check avoids repeated git work while dependencies are unchanged.
        let target_branch = crate::git::branch::resolve_target_branch(
            &self.config.base_branch,
            &self.config.repo_root,
        );
        match crate::verify::transitions::are_all_dependencies_satisfied_cached(
            &stage,
            &self.config.work_dir,
            &self.config.repo_root,
            &target_branch,
        ) {
            Ok(true) => {}
            Ok(false) => {
                // Cold path only: name the offending dependency so the report
                // can say "waiting on X because Y" instead of a bare refusal.
                let reason = match crate::verify::transitions::describe_dependency_block(
                    &stage,
                    &self.config.work_dir,
                    &self.config.repo_root,
                    &target_branch,
                ) {
                    Ok(Some(block)) => BlockReason::Dependency {
                        dependency: block.dependency,
                        detail: block.detail,
                        self_resolving: block.self_resolving,
                    },
                    // The two checks disagreed (a stage file changed between
                    // them). Report it plainly rather than inventing a cause.
                    Ok(None) => BlockReason::DependencyCheckFailed {
                        detail: "dependencies reported unsatisfied but no blocking \
                                 dependency was found; state changed mid-check"
                            .to_string(),
                    },
                    Err(e) => BlockReason::DependencyCheckFailed {
                        detail: format!("{e}"),
                    },
                };

                if self.spawn_skip_logged.insert(stage_id.to_string()) {
                    tracing::error!(
                        stage_id = %stage_id,
                        reason = %reason.describe(),
                        "Refusing to spawn: dependencies not truly satisfied (likely phantom merge in deps). Run `loom repair` to investigate."
                    );
                }
                self.spawn_blocks.insert(stage_id.to_string(), reason);
                return Ok(());
            }
            Err(e) => {
                if self.spawn_skip_logged.insert(stage_id.to_string()) {
                    tracing::error!(
                        stage_id = %stage_id,
                        error = %e,
                        "Refusing to spawn: dependency satisfaction check errored"
                    );
                }
                self.spawn_blocks.insert(
                    stage_id.to_string(),
                    BlockReason::DependencyCheckFailed {
                        detail: format!("{e}"),
                    },
                );
                return Ok(());
            }
        }

        // Transition through Queued if currently WaitingForDeps to reduce race window
        if stage.status == StageStatus::WaitingForDeps {
            stage = self.update_stage(stage_id, |current| current.try_mark_queued())?;
        }

        // Knowledge stages run in main repo without a worktree.
        // `start_knowledge_stage` itself resolves the session, writes the
        // write-ahead record, and marks the stage Executing (mirroring
        // `spawn_stage_agent`, the worktree spawn path), so this branch only
        // dispatches and contains a failure.
        if stage.stage_type == StageType::Knowledge {
            // Wrap the spawn so a failure does not strand the stage in
            // Executing state. Propagating the error here causes the
            // orchestrator to exit, leaving disk state Executing — and the
            // next `loom run` will then refuse to spawn it (graph keeps it
            // out of ready_stages), polling forever with no progress.
            if let Err(spawn_err) = self.start_knowledge_stage(stage) {
                let err_msg = format!("{spawn_err:#}");
                eprintln!("Knowledge stage '{stage_id}' spawn failed: {err_msg}");
                if self
                    .persist_blocked_stage(
                        stage_id,
                        FailureType::InfrastructureError,
                        vec![err_msg],
                    )
                    .is_ok()
                {
                    let _ = self.graph.mark_status(stage_id, StageStatus::Blocked);
                }
            }
            return Ok(());
        }

        // For worktree stages: attempt worktree creation BEFORE marking as Executing
        // This ensures we don't leave stages in Executing state if worktree creation fails
        let Some((resolved, worktree)) = self.resolve_worktree(stage_id, &stage)? else {
            return Ok(());
        };

        // Run before-stage checks if configured (verify pre-conditions in a
        // pristine worktree). Blocks the stage when they fail.
        if !self.before_stage_gate_passed(&stage, &worktree.path, resolved.branch_name())? {
            return Ok(());
        }

        // A v2 standard stage writes and freezes its contract tests before
        // anything implements them (DESIGN D8).
        let Some(kind) = self.first_agent_kind(&stage) else {
            return Ok(());
        };
        let base = resolved.branch_name().to_string();
        self.spawn_stage_agent(stage, worktree, Some(base), kind)
    }

    fn start_knowledge_stage(&mut self, stage: Stage) -> Result<()> {
        let stage_id = stage.id.clone();

        // Resolve the session and persist a write-ahead record BEFORE the
        // stage is marked Executing, mirroring the worktree spawn path
        // (`spawn_stage_agent`): a daemon crash between "Executing" and a
        // live agent must never leave the stage pointing at a session record
        // that does not exist on disk.
        let Some(session) = self.write_ahead_knowledge_session(&stage_id)? else {
            return Ok(());
        };

        if !self.setup_knowledge_sandbox_and_hooks(&stage, &stage_id, &session.id)? {
            return Ok(());
        }

        let deps = get_dependency_status(&stage, &self.graph);

        let Some(handoff_file) =
            self.continuation_handoff_or_block(&stage.id, stage.session.as_deref(), &session.id)
        else {
            return Ok(());
        };

        // Generate knowledge-specific signal (runs in main repo, no commit required)
        let signal_path = generate_knowledge_signal(
            &session,
            &stage,
            &self.config.repo_root,
            &deps,
            &self.config.work_dir,
            handoff_file.as_deref(),
        )
        .context("Failed to generate knowledge signal file")?;

        // Store original session ID to verify consistency after spawn
        let original_session_id = session.id.clone();

        let spawned_session = if !self.config.manual_mode {
            // Spawn session in the main repo directory (not a worktree)
            match self.backend.spawn_knowledge_session(
                &stage,
                session,
                &signal_path,
                &self.config.repo_root,
            ) {
                Ok(spawned) => {
                    // Print confirmation that stage was started
                    println!("  Started (knowledge): {stage_id}");
                    spawned
                }
                Err(spawn_err) => {
                    let err_msg = format!(
                        "Failed to spawn knowledge session for stage {stage_id}: {spawn_err:#}"
                    );
                    self.block_and_undo_session(
                        &stage_id,
                        &original_session_id,
                        super::crash_classification::spawn_failure_type(&spawn_err),
                        err_msg,
                    );
                    return Ok(());
                }
            }
        } else {
            println!("Manual mode: Session setup for knowledge stage '{stage_id}'");
            println!("  Directory: {}", self.config.repo_root.display());
            println!("  Signal: {}", signal_path.display());
            println!(
                "  To start: cd {} && claude \"Read the signal file at {} and execute the assigned stage work.\"",
                self.config.repo_root.display(),
                signal_path.display()
            );
            session
        };

        // Verify session ID consistency (signal file uses this ID)
        debug_assert_eq!(
            original_session_id, spawned_session.id,
            "Session ID mismatch: signal file created with '{}' but saving session with '{}'",
            original_session_id, spawned_session.id
        );

        self.save_session(&spawned_session)?;

        // Knowledge stages don't have a worktree; assign_session already
        // happened before the spawn (write-ahead, above), so only clear the
        // executor-owned worktree fields here.
        self.update_stage(&stage_id, |current| {
            current.set_worktree(None);
            current.set_resolved_base(None);
            Ok(())
        })?;

        // Add to active sessions but NOT to active_worktrees (no worktree for knowledge stages)
        self.insert_active_session(&stage_id, spawned_session);

        Ok(())
    }
}

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
    fn before_stage_gate_passed(
        &mut self,
        stage: &Stage,
        worktree_path: &std::path::Path,
        base_branch: &str,
    ) -> Result<bool> {
        if stage.before_stage.is_empty() {
            return Ok(true);
        }

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
            return Ok(true);
        }

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
