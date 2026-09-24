//! The spawn tail every worktree stage agent goes through, split out of
//! `stage_executor.rs` so the contract-phase handlers
//! (`event_handler/contract_phase.rs`) reuse it: a v2 standard stage with
//! contracts runs a `Contract` session, then its `Stage` session (DESIGN D8),
//! both spawned from here while the stage stays `Executing`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;

use crate::git;
use crate::models::failure::FailureType;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::models::worktree::Worktree;
use crate::orchestrator::merge_lifecycle::MergeLifecycle;
use crate::orchestrator::signals::{generate_contract_signal, generate_signal_with_skills};
use crate::verify::contracts::store::load_freeze;

use super::crash_classification::spawn_failure_type;
use super::persistence::Persistence;
use super::stage_executor::get_dependency_status;
use super::Orchestrator;

/// Which agent a stage spawn starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AgentKind {
    /// The `Stage` session that implements the stage.
    Implementation,
    /// The `Contract` session that writes and freezes a v2 stage's contract
    /// tests before anything implements them.
    Contract,
}

impl Orchestrator {
    /// The agent a fresh attempt at `stage` starts with: the contract writer
    /// while a v2 standard stage's contracts are not frozen yet, else the
    /// implementer.
    ///
    /// Returns `None` if the stage was blocked instead: a freeze record that
    /// cannot be read leaves no safe guess, since either answer may run the
    /// wrong agent.
    pub(super) fn first_agent_kind(&mut self, stage: &Stage) -> Option<AgentKind> {
        let has_contract_phase = stage.plan_version == 2
            && stage.stage_type == StageType::Standard
            && !stage.contracts.is_empty();
        if !has_contract_phase {
            return Some(AgentKind::Implementation);
        }
        match load_freeze(&self.config.work_dir, &stage.id) {
            Ok(None) => Some(AgentKind::Contract),
            Ok(Some(_)) => Some(AgentKind::Implementation),
            Err(e) => {
                self.block_stranded_stage(
                    &stage.id,
                    format!("Failed to read the contract freeze record: {e:#}"),
                );
                None
            }
        }
    }

    /// Spawn `kind`'s agent for `stage` into `worktree`.
    ///
    /// `stage` is either `Queued` (a fresh attempt, from `start_stage`) or
    /// already `Executing` (the contract phase handing over, from
    /// `event_handler/contract_phase.rs`); both leave it `Executing` with the
    /// new session assigned. `base` is the branch the worktree was cut from,
    /// recorded once the spawn succeeds; `None` keeps the recorded one.
    ///
    /// Every failure is contained: the stage is marked Blocked rather than the
    /// error propagating to the orchestrator loop and killing the daemon while
    /// the stage is Executing (O-11).
    pub(super) fn spawn_stage_agent(
        &mut self,
        stage: Stage,
        worktree: Worktree,
        base: Option<String>,
        kind: AgentKind,
    ) -> Result<()> {
        let stage_id = stage.id.clone();
        let handover = stage.status == StageStatus::Executing;
        let outgoing_session_id = stage.session.clone();
        let Some((session, recovery_signal)) = self.write_ahead_agent_session(&stage, kind) else {
            return Ok(());
        };
        let Some(stage) = self.mark_agent_executing(&stage_id, &session.id)? else {
            return Ok(());
        };
        if !self.validate_stage_sandbox(&stage, &stage_id, &session.id) {
            return Ok(());
        }
        refresh_stage_overlay(&stage_id, &self.config.repo_root, &self.config.work_dir);
        if !self.require_stage_hooks(&stage_id, &session.id) {
            return Ok(());
        }

        let signal_path = match recovery_signal {
            // Reuse the pre-written recovery signal.
            Some((_, recovery_path)) => recovery_path,
            None => {
                let outgoing = outgoing_session_id.as_deref();
                match self.write_agent_signal(&stage, &worktree, &session, outgoing, kind) {
                    Some(path) => path,
                    None => return Ok(()),
                }
            }
        };

        // Stale recovery signals from earlier attempts must not accumulate.
        self.cleanup_stale_recovery_signals(&stage_id, &session.id);

        let launched = self.launch_agent(&stage, &worktree, session, &signal_path, kind, handover);
        if let Some(spawned) = launched {
            self.record_spawned_agent(&stage, worktree, base, spawned);
        }
        Ok(())
    }

    /// Resolve the session `kind` runs as and write its record ahead of the
    /// stage being marked Executing (see `write_ahead_session`'s invariant
    /// doc). Only the implementation spawn honours a pending recovery signal
    /// (C-5): that signal briefs a stage session, not a contract writer.
    ///
    /// Returns `None` if the write-ahead failed; the stage has already been
    /// marked Blocked.
    fn write_ahead_agent_session(
        &mut self,
        stage: &Stage,
        kind: AgentKind,
    ) -> Option<(Session, Option<(String, PathBuf)>)> {
        match kind {
            AgentKind::Implementation => self.write_ahead_session(stage, &stage.id),
            AgentKind::Contract => self
                .write_ahead_contract_session(&stage.id)
                .map(|session| (session, None)),
        }
    }

