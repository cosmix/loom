//! Taking every agent off a stage, and proving none survived.
//!
//! Split out of `event_handler` to keep that file inside the size limit. The
//! handoff paths call in here; nothing else does.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::session_registry::live_sessions_for_stage;
use crate::orchestrator::signals::remove_signal;

use super::super::persistence::Persistence;
use super::super::Orchestrator;

/// How long a takedown waits for a signalled agent to actually exit before
/// calling it a survivor. See [`Orchestrator::confirm_session_gone`].
const KILL_CONFIRM_TIMEOUT: Duration = Duration::from_secs(2);
const KILL_CONFIRM_POLL_INTERVAL: Duration = Duration::from_millis(50);

impl Orchestrator {
    /// Every agent the daemon can find for `stage_id`: the one it tracks in
    /// memory, plus any live session RECORD on disk it does not.
    ///
    /// `active_sessions` is in-memory only and is NOT rebuilt when the daemon
    /// restarts (see `Orchestrator::new`), so a missing map entry is no evidence
    /// that nothing is running — after a restart the recovered stage's original
    /// agent is still there with no entry to its name. The records on disk
    /// outlive the daemon, so they are consulted through the same
    /// `live_sessions_for_stage` probe the executor uses before spawning.
    fn stage_agents(&self, stage_id: &str) -> Vec<Session> {
        let mut agents: Vec<Session> = self
            .active_sessions
            .get(stage_id)
            .cloned()
            .into_iter()
            .collect();
        match live_sessions_for_stage(&self.config.work_dir, stage_id) {
            Ok(live) => {
                let tracked: HashSet<String> = agents.iter().map(|s| s.id.clone()).collect();
                agents.extend(live.into_iter().filter(|s| !tracked.contains(&s.id)));
            }
            Err(e) => eprintln!(
                "Warning: Failed to list live sessions for stage '{stage_id}': {e}. \
                 Working from the daemon's in-memory session only."
            ),
        }
        agents
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
    /// Best-effort: a session already in a terminal state keeps the status it
    /// earned, and a record that cannot be written must not stop the takedown.
    fn record_context_exhausted(&self, session: &Session) {
        if session.status.is_terminal() {
            return;
        }
        let mut record = session.clone();
        record.status = SessionStatus::ContextExhausted;
        if let Err(e) = self.save_session(&record) {
            eprintln!(
                "Warning: Failed to save session '{}' as ContextExhausted: {e}",
                session.id
            );
        }
    }

    /// Kill every agent attached to `stage_id`; return the ids of any that are
    /// still alive afterwards.
    ///
    /// An empty return is the only proof that nothing writes the worktree any
    /// more, which is what re-queueing needs.
    pub(super) fn take_down_stage_agents(&mut self, stage_id: &str) -> Vec<String> {
        let agents = self.stage_agents(stage_id);

        let mut survivors = Vec::new();
        for session in &agents {
            if let Err(e) = self.backend.kill_session(session) {
                eprintln!("Warning: Failed to kill session '{}': {e}", session.id);
            }
            // The liveness probe decides, not the kill's return value: a kill
            // that reported an error may still have taken the agent down, and
            // one that reported success may not have (`TmuxBackend::kill_session`
            // returns `Ok` unconditionally, and the native lane returns `Ok`
            // when it refuses to signal an unverifiable identity).
            if self.confirm_session_gone(session) {
                self.record_context_exhausted(session);
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
        survivors
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
    /// A probe that ERRORS counts as gone, the same reading
    /// `session_registry` and `stage/skip_retry.rs` give it: a host that cannot
    /// answer the question must not wedge the run.
    fn confirm_session_gone(&self, session: &Session) -> bool {
        let deadline = Instant::now() + KILL_CONFIRM_TIMEOUT;
        loop {
            if !self.backend.is_session_alive(session).unwrap_or(false) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(KILL_CONFIRM_POLL_INTERVAL);
        }
    }
}
