//! Stage execution logic - creating worktrees, spawning sessions

use anyhow::{Context, Result};
use chrono::Utc;

use crate::git;
use crate::git::worktree::setup_worktree_hooks;
use crate::git::BaseBranchError;
use crate::handoff::find_latest_handoff;
use crate::hooks::{find_hooks_dir, setup_hooks_for_worktree, HooksConfig};
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::signals::{
    generate_knowledge_signal, generate_signal_with_skills, DependencyStatus,
};

use super::persistence::Persistence;
use super::Orchestrator;

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
        let ready_stages = self.graph.ready_stages();
        let available_slots = self
            .config
            .max_parallel_sessions
            .saturating_sub(self.active_sessions.len());

        // Collect stage IDs first to avoid borrow checker issues
        let stage_ids: Vec<String> = ready_stages
            .iter()
            .take(available_slots)
            .map(|node| node.id.clone())
            .collect();

        let mut started = 0;
        for stage_id in stage_ids {
            self.start_stage(&stage_id)
                .with_context(|| format!("Failed to start stage: {stage_id}"))?;
            started += 1;
        }

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
            return Ok(());
        }

        // Defense-in-depth: refuse to spawn if dependencies aren't truly satisfied,
        // even if the graph thinks they are. This prevents phantom-merge propagation
        // where a dep's `merged` flag is set but its commit is not actually in the
        // target branch. See PLAN-fix-phantom-merge.md Fix 10.
        //
        // Per the plan, we do NOT transition the stage to Blocked here — the
        // dependent hasn't been attempted, and Queued -> Blocked is technically
        // valid but semantically wrong. Skip silently; the next poll cycle
        // re-evaluates once the upstream dependency stage is fixed.
        //
        // The check is memoized per stage (P-6): the underlying per-dep
        // `git merge-base --is-ancestor` work only re-runs when a dependency
        // stage file changes or the recheck interval elapses, so a stuck
        // dependent does not shell out to git every poll cycle.
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
                if self.spawn_skip_logged.insert(stage_id.to_string()) {
                    tracing::error!(
                        stage_id = %stage_id,
                        "Refusing to spawn: dependencies not truly satisfied (likely phantom merge in deps). Run `loom repair` to investigate."
                    );
                }
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
                return Ok(());
            }
        }

        // Transition through Queued if currently WaitingForDeps to reduce race window
        if stage.status == StageStatus::WaitingForDeps {
            stage.try_mark_queued()?;
            self.save_stage(&stage)?;
        }

        // Knowledge stages run in main repo without a worktree - mark executing immediately
        if stage.stage_type == StageType::Knowledge {
            stage.try_mark_executing()?;
            stage.begin_attempt(Utc::now());
            self.save_stage(&stage)?;
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
                if let Ok(mut reloaded) = self.load_stage(stage_id) {
                    if reloaded.try_mark_blocked().is_ok() {
                        reloaded.failure_info = Some(FailureInfo {
                            failure_type: FailureType::InfrastructureError,
                            detected_at: Utc::now(),
                            evidence: vec![err_msg],
                        });
                        let _ = self.save_stage(&reloaded);
                        let _ = self.graph.mark_status(stage_id, StageStatus::Blocked);
                    }
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
            Err(BaseBranchError::MergeConflict(msg)) => {
                // Mark stage as Blocked — a resolver is needed.
                // Stage is in Queued state here, can transition directly to Blocked.
                eprintln!("Stage '{stage_id}' blocked due to merge conflict: {msg}");
                if stage.try_mark_blocked().is_ok() {
                    self.save_stage(&stage)?;
                }
                return Ok(());
            }
            Err(BaseBranchError::SchedulingNotReady(msg)) => {
                // Transient — skip this cycle, retry on the next poll.
                eprintln!("Stage '{stage_id}' skipped due to scheduling error (will retry): {msg}");
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

                // Mark stage as blocked with failure info
                // Stage is in Queued state here, can transition directly to Blocked
                if stage.try_mark_blocked().is_ok() {
                    stage.failure_info = Some(FailureInfo {
                        failure_type: FailureType::InfrastructureError,
                        detected_at: Utc::now(),
                        evidence: vec![err_msg],
                    });
                    self.save_stage(&stage)?;
                }
                return Ok(());
            }
        };

        // Run before-stage checks if configured (verify pre-conditions in fresh worktree)
        if !stage.before_stage.is_empty() {
            let check_dir = match &stage.working_dir {
                Some(wd) if wd != "." && !wd.is_empty() => worktree.path.join(wd),
                _ => worktree.path.clone(),
            };

            println!("  Running before-stage checks for '{stage_id}'...");
            match crate::verify::before_after::run_before_stage_checks(
                &stage.before_stage,
                &check_dir,
            ) {
                Ok(gaps) if !gaps.is_empty() => {
                    for gap in &gaps {
                        eprintln!("  ✗ Before-stage: {}", gap.description);
                        eprintln!("    → {}", gap.suggestion);
                    }
                    eprintln!("Before-stage verification failed for '{stage_id}' - pre-conditions not met");

                    if stage.try_mark_blocked().is_ok() {
                        stage.failure_info = Some(FailureInfo {
                            failure_type: FailureType::TestFailure,
                            detected_at: Utc::now(),
                            evidence: gaps.iter().map(|g| g.description.clone()).collect(),
                        });
                        self.save_stage(&stage)?;
                    }
                    return Ok(());
                }
                Ok(_) => {
                    println!("  ✓ Before-stage checks passed for '{stage_id}'");
                }
                Err(e) => {
                    eprintln!("Warning: Before-stage checks errored for '{stage_id}': {e}");
                    // Continue anyway - before-stage is advisory, don't block on errors
                }
            }
        }

        // Worktree created successfully - NOW mark as Executing
        // This ensures we only reach Executing state after infrastructure is ready
        stage.try_mark_executing()?;
        stage.begin_attempt(Utc::now());
        self.save_stage(&stage)?;
        self.graph
            .mark_executing(stage_id)
            .context("Failed to mark stage as executing in graph")?;

        // Generate and write sandbox settings to worktree
        let mut merged_sandbox = crate::sandbox::merge_config(
            &self.config.sandbox_config,
            &stage.sandbox,
            stage.stage_type,
        );
        // Defense-in-depth: re-validate at spawn time. `loom init` already
        // rejects incompatible configs; refuse to spawn rather than silently
        // downgrade if the on-disk config has since become invalid.
        if let Err(e) = crate::sandbox::validate_config(&merged_sandbox) {
            let err_msg = format!("{e:#}");
            eprintln!("Stage '{stage_id}' blocked: invalid sandbox config at spawn: {err_msg}");
            if stage.try_mark_blocked().is_ok() {
                stage.failure_info = Some(FailureInfo {
                    failure_type: FailureType::InfrastructureError,
                    detected_at: Utc::now(),
                    evidence: vec![err_msg],
                });
                self.save_stage(&stage)?;
            }
            return Ok(());
        }
        crate::sandbox::expand_paths(&mut merged_sandbox);
        if let Err(e) = crate::sandbox::write_settings(&merged_sandbox, &worktree.path) {
            eprintln!("Warning: Failed to write sandbox settings for stage '{stage_id}': {e}");
            // Continue anyway - sandbox is optional enhancement
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

        // Set up Claude Code hooks for this session. A failure here must not
        // strand the stage as Executing+session:None (O-11): the daemon would
        // exit (via per-tick error propagation) and orphan recovery, which
        // iterates session files only, would never see the stage. Hooks are an
        // optional enhancement, so we only warn — but the same containment
        // applies to the fatal `?`-propagating steps below.
        if let Some(hooks_dir) = find_hooks_dir() {
            if let Err(e) = setup_worktree_hooks(
                &worktree.path,
                &self.config.work_dir,
                &hooks_dir,
                merged_sandbox.permission_mode,
            ) {
                eprintln!("Warning: Failed to set up hooks for stage '{stage_id}': {e}");
                // Continue anyway - hooks are optional enhancement
            }
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
                .native
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
                    if let Ok(mut reloaded) = self.load_stage(stage_id) {
                        if reloaded.try_mark_blocked().is_ok() {
                            reloaded.failure_info = Some(FailureInfo {
                                failure_type: FailureType::InfrastructureError,
                                detected_at: Utc::now(),
                                evidence: vec![err_msg],
                            });
                            let _ = self.save_stage(&reloaded);
                            let _ = self.graph.mark_status(stage_id, StageStatus::Blocked);
                        }
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

        // Update the stage with session/worktree/resolved_base. Reload from disk
        // first and merge only these executor-owned fields, so a concurrent CLI
        // update (e.g. `loom stage` editing the same file during the slow spawn)
        // is not clobbered by a stale in-memory copy (O-22). The spawn-error
        // path already reloads; this mirrors it on the success path.
        let mut updated_stage = self.load_stage(stage_id).unwrap_or(stage);
        updated_stage.assign_session(spawned_session.id.clone());
        updated_stage.set_worktree(Some(worktree.id.clone()));
        updated_stage.set_resolved_base(Some(resolved.branch_name().to_string()));
        if let Err(e) = self.save_stage(&updated_stage) {
            let err_msg = format!("Failed to save stage after spawn for {stage_id}: {e:#}");
            self.block_stranded_stage(stage_id, err_msg);
            return Ok(());
        }

        self.active_sessions
            .insert(stage_id.to_string(), spawned_session);
        self.active_worktrees.insert(stage_id.to_string(), worktree);

        Ok(())
    }

    fn start_knowledge_stage(&mut self, mut stage: Stage) -> Result<()> {
        let stage_id = stage.id.clone();

        // Generate and write sandbox settings to main repo
        let mut merged_sandbox = crate::sandbox::merge_config(
            &self.config.sandbox_config,
            &stage.sandbox,
            stage.stage_type,
        );
        // Defense-in-depth: re-validate at spawn time even for knowledge stages.
        if let Err(e) = crate::sandbox::validate_config(&merged_sandbox) {
            let err_msg = format!("{e:#}");
            eprintln!(
                "Knowledge stage '{stage_id}' blocked: invalid sandbox config at spawn: {err_msg}"
            );
            if stage.try_mark_blocked().is_ok() {
                stage.failure_info = Some(FailureInfo {
                    failure_type: FailureType::InfrastructureError,
                    detected_at: Utc::now(),
                    evidence: vec![err_msg],
                });
                self.save_stage(&stage)?;
            }
            return Ok(());
        }
        crate::sandbox::expand_paths(&mut merged_sandbox);
        // Knowledge stages share the host's main-repo `.claude/settings.local.json`
        // (the agent runs on the host directly), so the sandbox/permissions settings
        // must be written there.
        if let Err(e) = crate::sandbox::write_settings(&merged_sandbox, &self.config.repo_root) {
            eprintln!(
                "Warning: Failed to write sandbox settings for knowledge stage '{stage_id}': {e}"
            );
            // Continue anyway - sandbox is optional enhancement
        }

        let session = Session::new();

        // Set up Claude Code hooks for this session by writing into the main
        // repo's `.claude/settings.local.json` (the host's agent reads this
        // file directly). Session identity is deliberately NOT written: this
        // file is shared by every main-repo session (later knowledge stages,
        // interactive user sessions), so persisted stage/session IDs would go
        // stale and shadow the wrapper script's fresh exports.
        if let Some(hooks_dir) = find_hooks_dir() {
            // Canonicalize work_dir to absolute path
            let absolute_work_dir = self
                .config
                .work_dir
                .canonicalize()
                .unwrap_or_else(|_| self.config.work_dir.clone());

            let config =
                HooksConfig::new(hooks_dir, absolute_work_dir, merged_sandbox.permission_mode);

            if let Err(e) = setup_hooks_for_worktree(&self.config.repo_root, &config) {
                eprintln!("Warning: Failed to set up hooks for knowledge stage '{stage_id}': {e}");
                // Continue anyway - hooks are optional enhancement
            }
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
                .native
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

        // Update stage with session info (already marked Executing earlier)
        let mut updated_stage = stage;
        updated_stage.assign_session(spawned_session.id.clone());
        // Knowledge stages don't have a worktree
        updated_stage.set_worktree(None);
        updated_stage.set_resolved_base(None);
        self.save_stage(&updated_stage)?;

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
        if let Ok(mut reloaded) = self.load_stage(stage_id) {
            if reloaded.try_mark_blocked().is_ok() {
                reloaded.failure_info = Some(FailureInfo {
                    failure_type: FailureType::InfrastructureError,
                    detected_at: Utc::now(),
                    evidence: vec![err_msg],
                });
                let _ = self.save_stage(&reloaded);
                let _ = self.graph.mark_status(stage_id, StageStatus::Blocked);
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