    /// Mark the stage Executing, linked to the session record written ahead,
    /// in ONE locked update so "Executing" and "session assigned" can never
    /// be observed apart. A stage that is already Executing (the contract
    /// phase handing over) keeps its running attempt clock and graph node.
    ///
    /// Returns `Ok(None)` if the stage was marked Blocked instead.
    fn mark_agent_executing(&mut self, stage_id: &str, session_id: &str) -> Result<Option<Stage>> {
        let stage = match self.update_stage(stage_id, |current| {
            let continuing = current.status == StageStatus::Executing;
            current.try_mark_executing()?;
            if !continuing {
                current.begin_attempt(Utc::now());
            }
            current.assign_session(session_id.to_string());
            Ok(())
        }) {
            Ok(stage) => stage,
            Err(e) => {
                self.block_and_undo_session(
                    stage_id,
                    session_id,
                    FailureType::InfrastructureError,
                    format!("Failed to mark stage executing: {e:#}"),
                );
                return Ok(None);
            }
        };
        let graph_executing = self
            .graph
            .get_node(stage_id)
            .is_some_and(|node| node.status == StageStatus::Executing);
        if !graph_executing {
            self.graph
                .mark_executing(stage_id)
                .context("Failed to mark stage as executing in graph")?;
        }
        Ok(Some(stage))
    }

    /// Write `kind`'s signal for this spawn. Generating it can fail (e.g. an
    /// unwritable signals dir); contain it: mark Blocked rather than
    /// propagating and killing the daemon while the stage is Executing with
    /// no session yet (O-11).
    fn write_agent_signal(
        &mut self,
        stage: &Stage,
        worktree: &Worktree,
        session: &Session,
        outgoing: Option<&str>,
        kind: AgentKind,
    ) -> Option<PathBuf> {
        let generated = match kind {
            AgentKind::Contract => generate_contract_signal(
                session,
                stage,
                worktree,
                &self.config.work_dir,
                self.skill_index.as_ref(),
                &self.detected_languages,
            ),
            AgentKind::Implementation => {
                let deps = get_dependency_status(stage, &self.graph);
                let handoff_file =
                    self.continuation_handoff_or_block(&stage.id, outgoing, &session.id)?;
                generate_signal_with_skills(
                    session,
                    stage,
                    worktree,
                    &deps,
                    handoff_file.as_deref(),
                    None, // git_history will be extracted from worktree in future enhancement
                    &self.config.work_dir,
                    self.skill_index.as_ref(),
                    &self.detected_languages,
                )
            }
        };
        match generated {
            Ok(path) => Some(path),
            Err(e) => {
                self.block_and_undo_session(
                    &stage.id,
                    &session.id,
                    FailureType::InfrastructureError,
                    format!("Failed to generate signal file: {e:#}"),
                );
                None
            }
        }
    }

    /// Spawn the agent, or in manual mode print how to start it by hand.
    /// Returns `None` if the spawn failed and the stage was blocked instead.
    fn launch_agent(
        &mut self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
        kind: AgentKind,
        handover: bool,
    ) -> Option<Session> {
        let stage_id = stage.id.as_str();
        if self.config.manual_mode {
            print_manual_start(
                stage_id,
                worktree,
                &session,
                signal_path,
                &self.config.work_dir,
            );
            return Some(session);
        }
        let original_session_id = session.id.clone();
        let spawned = match kind {
            AgentKind::Implementation => {
                self.backend
                    .spawn_session(stage, worktree, session, signal_path)
            }
            AgentKind::Contract => {
                self.backend
                    .spawn_contract_session(stage, worktree, session, signal_path)
            }
        };
        let spawned = match spawned {
            Ok(spawned) => spawned,
            Err(spawn_err) => {
                self.block_failed_spawn(stage_id, &original_session_id, &spawn_err, handover);
                return None;
            }
        };
        match kind {
            AgentKind::Implementation => println!("  Started: {stage_id}"),
            AgentKind::Contract => println!("  Started (contract): {stage_id}"),
        }
        // The signal file was written for the write-ahead session id.
        debug_assert_eq!(
            original_session_id, spawned.id,
            "Session ID mismatch: signal file created with '{}' but saving session with '{}'",
            original_session_id, spawned.id
        );
        Some(spawned)
    }

