//! Stage execution logic - creating worktrees, spawning sessions

use anyhow::{Context, Result};
use chrono::Utc;

use crate::git;
use crate::git::worktree::setup_worktree_hooks;
use crate::git::BaseBranchError;
use crate::handoff::find_latest_handoff;
use crate::hooks::find_hooks_dir;
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::scheduling_report::{self, BlockReason, BlockedStage, SchedulingReport};
use crate::orchestrator::signals::{
    generate_knowledge_signal, generate_signal_with_skills, DependencyStatus,
};

use super::persistence::Persistence;
use super::Orchestrator;

impl Orchestrator {
    fn persist_blocked_stage(
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

        // Knowledge stages run in main repo without a worktree - mark executing immediately
        if stage.stage_type == StageType::Knowledge {
            stage = self.update_stage(stage_id, |current| {
                current.try_mark_executing()?;
                current.begin_attempt(Utc::now());
                Ok(())
            })?;
            self.graph
                .mark_executing(stage_id)
                .context("Failed to mark stage as executing in graph")?;
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

        // Resolve the base branch for worktree creation.
        //
        // Route on the *typed* `BaseBranchError` variant rather than matching
        // substrings of the error text (A-13): rewording a message can no
        // longer silently reclassify a handled condition into a propagated
        // error that exits the orchestrator loop.
        let resolved = match git::resolve_base_branch(
            stage_id,
            &stage.dependencies,
            &self.graph,
            &self.config.repo_root,
            self.config.base_branch.as_deref(),
        ) {
            Ok(resolved) => resolved,
            Err(BaseBranchError::SchedulingNotReady(msg)) => {
                // Transient — skip this cycle, retry on the next poll.
                //
                // Logged once per stage per daemon run: this fires on every
                // 5-second poll for as long as the condition holds, and an
                // unbounded print here buries the log (and the operator) under
                // thousands of identical lines while the stage sits Queued.
                if self.spawn_skip_logged.insert(stage_id.to_string()) {
                    tracing::warn!(
                        stage_id = %stage_id,
                        reason = %msg,
                        "Stage skipped due to scheduling error; will retry each poll"
                    );
                }
                self.spawn_blocks.insert(
                    stage_id.to_string(),
                    BlockReason::SchedulingNotReady { detail: msg },
                );
                return Ok(());
            }
            Err(BaseBranchError::Other(e)) => {
                return Err(e).with_context(|| {
                    format!("Failed to resolve base branch for stage: {stage_id}")
                });
            }
        };

        let worktree = match git::get_or_create_worktree(
            stage_id,
            &self.config.repo_root,
            Some(resolved.branch_name()),
        ) {
            Ok(wt) => wt,
            Err(e) => {
                let err_msg = format!("{e:#}");
                eprintln!("Stage '{stage_id}' blocked due to worktree error: {err_msg}");

                // Stage is Queued here and may transition directly to Blocked.
                let _ = self.persist_blocked_stage(
                    stage_id,
                    FailureType::InfrastructureError,
                    vec![err_msg],
                );
                return Ok(());
            }
        };

        // Run before-stage checks if configured (verify pre-conditions in a
        // pristine worktree). Blocks the stage when they fail.
        if !self.before_stage_gate_passed(&stage, &worktree.path, resolved.branch_name())? {
            return Ok(());
        }

        // Worktree created successfully - NOW mark as Executing
        // This ensures we only reach Executing state after infrastructure is ready
        stage = self.update_stage(stage_id, |current| {
            current.try_mark_executing()?;
            current.begin_attempt(Utc::now());
            Ok(())
        })?;
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
            let err_msg = format!("{e:#}");
            eprintln!("Stage '{stage_id}' blocked: invalid sandbox config at spawn: {err_msg}");
            let _ = self.persist_blocked_stage(
                stage_id,
                FailureType::InfrastructureError,
                vec![err_msg],
            );
            return Ok(());
        }
        crate::sandbox::expand_paths(&mut merged_sandbox);
        if let Err(error) =
            write_required_sandbox_settings(&merged_sandbox, &worktree.path, stage_id)
        {
            self.block_stranded_stage(stage_id, format!("{error:#}"));
            return Ok(());
        }

        // Honor a pending recovery signal (C-5). `loom stage retry --context`
        // (and crash/hung auto-recovery) writes a `recovery-<...>` signal file
        // keyed to a new session ID and stores that ID in `stage.session`. If
        // such a signal exists, reuse its session ID and signal path so the new
        // agent actually receives the recovery context, instead of overwriting
        // it with a freshly generated signal. The tracking key is derived from
        // the stage ID (not the session ID), so kill/liveness still work.
        let recovery_signal = self.pending_recovery_signal(&stage);
        let mut session = Session::new();
        if let Some((recovery_session_id, _)) = &recovery_signal {
            session.id = recovery_session_id.clone();
        }

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
            let err_msg = format!("Stage '{stage_id}' blocked: {e:#}");
            eprintln!("{err_msg}");
            let _ = self.persist_blocked_stage(
                stage_id,
                FailureType::SandboxSetupFailure,
                vec![err_msg],
            );
            let _ = self.graph.mark_status(stage_id, StageStatus::Blocked);
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
                    let err_msg = format!("Failed to generate signal file: {e:#}");
                    self.block_stranded_stage(stage_id, err_msg);
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
                    eprintln!("{err_msg}");
                    // Remove orphan resources so a retry can start clean.
                    // Worktree — best-effort force-removal; ignore "not found" etc.
                    let _ = git::remove_worktree(stage_id, &self.config.repo_root, true);
                    // Branch — force-delete so the next retry can recreate
                    // it from the correct base.
                    let branch = git::branch_name_for_stage(stage_id);
                    let _ = git::delete_branch(&branch, true, &self.config.repo_root);
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

        // Persisting the session can fail. At this point a real session may be
        // running, but the stage on disk is still Executing+session:None, which
        // orphan recovery cannot see (it scans session files). Contain the
        // failure: mark Blocked + InfrastructureError so a retry can clean up,
        // rather than propagating and killing the daemon (O-11).
        if let Err(e) = self.save_session(&spawned_session) {
            let err_msg = format!("Failed to save session for stage {stage_id}: {e:#}");
            self.block_stranded_stage(stage_id, err_msg);
            return Ok(());
        }

        // Merge only executor-owned fields into the fresh record under lock, so
        // the slow spawn cannot clobber a concurrent CLI update (O-22).
        let session_id = spawned_session.id.clone();
        let worktree_id = worktree.id.clone();
        let resolved_base = resolved.branch_name().to_string();
        if let Err(e) = self.update_stage(stage_id, |current| {
            current.assign_session(session_id);
            current.set_worktree(Some(worktree_id));
            current.set_resolved_base(Some(resolved_base));
            Ok(())
        }) {
            let err_msg = format!("Failed to save stage after spawn for {stage_id}: {e:#}");
            self.block_stranded_stage(stage_id, err_msg);
            return Ok(());
        }

        self.active_sessions
            .insert(stage_id.to_string(), spawned_session);
        self.active_worktrees.insert(stage_id.to_string(), worktree);

        Ok(())
    }

