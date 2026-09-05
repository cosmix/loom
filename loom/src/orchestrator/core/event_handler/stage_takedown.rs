//! Taking every agent off a stage, and proving none survived.
//!
//! Split out of `event_handler` to keep that file inside the size limit. The
//! handoff paths call in here; nothing else does.

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::fs::session_files::{load_session_exact, mark_session_context_exhausted};
use crate::handoff::HandoffOrigin;
use crate::models::session::{Session, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::session_registry::in_progress_sessions_for_stage;
use crate::orchestrator::signals::remove_signal;
use crate::orchestrator::terminal::native::{session_process_status, SessionProcessStatus};

use super::super::{persistence::Persistence, Orchestrator};

/// How long a takedown waits for a signalled agent to actually exit before
/// calling it a survivor. See [`Orchestrator::confirm_session_gone`], which is
/// also the poll `close_adjudication_session` (`judge_close.rs`) uses to confirm
/// a killed judge before its record is written.
const KILL_CONFIRM_TIMEOUT: Duration = Duration::from_secs(2);
const KILL_CONFIRM_POLL_INTERVAL: Duration = Duration::from_millis(50);

fn retain_missing_identity(missing: bool, session: &Session, survivors: &mut Vec<String>) -> bool {
    if !missing {
        return false;
    }
    eprintln!(
        "Warning: Session '{}' had no PID identity evidence before takedown; refusing to treat \
         absence as confirmed death",
        session.id
    );
    survivors.push(session.id.clone());
    true
}

fn include_assigned_session(
    agents: &mut Vec<Session>,
    stage_id: &str,
    assigned_id: &str,
    work_dir: &Path,
) -> Result<()> {
    if let Some(assigned) = agents.iter().find(|session| session.id == assigned_id) {
        anyhow::ensure!(
            assigned.stage_id.as_deref() == Some(stage_id),
            "assigned session '{}' belongs to stage {:?}, not '{}'",
            assigned.id,
            assigned.stage_id,
            stage_id
        );
        return Ok(());
    }

    let Some(assigned) = load_session_exact(work_dir, assigned_id)? else {
        bail!(
            "stage '{stage_id}' assigns session '{assigned_id}', but no exact session record \
             exists; refusing to re-queue without durable proof that its writer is gone"
        );
    };
    anyhow::ensure!(
        assigned.stage_id.as_deref() == Some(stage_id),
        "assigned session '{}' belongs to stage {:?}, not '{}'",
        assigned.id,
        assigned.stage_id,
        stage_id
    );
    // Terminal is workflow state, not process-death evidence. Completion is
    // persisted before the agent finishes its merge/teardown path, so a
    // restarted daemon must still probe and take down the exact record.
    agents.push(assigned);
    Ok(())
}

impl Orchestrator {
    /// Every agent the daemon can find for `stage_id`: the one it tracks in
    /// memory, plus any `Running`/`Spawning` session RECORD on disk it does
    /// not.
    ///
    /// `active_sessions` is in-memory only and is NOT rebuilt when the daemon
    /// restarts (see `Orchestrator::new`), so a missing map entry is no evidence
    /// that nothing is running — after a restart the recovered stage's original
    /// agent is still there with no entry to its name. The records on disk
    /// outlive the daemon. Unlike the executor's liveness-filtered scan, this
    /// takes every in-progress record: a dead record is still an unresolved
    /// deliberate handoff until this takedown confirms it gone and persists
    /// `ContextExhausted`.
    ///
    /// The stage's assigned session is the final authority. If neither source
    /// above contains it, its exact record must still be probed even when its
    /// workflow status is terminal: `Completed` is persisted before the agent
    /// necessarily exits. No record at all is uncertainty, not proof of
    /// absence, and therefore fails closed instead of re-queueing a possible
    /// second writer.
    fn stage_agents(&self, stage_id: &str, expected_session_id: &str) -> Result<Vec<Session>> {
        let mut agents: Vec<Session> = self
            .active_sessions
            .get(stage_id)
            .cloned()
            .into_iter()
            .collect();
        let persisted = in_progress_sessions_for_stage(&self.config.work_dir, stage_id)
            .with_context(|| format!("discovering every session attached to stage '{stage_id}'"))?;
        let tracked: HashSet<String> = agents.iter().map(|s| s.id.clone()).collect();
        agents.extend(
            persisted
                .into_iter()
                .filter(|session| !tracked.contains(&session.id)),
        );

        let stage = self.load_stage(stage_id)?;
        anyhow::ensure!(
            stage.session.as_deref() == Some(expected_session_id),
            "stage '{stage_id}' moved from handoff session '{expected_session_id}' to {:?}; \
             refusing to take down the newer assignment",
            stage.session
        );
        anyhow::ensure!(
            stage.status == StageStatus::NeedsHandoff,
            "stage '{stage_id}' moved out of NeedsHandoff to {:?}; refusing destructive takedown",
            stage.status
        );
        include_assigned_session(
            &mut agents,
            stage_id,
            expected_session_id,
            &self.config.work_dir,
        )?;
        Ok(agents)
    }

    /// Persist a session the takedown confirmed gone as `ContextExhausted`.
    ///
    /// Without this the record stays `Running` with a dead PID, and the next
    /// poll reads the vanished process as a CRASH: `exited_after_stage_finished`
    /// forgives only `Completed`/`MergeConflict`/`MergeBlocked`, so a routine
    /// ceiling handoff files a crash report, charges the stage's retry budget
    /// and can block the stage outright when the respawn is declined. It is
    /// also what the comment below already assumes when it calls such a record
    /// no longer live.
    ///
    /// The status is DECLARED, not transitioned. The takedown also kills agents
    /// that never reached `Running`, and `Spawning -> ContextExhausted` is not a
    /// legal transition (`models/session/transitions.rs`), so routing this
    /// through `try_mark_context_exhausted` would refuse exactly the record this
    /// exists to remove and leave it non-terminal. `Handlers::persist_session_status`
    /// states a status the same way, for the same reason.
    ///
    /// `Crashed` — the only other terminal status a `Spawning` record may
    /// legally take — is deliberately NOT used: Detection writes a crash report
    /// and emits `SessionCrashed` for any observed transition INTO `Crashed`
    /// (`monitor/session_events.rs`), and since the stage still names this
    /// session that event blocks the stage and charges its retry budget. That is
    /// the very failure this function exists to prevent. `ContextExhausted` is
    /// terminal AND silent, which is what a deliberate takedown wants; it is
    /// read as "the governor took this agent off the stage", not as a claim
    /// about how much context the agent had actually used.
    ///
    /// A session already in a terminal state keeps the status it earned. The
    /// canonical helper re-reads under the session-directory lock, so this
    /// stale liveness snapshot cannot overwrite a newer heartbeat or terminal
    /// state. A write failure is fatal to this handoff: re-queueing with
    /// `Running` still on disk would turn the deliberate exit into a crash on
    /// the next poll.
    fn record_context_exhausted(&self, session: &Session) -> Result<()> {
        mark_session_context_exhausted(&self.config.work_dir, &session.id).with_context(|| {
            format!(
                "persisting session '{}' as ContextExhausted before re-queue",
                session.id
            )
        })
    }

    /// Kill every agent attached to `stage_id`; return the ids of any that are
    /// still alive afterwards.
    ///
    /// An empty return is the only proof that nothing writes the worktree any
    /// more, which is what re-queueing needs.
    pub(super) fn take_down_stage_agents(
        &mut self,
        stage_id: &str,
        expected_session_id: &str,
    ) -> Result<Vec<String>> {
        let agents = self.stage_agents(stage_id, expected_session_id)?;
        self.take_down_agents(stage_id, agents)
    }

    /// Kill every session in `agents`, all understood to belong to
    /// `stage_id`; return the ids of any that are still alive afterwards.
    ///
    /// Split out of `take_down_stage_agents` so a caller that has already
    /// assembled its own agent list (see `retire_disputing_agents`, which
    /// excludes the stage's adjudication session) can drive the same kill
    /// loop without going through the `NeedsHandoff`-only lookup in
    /// `stage_agents`.
    pub(super) fn take_down_agents(
        &mut self,
        stage_id: &str,
        agents: Vec<Session>,
    ) -> Result<Vec<String>> {
        let mut survivors = Vec::new();
        for session in &agents {
            // Capture whether identity evidence existed BEFORE teardown. A
            // backend may remove its PID file after a successful kill, but a
            // record that started with no evidence cannot turn a false
            // liveness answer into proof of death.
            let identity_was_missing = session_process_status(&self.config.work_dir, session)
                == SessionProcessStatus::Missing;
            if let Err(e) = self.backend.kill_session(session) {
                eprintln!("Warning: Failed to kill session '{}': {e}", session.id);
            }
            if retain_missing_identity(identity_was_missing, session, &mut survivors) {
                continue;
            }
            // The liveness probe decides, not the kill's return value: a kill
            // that reported an error may still have taken the agent down, and
            // one that reported success may not have (`TmuxBackend::kill_session`
            // returns `Ok` unconditionally, and the native lane returns `Ok`
            // when it refuses to signal an unverifiable identity).
            if self.confirm_session_gone(session)? {
                self.record_context_exhausted(session)?;
                if let Err(e) = remove_signal(&session.id, &self.config.work_dir) {
                    eprintln!(
                        "Warning: Failed to remove signal for session '{}': {e}",
                        session.id
                    );
                }
            } else {
                survivors.push(session.id.clone());
            }
        }

        // Keep the daemon's handle on a session that outlived its kill: dropping
        // it would leave the next attempt with nothing to find, since the record
        // this path already marked `ContextExhausted` no longer counts as live.
        if survivors.is_empty() {
            self.active_sessions.remove(stage_id);
        }
        Ok(survivors)
    }

    /// Whether `session`'s process is gone, waiting a bounded moment for it.
    ///
    /// The teardown signals with SIGTERM and returns immediately
    /// (`process::terminate`), so an agent that is exiting exactly as asked
    /// still answers the liveness probe for a while. Deciding on the first
    /// probe would call every correctly-killed agent a survivor and leave every
    /// handed-off stage sitting in `NeedsHandoff` forever. The wait is short and
    /// runs once per handoff, which the poll loop can afford; an agent that
    /// outlasts it is genuinely not responding to the kill.
    ///
    /// A probe error is uncertainty, not proof of death. Propagating it keeps
    /// the stage in `NeedsHandoff`, where an operator can retry safely without
    /// ever admitting a second writer to the worktree.
    pub(crate) fn confirm_session_gone(&self, session: &Session) -> Result<bool> {
        let deadline = Instant::now() + KILL_CONFIRM_TIMEOUT;
        loop {
            if !self.backend.is_session_alive(session).with_context(|| {
                format!(
                    "confirming whether session '{}' survived takedown",
                    session.id
                )
            })? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::sleep(KILL_CONFIRM_POLL_INTERVAL);
        }
    }

    /// Retire every agent working `stage_id` before a verdict is applied to
    /// it.
    ///
    /// The agent that filed the dispute ended its turn by filing it: it is
    /// idle, has never read the amended criteria, and if left alive the
    /// executor adopts it instead of spawning a successor. Its handoff is
    /// written, it is killed, and `stage.session` is cleared once every
    /// agent is confirmed gone. The adjudication session judging the stage
    /// shares its `stage_id` and is never touched. Returns the ids of
    /// agents that survived the kill; the caller must not apply the verdict
    /// while any remain.
    pub(crate) fn retire_disputing_agents(&mut self, stage_id: &str) -> Result<Vec<String>> {
        let stage = self.load_stage(stage_id)?;
        if stage.status != StageStatus::NeedsAdjudication {
            return Ok(Vec::new());
        }
        let agents = self.disputing_agents(stage_id, &stage)?;
        for agent in &agents {
            match self.monitor.handlers().ensure_context_handoff(
                agent,
                &stage,
                HandoffOrigin::Retired,
            ) {
                Ok(Some(path)) => {
                    eprintln!("Generated retirement handoff at: {}", path.display())
                }
                Ok(None) => {}
                Err(error) => tracing::warn!(
                    target: "loom::adjudication",
                    stage = %stage_id,
                    session = %agent.id,
                    %error,
                    "could not write a retirement handoff for a disputing agent",
                ),
            }
        }

        let survivors = self.take_down_agents(stage_id, agents)?;
        if survivors.is_empty() {
            self.update_stage(stage_id, |s| {
                if s.status == StageStatus::NeedsAdjudication {
                    s.release_session();
                }
                Ok(())
            })?;
        }
        Ok(survivors)
    }

    /// Every agent to retire for a disputing stage: the in-memory tracked
    /// session (if any), every persisted `Running`/`Spawning` record for the
    /// stage not already present, and — if it names something not yet in the
    /// list — the record `stage.session` points at. Filters out the
    /// adjudication session judging this stage, which shares `stage_id` but
    /// must never be killed by this path.
    fn disputing_agents(&self, stage_id: &str, stage: &Stage) -> Result<Vec<Session>> {
        let mut agents: Vec<Session> = self
            .active_sessions
            .get(stage_id)
            .cloned()
            .into_iter()
            .collect();
        let persisted = in_progress_sessions_for_stage(&self.config.work_dir, stage_id)
            .with_context(|| format!("discovering every session attached to stage '{stage_id}'"))?;
        let tracked: HashSet<String> = agents.iter().map(|s| s.id.clone()).collect();
        agents.extend(
            persisted
                .into_iter()
                .filter(|session| !tracked.contains(&session.id)),
        );
        if let Some(assigned_id) = stage.session.as_deref() {
            if !agents.iter().any(|s| s.id == assigned_id) {
                match load_session_exact(&self.config.work_dir, assigned_id)? {
                    Some(assigned) if assigned.stage_id.as_deref() == Some(stage_id) => {
                        agents.push(assigned);
                    }
                    Some(_) => {}
                    None => tracing::warn!(
                        target: "loom::adjudication",
                        stage = %stage_id,
                        session = %assigned_id,
                        "stage names a session with no record; continuing without it",
                    ),
                }
            }
        }
        Ok(agents
            .into_iter()
            .filter(|s| s.session_type != SessionType::Adjudication)
            .collect())
    }
}
