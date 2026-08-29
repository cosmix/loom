//! Stage execution logic - creating worktrees, spawning sessions

use anyhow::{Context, Result};
use chrono::Utc;

use crate::git;
use crate::git::worktree::setup_worktree_hooks;
use crate::handoff::find_latest_handoff;
use crate::hooks::find_hooks_dir;
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::merge_lifecycle::MergeLifecycle;
use crate::orchestrator::scheduling_report::{self, BlockReason, BlockedStage, SchedulingReport};
use crate::orchestrator::signals::{
    generate_knowledge_signal, generate_signal_with_skills, DependencyStatus,
};

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
    /// `.work/scheduling.json` for the dashboards to read.
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

pub(super) fn write_required_sandbox_settings(
    config: &crate::sandbox::MergedSandboxConfig,
    target: &std::path::Path,
    stage_id: &str,
) -> Result<()> {
    crate::sandbox::write_settings(config, target)
        .with_context(|| format!("Failed to enforce sandbox settings for stage '{stage_id}'"))
}

/// Install the stage's Claude Code hooks, or fail.
///
/// Hooks are the stage's security boundary — the commit filter, git-add guard,
/// worktree file guard and subagent verify guard all arrive this way — not an
/// optional enhancement. A missing hooks directory is therefore an error, not a
/// silent skip: spawning without them would run the agent unguarded.
///
/// `worktree_path` is the target that receives `.claude/settings.local.json`:
/// a stage worktree for standard stages, or the main repo root for knowledge
/// stages (which run on the host directly rather than in a worktree).
pub(super) fn install_required_hooks(
    hooks_dir: Option<std::path::PathBuf>,
    worktree_path: &std::path::Path,
    work_dir: &std::path::Path,
    permission_mode: crate::plan::schema::PermissionMode,
    stage_id: &str,
) -> Result<()> {
    let hooks_dir = hooks_dir.ok_or_else(|| {
        anyhow::anyhow!(
            "Claude Code hooks directory not found; refusing to spawn an unhooked session for stage '{stage_id}'"
        )
    })?;
    setup_worktree_hooks(worktree_path, work_dir, &hooks_dir, permission_mode)
        .with_context(|| format!("Failed to install Claude Code hooks for stage '{stage_id}'"))
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
        // Privileged completion capabilities are command-scoped and must never
        // cross into an agent runtime, even if the daemon itself was launched
        // from a shell that happened to carry them.
        crate::commands::stage::complete::strip_privileged_env_for_runtime();

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
        // write-ahead record, and marks the stage Executing (mirroring the
        // worktree spawn path below), so this branch only dispatches and
        // contains a failure.
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

        // Honor a pending recovery signal (C-5) and resolve the session id up
        // front. `loom stage retry --context` (and crash/hung auto-recovery)
        // writes a `recovery-<...>` signal file keyed to a new session ID and
        // stores that ID in `stage.session`. If such a signal exists, reuse
        // its session ID (and, once the signal path is resolved further
        // below, its signal file) so the new agent actually receives the
        // recovery context, instead of overwriting it with a freshly
        // generated signal.
        //
        // Also writes the session record BEFORE the stage is marked
        // Executing (see `write_ahead_session`'s invariant doc): a daemon
        // crash must never produce a live, unreachable agent with no record
        // on disk at all.
        let Some((session, recovery_signal)) = self.write_ahead_session(&stage, stage_id) else {
            return Ok(());
        };

        // Worktree created successfully - NOW mark as Executing, linked to
        // the session record written above, in ONE locked update so
        // "Executing" and "session assigned" can never be observed apart.
        let session_id = session.id.clone();
        stage = match self.update_stage(stage_id, |current| {
            current.try_mark_executing()?;
            current.begin_attempt(Utc::now());
            current.assign_session(session_id.clone());
            Ok(())
        }) {
            Ok(stage) => stage,
            Err(e) => {
                self.block_and_undo_session(
                    stage_id,
                    &session.id,
                    FailureType::InfrastructureError,
                    format!("Failed to mark stage executing: {e:#}"),
                );
                return Ok(());
            }
        };
        self.graph
            .mark_executing(stage_id)
            .context("Failed to mark stage as executing in graph")?;

        // Generate and write sandbox settings to worktree
        let mut merged_sandbox = crate::sandbox::merge_config(
            &self.config.sandbox_config,
            &stage.sandbox,
            stage.stage_type,
            &stage.implementers,
        );
        // Defense-in-depth: re-validate at spawn time. `loom init` already
        // rejects incompatible configs; refuse to spawn rather than silently
        // downgrade if the on-disk config has since become invalid.
        if let Err(e) = crate::sandbox::validate_config(&merged_sandbox) {
            self.block_and_undo_session(
                stage_id,
                &session.id,
                FailureType::InfrastructureError,
                format!("invalid sandbox config at spawn: {e:#}"),
            );
            return Ok(());
        }
        crate::sandbox::expand_paths(&mut merged_sandbox);
        if let Err(error) =
            write_required_sandbox_settings(&merged_sandbox, &worktree.path, stage_id)
        {
            self.block_and_undo_session(
                stage_id,
                &session.id,
                FailureType::InfrastructureError,
                format!("{error:#}"),
            );
            return Ok(());
        }

        // Refresh the stage's source-graph overlay BEFORE the signal is
        // generated below: the Knowledge Brief embedded in the signal is
        // built from the overlay, so a stale overlay would brief the agent
        // from the pre-stage tree. This mirrors the reconcile
        // `merge_handler.rs` already does before a merge.
        //
        // `start_knowledge_stage` (above) deliberately does not get this
        // call: it runs in the main repo with no worktree, so
        // `reconcile_overlay` would early-return at merge_lifecycle.rs:76-79
        // anyway - adding it there would just add a pointless full walk of
        // the main repo on every knowledge stage.
        //
        // `reconcile_source_graph` is incremental (it reuses a cached entry
        // whenever `body_hash` matches, see refresh/source_graph.rs:212-245),
        // so steady-state cost is proportional to changed files; only the
        // first call on a fresh worktree pays a full walk.
        MergeLifecycle::new(stage_id, &self.config.repo_root, &self.config.work_dir)
            .reconcile_overlay();

        // Claude Code hooks are the stage's security boundary, not an optional
        // enhancement: a session is never spawned without them. Contain a
        // setup failure as Blocked rather than propagating it, which would kill
        // the daemon while the stage sits Executing with no session (O-11).
        if let Err(e) = install_required_hooks(
            find_hooks_dir(),
            &worktree.path,
            &self.config.work_dir,
            merged_sandbox.permission_mode,
            stage_id,
        ) {
            self.block_and_undo_session(
                stage_id,
                &session.id,
                FailureType::SandboxSetupFailure,
                format!("{e:#}"),
            );
            return Ok(());
        }

        let signal_path = if let Some((_, recovery_path)) = recovery_signal {
            // Reuse the pre-written recovery signal.
            recovery_path
        } else {
            let deps = get_dependency_status(&stage, &self.graph);

            // Check for existing handoff file to include in signal for continuation
            let handoff_file = find_latest_handoff(&stage.id, &self.config.work_dir)
                .ok()
                .flatten()
                .and_then(|p| {
                    p.file_stem()
                        .and_then(|s| s.to_str().map(|s| s.to_string()))
                });

            // Generating the signal can fail (e.g. unwritable signals dir).
            // Contain it: mark Blocked rather than propagating and killing the
            // daemon while the stage is Executing with no session yet (O-11).
            match generate_signal_with_skills(
                &session,
                &stage,
                &worktree,
                &deps,
                handoff_file.as_deref(),
                None, // git_history will be extracted from worktree in future enhancement
                &self.config.work_dir,
                self.skill_index.as_ref(),
                &self.detected_languages,
            ) {
                Ok(path) => path,
                Err(e) => {
                    self.block_and_undo_session(
                        stage_id,
                        &session.id,
                        FailureType::InfrastructureError,
                        format!("Failed to generate signal file: {e:#}"),
                    );
                    return Ok(());
                }
            }
        };

        // Stale recovery signals from earlier attempts must not accumulate.
        self.cleanup_stale_recovery_signals(stage_id, &session.id);

        // Store original session ID to verify consistency after spawn
        let original_session_id = session.id.clone();

        let spawned_session = if !self.config.manual_mode {
            // Wrap spawn so failure transitions the stage to Blocked rather
            // than propagating to the orchestrator loop and killing the
            // daemon. Without this, a transient spawn error strands the
            // stage in Executing on disk; subsequent `loom run` invocations
            // poll forever because Executing stages are never re-spawned.
            match self
                .backend
                .spawn_session(&stage, &worktree, session, &signal_path)
            {
                Ok(spawned) => {
                    println!("  Started: {stage_id}");
                    spawned
                }
                Err(spawn_err) => {
                    let err_msg =
                        format!("Failed to spawn session for stage {stage_id}: {spawn_err:#}");
                    // Remove orphan resources so a retry can start clean.
                    // Worktree — best-effort force-removal; ignore "not found" etc.
                    let _ = git::remove_worktree(stage_id, &self.config.repo_root, true);
                    // Branch — force-delete so the next retry can recreate
                    // it from the correct base.
                    let branch = git::branch_name_for_stage(stage_id);
                    let _ = git::delete_branch(&branch, true, &self.config.repo_root);
                    self.block_and_undo_session(
                        stage_id,
                        &original_session_id,
                        FailureType::InfrastructureError,
                        err_msg,
                    );
                    return Ok(());
                }
            }
        } else {
            println!("Manual mode: Session setup for stage '{stage_id}'");
            println!("  Worktree: {}", worktree.path.display());
            println!("  Signal: {}", signal_path.display());
            // Identity env vars are normally exported by the wrapper script;
            // in manual mode the user must provide them so hooks and
            // `loom memory` attribute work to the right stage/session.
            let absolute_work_dir = self
                .config
                .work_dir
                .canonicalize()
                .unwrap_or_else(|_| self.config.work_dir.clone());
            println!(
                "  To start: cd {} && LOOM_STAGE_ID={} LOOM_SESSION_ID={} LOOM_WORK_DIR={} claude \"Read the signal file at {} and execute the assigned stage work.\"",
                worktree.path.display(),
                stage_id,
                session.id,
                absolute_work_dir.display(),
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

        // Persisting the update (pid, Running status) can fail even though a
        // real agent is now running: the write-ahead `save_session` above
        // already created the record and linked `stage.session` to it, so
        // orphan recovery and `loom attach` can still find the session even
        // if this particular update is lost. Contain the failure: mark
        // Blocked + InfrastructureError so a retry can clean up, rather than
        // propagating and killing the daemon (O-11).
        if let Err(e) = self.save_session(&spawned_session) {
            let err_msg = format!("Failed to save session for stage {stage_id}: {e:#}");
            self.block_stranded_stage(stage_id, err_msg);
            return Ok(());
        }

        super::stage_telemetry::record_context_telemetry(self, &stage, &spawned_session.id);
        // Merge only executor-owned fields into the fresh record under lock,
        // so the slow spawn cannot clobber a concurrent CLI update (O-22).
        // Session assignment already happened before the spawn (write-ahead,
        // above); only the worktree/base fields the spawn just learned land
        // here.
        let worktree_id = worktree.id.clone();
        let resolved_base = resolved.branch_name().to_string();
        if let Err(e) = self.update_stage(stage_id, |current| {
            current.set_worktree(Some(worktree_id));
            current.set_resolved_base(Some(resolved_base));
            Ok(())
        }) {
            let err_msg = format!("Failed to save stage after spawn for {stage_id}: {e:#}");
            self.block_stranded_stage(stage_id, err_msg);
            return Ok(());
        }

        self.insert_active_session(stage_id, spawned_session);
        self.active_worktrees.insert(stage_id.to_string(), worktree);

        Ok(())
    }

    fn start_knowledge_stage(&mut self, stage: Stage) -> Result<()> {
        let stage_id = stage.id.clone();

        // Resolve the session and persist a write-ahead record BEFORE the
        // stage is marked Executing, mirroring the worktree spawn path above:
        // a daemon crash between "Executing" and a live agent must never
        // leave the stage pointing at a session record that does not exist
        // on disk.
        let Some(session) = self.write_ahead_knowledge_session(&stage_id)? else {
            return Ok(());
        };

        if !self.setup_knowledge_sandbox_and_hooks(&stage, &stage_id, &session.id)? {
            return Ok(());
        }

        let deps = get_dependency_status(&stage, &self.graph);

        // Check for existing handoff file to include in signal for continuation
        let handoff_file = find_latest_handoff(&stage.id, &self.config.work_dir)
            .ok()
            .flatten()
            .and_then(|p| {
                p.file_stem()
                    .and_then(|s| s.to_str().map(|s| s.to_string()))
            });

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
                        FailureType::InfrastructureError,
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
/// `session_lifecycle.rs`; this impl keeps what is specific to the spawn
/// sequence itself.
impl Orchestrator {
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
fn get_dependency_status(
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