    fn start_knowledge_stage(&mut self, stage: Stage) -> Result<()> {
        let stage_id = stage.id.clone();

        // Generate and write sandbox settings to main repo
        let mut merged_sandbox = crate::sandbox::merge_config(
            &self.config.sandbox_config,
            &stage.sandbox,
            stage.stage_type,
            &stage.implementers,
        );
        // Defense-in-depth: re-validate at spawn time even for knowledge stages.
        if let Err(e) = crate::sandbox::validate_config(&merged_sandbox) {
            let err_msg = format!("{e:#}");
            eprintln!(
                "Knowledge stage '{stage_id}' blocked: invalid sandbox config at spawn: {err_msg}"
            );
            let _ = self.persist_blocked_stage(
                &stage_id,
                FailureType::InfrastructureError,
                vec![err_msg],
            );
            return Ok(());
        }
        crate::sandbox::expand_paths(&mut merged_sandbox);
        // Knowledge stages share the host's main-repo `.claude/settings.local.json`
        // (the agent runs on the host directly), so the sandbox/permissions settings
        // must be written there.
        write_required_sandbox_settings(&merged_sandbox, &self.config.repo_root, &stage_id)?;

        let session = Session::new();

        // Set up Claude Code hooks for this session by writing into the main
        // repo's `.claude/settings.local.json` (the host's agent reads this
        // file directly). Session identity is deliberately NOT written: this
        // file is shared by every main-repo session (later knowledge stages,
        // interactive user sessions), so persisted stage/session IDs would go
        // stale and shadow the wrapper script's fresh exports.
        // Claude Code hooks are the knowledge stage's security boundary, not
        // an optional enhancement: a session is never spawned without them.
        // Contain a setup failure as Blocked rather than propagating it, which
        // would kill the daemon while the stage sits Executing with no session.
        // Knowledge stages have no worktree of their own: the main repo root is
        // the install target that receives `.claude/settings.json`.
        if let Err(e) = install_required_hooks(
            find_hooks_dir(),
            &self.config.repo_root,
            &self.config.work_dir,
            merged_sandbox.permission_mode,
            &stage_id,
        ) {
            let err_msg = format!("Knowledge stage '{stage_id}' blocked: {e:#}");
            eprintln!("{err_msg}");
            let _ = self.persist_blocked_stage(
                &stage_id,
                FailureType::SandboxSetupFailure,
                vec![err_msg],
            );
            let _ = self.graph.mark_status(&stage_id, StageStatus::Blocked);
            return Ok(());
        }

        // Exclude .claude/settings.local.json from the main repo's gitignore so knowledge-stage
        // hook configs cannot be accidentally committed.
        if let Err(e) =
            crate::git::worktree::add_settings_local_to_main_gitignore(&self.config.repo_root)
        {
            eprintln!("Warning: Failed to add settings.local.json to main repo gitignore: {e}");
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
            let spawned = self
                .backend
                .spawn_knowledge_session(&stage, session, &signal_path, &self.config.repo_root)
                .with_context(|| {
                    format!("Failed to spawn knowledge session for stage: {stage_id}")
                })?;

            // Print confirmation that stage was started
            println!("  Started (knowledge): {stage_id}");

            spawned
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

        // Knowledge stages don't have a worktree; update only executor-owned
        // runtime fields on the fresh record.
        let session_id = spawned_session.id.clone();
        self.update_stage(&stage_id, |current| {
            current.assign_session(session_id);
            current.set_worktree(None);
            current.set_resolved_base(None);
            Ok(())
        })?;

        // Add to active sessions but NOT to active_worktrees (no worktree for knowledge stages)
        self.active_sessions.insert(stage_id, spawned_session);

        Ok(())
    }
}

/// Helpers shared by the worktree spawn path (recovery-signal delivery and
/// infrastructure-failure containment).
impl Orchestrator {
    /// Mark a stage Blocked with an `InfrastructureError` after a failure that
    /// occurred *after* it was already marked Executing but *before* a session
    /// was successfully recorded (O-11).
    ///
    /// Such a stage would otherwise be stranded as `Executing, session: None`:
    /// the daemon would exit on the propagated error and orphan recovery, which
    /// iterates session *files*, would never route it back to a runnable state.
    /// We reload from disk (the in-memory copy may be stale) and best-effort
    /// transition + persist; failures here are logged, not propagated.
    fn block_stranded_stage(&mut self, stage_id: &str, err_msg: String) {
        eprintln!("Stage '{stage_id}' blocked due to spawn-setup failure: {err_msg}");
        match self.persist_blocked_stage(stage_id, FailureType::InfrastructureError, vec![err_msg])
        {
            Ok(()) => {
                let _ = self.graph.mark_status(stage_id, StageStatus::Blocked);
            }
            Err(error) => {
                eprintln!("Failed to persist Blocked state for '{stage_id}': {error:#}");
            }
        }
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

    /// If the stage's recorded session points at an existing `recovery-*` signal
    /// file, return `(recovery_session_id, signal_path)` so the spawn path can
    /// reuse it and deliver the recovery context (C-5).
    fn pending_recovery_signal(&self, stage: &Stage) -> Option<(String, std::path::PathBuf)> {
        let session_id = stage.session.as_ref()?;
        if !session_id.starts_with("recovery-") {
            return None;
        }
        let signal_path = self
            .config
            .work_dir
            .join("signals")
            .join(format!("{session_id}.md"));
        if signal_path.exists() {
            Some((session_id.clone(), signal_path))
        } else {
            None
        }
    }

    /// Remove `recovery-<stage_id>-*` signal files that do not belong to the
    /// session about to spawn, so stale recovery signals from prior attempts do
    /// not accumulate in `.work/signals/` (C-5).
    ///
    /// Recovery session IDs are `recovery-<stage_id>-<8hex>-<timestamp>`. We
    /// match the trailing `<8hex>-<timestamp>` shape exactly so a sibling stage
    /// whose ID shares this stage's prefix (e.g. `auth` vs `auth-tests`) is not
    /// caught by a naive `starts_with` — the prefix-collision class behind O-5.
    fn cleanup_stale_recovery_signals(&self, stage_id: &str, keep_session_id: &str) {
        let signals_dir = self.config.work_dir.join("signals");
        let prefix = format!("recovery-{stage_id}-");
        let keep_file = format!("{keep_session_id}.md");
        let Ok(entries) = std::fs::read_dir(&signals_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name == keep_file {
                continue;
            }
            let Some(stem) = name.strip_suffix(".md") else {
                continue;
            };
            let Some(suffix) = stem.strip_prefix(&prefix) else {
                continue;
            };
            // Suffix must be exactly `<8hex>-<digits>` for this stage — not a
            // sibling stage whose ID begins with `stage_id-`.
            if is_recovery_id_suffix(suffix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// Whether `suffix` is the `<8hex>-<timestamp>` tail of a recovery session ID.
///
/// Used to distinguish this stage's recovery signals from those of a sibling
/// stage whose ID merely begins with `<stage_id>-`.
fn is_recovery_id_suffix(suffix: &str) -> bool {
    let Some((hex, ts)) = suffix.split_once('-') else {
        return false;
    };
    hex.len() == 8
        && hex.chars().all(|c| c.is_ascii_hexdigit())
        && !ts.is_empty()
        && ts.chars().all(|c| c.is_ascii_digit())
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