    /// Mark the stage Blocked after its spawn failed, rather than propagating
    /// to the orchestrator loop and killing the daemon. Without this, a
    /// transient spawn error strands the stage in Executing on disk;
    /// subsequent `loom run` invocations poll forever because Executing
    /// stages are never re-spawned. A `handover` keeps its worktree and
    /// branch: they hold the contract phase's uncommitted work.
    fn block_failed_spawn(
        &mut self,
        stage_id: &str,
        session_id: &str,
        spawn_err: &anyhow::Error,
        handover: bool,
    ) {
        let err_msg = format!("Failed to spawn session for stage {stage_id}: {spawn_err:#}");
        if !handover {
            // Remove orphan resources so a retry can start clean.
            // Worktree — best-effort force-removal; ignore "not found" etc.
            let _ = git::remove_worktree(stage_id, &self.config.repo_root, true);
            // Branch — force-delete so the next retry can recreate
            // it from the correct base.
            let branch = git::branch_name_for_stage(stage_id);
            let _ = git::delete_branch(&branch, true, &self.config.repo_root);
        }
        let failure_type = spawn_failure_type(spawn_err);
        self.block_and_undo_session(stage_id, session_id, failure_type, err_msg);
    }

    /// Persist what the spawn produced and start tracking it.
    ///
    /// Persisting the update (pid, Running status) can fail even though a
    /// real agent is now running: the write-ahead record already exists and
    /// `stage.session` names it, so orphan recovery and `loom attach` can
    /// still find the session even if this particular update is lost.
    /// Contain the failure: mark Blocked + InfrastructureError so a retry can
    /// clean up, rather than propagating and killing the daemon (O-11).
    fn record_spawned_agent(
        &mut self,
        stage: &Stage,
        worktree: Worktree,
        base: Option<String>,
        spawned: Session,
    ) {
        let stage_id = stage.id.as_str();
        if let Err(e) = self.save_session(&spawned) {
            let err_msg = format!("Failed to save session for stage {stage_id}: {e:#}");
            self.block_stranded_stage(stage_id, err_msg);
            return;
        }

        super::stage_telemetry::record_context_telemetry(self, stage, &spawned.id);
        // Merge only executor-owned fields into the fresh record under lock,
        // so the slow spawn cannot clobber a concurrent CLI update (O-22).
        // Session assignment already happened before the spawn (write-ahead);
        // only the worktree/base fields the spawn just learned land here.
        let worktree_id = worktree.id.clone();
        if let Err(e) = self.update_stage(stage_id, |current| {
            current.set_worktree(Some(worktree_id));
            if let Some(base) = base {
                current.set_resolved_base(Some(base));
            }
            Ok(())
        }) {
            let err_msg = format!("Failed to save stage after spawn for {stage_id}: {e:#}");
            self.block_stranded_stage(stage_id, err_msg);
            return;
        }

        self.insert_active_session(stage_id, spawned);
        self.active_worktrees.insert(stage_id.to_string(), worktree);
    }
}

/// Refresh the stage's source-graph overlay BEFORE the signal is generated:
/// the Knowledge Brief embedded in the signal is built from the overlay, so a
/// stale overlay would brief the agent from the pre-stage tree. This mirrors
/// the reconcile `merge_handler.rs` already does before a merge.
///
/// `start_knowledge_stage` deliberately does not get this call: it runs in the
/// main repo with no worktree, so `reconcile_overlay` would early-return at
/// merge_lifecycle.rs:76-79 anyway - adding it there would just add a
/// pointless full walk of the main repo on every knowledge stage.
///
/// `reconcile_source_graph` is incremental (it reuses a cached entry whenever
/// `body_hash` matches, see refresh/source_graph.rs:212-245), so steady-state
/// cost is proportional to changed files; only the first call on a fresh
/// worktree pays a full walk.
fn refresh_stage_overlay(stage_id: &str, repo_root: &Path, work_dir: &Path) {
    MergeLifecycle::new(stage_id, repo_root, work_dir).reconcile_overlay();
}

/// Manual mode: print how to start the agent by hand. Identity env vars are
/// normally exported by the wrapper script; here the user must provide them
/// so hooks and `loom memory` attribute work to the right stage/session.
fn print_manual_start(
    stage_id: &str,
    worktree: &Worktree,
    session: &Session,
    signal_path: &Path,
    work_dir: &Path,
) {
    println!("Manual mode: Session setup for stage '{stage_id}'");
    println!("  Worktree: {}", worktree.path.display());
    println!("  Signal: {}", signal_path.display());
    let absolute_work_dir = work_dir
        .canonicalize()
        .unwrap_or_else(|_| work_dir.to_path_buf());
    println!(
        "  To start: cd {} && LOOM_STAGE_ID={} LOOM_SESSION_ID={} LOOM_WORK_DIR={} claude \"Read the signal file at {} and execute the assigned stage work.\"",
        worktree.path.display(),
        stage_id,
        session.id,
        absolute_work_dir.display(),
        signal_path.display()
    );
}
