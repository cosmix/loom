//! Session events, the 5 s tick, the start-up sweep and the server stop.

use std::sync::Arc;
use std::time::Instant;

use super::{evaluation, Attributed, Rebuilds};
use crate::plan_server::lifecycle::{line, safe};
use crate::plan_server::summaries::attribution::last_seal_error;
use crate::plan_server::summaries::scheduler::SuspendReason;
use crate::plan_session::bridge::{Bridge, BridgeEvent, EndOutcome};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::SessionState;
use crate::planning::summary::claims::list_claims;
use crate::planning::summary::evaluate::Evaluation;

impl Rebuilds {
    /// Follow one session bridge event (called from the bridge's listener, never under a lock
    /// of the bridge).
    pub(crate) fn on_bridge_event(&self, bridge: &Bridge, event: &BridgeEvent) {
        match event {
            BridgeEvent::Started { id, kind } => self.chat_started(id, kind),
            BridgeEvent::State { id, state } => {
                if matches!(state, SessionState::Idle | SessionState::Ended) {
                    self.chat_idle(id);
                }
            }
            BridgeEvent::TurnStarted { id, turn_id } => {
                self.turn_started(kind_of(bridge, id), id, turn_id);
            }
            BridgeEvent::TurnEnded { id, turn_id, .. } => self.turn_ended(bridge, id, turn_id),
            BridgeEvent::SubagentStopped { id, agent_id } => {
                self.subagent_stopped(bridge, id, agent_id);
            }
            BridgeEvent::Ended { id, kind, outcome } => {
                self.session_gone(bridge, id, kind, outcome)
            }
        }
    }

    /// An authoring or pressure session started; a pressure run suspends rebuilds until it ends.
    fn chat_started(&self, id: &str, kind: &SessionKind) {
        if *kind == SessionKind::Summary {
            return;
        }
        self.attribute(|attribution| attribution.chat_started(id));
        if matches!(kind, SessionKind::Pressure { .. }) {
            self.run(|state, _, _| {
                let token = state.scheduler.suspend(SuspendReason::PressureRun);
                state.pressure.insert(id.to_owned(), token);
            });
        }
    }

    /// The authoring session went Idle or Ended: end its turn if one is still open.
    fn chat_idle(&self, id: &str) {
        self.run(|state, spawner, now| {
            let open = state.open_turn.as_ref();
            if open.is_some_and(|(session, _)| session == id) {
                state.open_turn = None;
                let out_of_date = state.out_of_date();
                state
                    .scheduler
                    .authoring_state_idle(id, &out_of_date, now, spawner);
            }
        });
    }

    /// A turn began in session `id`, whose kind is `kind` (`None` when the bridge does not
    /// know it); only an authoring turn suspends rebuilds.
    pub(super) fn turn_started(&self, kind: Option<SessionKind>, id: &str, turn: &str) {
        if matches!(
            kind,
            Some(SessionKind::Authoring | SessionKind::Pressure { .. })
        ) {
            self.attribute(|attribution| attribution.turn_started(id, turn));
        }
        if kind == Some(SessionKind::Authoring) {
            self.run(|state, _, _| {
                state.open_turn = Some((id.to_owned(), turn.to_owned()));
                state.scheduler.turn_started();
            });
        }
    }

    /// The turn that holds the suspension ended; any chat's turn names what it rebuilt.
    fn turn_ended(&self, bridge: &Bridge, id: &str, turn: &str) {
        self.run(|state, spawner, now| {
            let open = state.open_turn.as_ref();
            if open.is_some_and(|(session, open)| session == id && open == turn) {
                state.open_turn = None;
                let out_of_date = state.out_of_date();
                state.scheduler.turn_ended(&out_of_date, now, spawner);
            }
        });
        let subagents = bridge.transcripts(id).map(|found| found.subagents);
        let subagents = subagents.unwrap_or_default();
        self.attribute(|attribution| attribution.turn_ended(id, turn, &subagents));
    }

    fn subagent_stopped(&self, bridge: &Bridge, id: &str, agent: &str) {
        let subagents = bridge.transcripts(id).map(|found| found.subagents);
        let path = subagents.and_then(|all| all.into_iter().find(|(found, _)| found == agent));
        let Some((_, path)) = path else {
            tracing::debug!(session = %id, %agent, "no transcript for a stopped subagent");
            return;
        };
        match self.attribute(|attribution| attribution.on_subagent_stopped(id, agent, &path)) {
            Some(Ok(attributed)) => self.record_costs(attributed),
            Some(Err(error)) => {
                let (agent, id) = (safe(agent), safe(id));
                let why = safe(&format!("{error:#}"));
                line(&format!(
                    "summaries: cannot attribute agent {agent} of {id}: {why}"
                ));
            }
            None => {}
        }
    }

    /// A session ended: clear its claims and cost its transcript first (the scheduler may start
    /// the next session for its ids at once), then end it in the scheduler.
    fn session_gone(&self, bridge: &Bridge, id: &str, kind: &SessionKind, outcome: &EndOutcome) {
        let main = bridge.transcripts(id).and_then(|found| found.main);
        let ended = self.attribute(|attribution| {
            attribution.note_main_transcript(id, main);
            attribution.on_session_ended(id)
        });
        match ended {
            Some(Ok(lines)) => self.record_costs(Attributed {
                lines,
                turn_cost: None,
            }),
            Some(Err(error)) => {
                let (id, why) = (safe(id), safe(&format!("{error:#}")));
                line(&format!(
                    "summaries: cannot clear the claims of session {id}: {why}"
                ));
            }
            None => {}
        }
        match kind {
            SessionKind::Summary => self.run(|state, spawner, now| match outcome {
                EndOutcome::Failed(why) => state.scheduler.session_failed(id, why, now, spawner),
                _ => state.scheduler.session_ended(id, now, spawner),
            }),
            SessionKind::Authoring => self.chat_idle(id),
            SessionKind::Pressure { .. } => self.run(|state, spawner, now| {
                if let Some(token) = state.pressure.remove(id) {
                    let out_of_date = state.out_of_date();
                    state.scheduler.resume(token, &out_of_date, now, spawner);
                }
            }),
        }
    }

    /// Every 5 s: the running session's claims (the watcher does not walk the runtime
    /// directory), its seal error when a stall is due, the stall itself, and the panel.
    pub(crate) fn tick(&self) {
        self.watch_claims();
        self.read_seal_error();
        self.run(|state, spawner, now| {
            let events = state.scheduler.tick(now, spawner);
            state.events.extend(events);
        });
        self.check_alive();
        self.publish_panel();
    }

    /// Apply a claims listing only when two in a row agree: a seal deletes its claim before
    /// the watcher's next batch shows the sealed file, and stopping in between would requeue
    /// summaries already sealed.
    fn watch_claims(&self) {
        let Some(env) = &self.env else {
            return;
        };
        let Some(session) = self.lock().running_session() else {
            return;
        };
        let held: Vec<String> = match list_claims(&env.runtime) {
            Ok(claims) => claims
                .into_iter()
                .filter(|claim| claim.session.as_deref() == Some(session.as_str()))
                .map(|claim| claim.summary)
                .collect(),
            Err(error) => {
                let why = safe(&format!("{error:#}"));
                line(&format!("summaries: cannot list the claims: {why}"));
                return;
            }
        };
        self.run(|state, spawner, now| {
            let stable = state.tick_claims.as_ref() == Some(&held);
            state.tick_claims = Some(held.clone());
            if stable {
                state
                    .scheduler
                    .claims_changed(&session, &held, now, spawner);
            }
        });
    }

    /// Before a stall stops the session, read its last failed seal from its transcript.
    fn read_seal_error(&self) {
        let Some(bridge) = self.env.as_ref().and_then(|env| env.bridge()) else {
            return;
        };
        let due = self
            .lock()
            .scheduler
            .stall_due(Instant::now())
            .map(str::to_owned);
        let Some(session) = due else {
            return;
        };
        let main = bridge.transcripts(&session).and_then(|found| found.main);
        let error = main.as_deref().and_then(last_seal_error);
        self.lock().scheduler.note_seal_error(&session, error);
    }

    /// End a running session the bridge no longer runs, in case its `Ended` never arrived.
    ///
    /// The bridge removes or marks a session and queues its `Ended` under one lock, then hands
    /// the event to listeners after dropping it, so a tick can see the session gone while its
    /// `Ended` (with the real failure reason) is still on its way. Act only when two ticks in a
    /// row see it gone: the first one's `Ended` has been delivered by the second.
    fn check_alive(&self) {
        let Some(bridge) = self.env.as_ref().and_then(|env| env.bridge()) else {
            return;
        };
        let running = self.lock().running_session();
        let Some(session) = running else {
            self.gone_twice(None);
            return;
        };
        let state = bridge
            .sessions()
            .into_iter()
            .find(|(id, ..)| *id == session);
        let outcome = match state.map(|(_, _, state)| state) {
            None | Some(SessionState::Ended) => EndOutcome::Closed,
            Some(SessionState::Failed) => EndOutcome::Failed("the session failed".to_owned()),
            Some(_) => {
                self.gone_twice(None);
                return;
            }
        };
        if self.gone_twice(Some(&session)) {
            self.session_gone(&bridge, &session, &SessionKind::Summary, &outcome);
        }
    }

    /// Record this tick's sighting of the running session gone (`Some`) or alive (`None`);
    /// true when the previous tick saw the same session gone too.
    fn gone_twice(&self, gone: Option<&str>) -> bool {
        let mut state = self.lock();
        let previous = state.tick_gone.take();
        let twice = gone.is_some() && previous.as_deref() == gone;
        if !twice {
            state.tick_gone = gone.map(str::to_owned);
        }
        twice
    }

    /// After plan 7's recovery: delete the claims of dead sessions, offer terminal claims for
    /// release, and queue what the engine's first update reported before this subscribed.
    pub(crate) fn after_recovery(
        &self,
        live: &[String],
        left_alone: &[String],
        latest: Arc<Evaluation>,
    ) {
        let (live_ids, left_ids): (Vec<&str>, Vec<&str>) = (
            live.iter().map(String::as_str).collect(),
            left_alone.iter().map(String::as_str).collect(),
        );
        let swept =
            self.attribute(|attribution| attribution.sweep_after_recovery(&live_ids, &left_ids));
        let terminal = match swept {
            Some(Ok(terminal)) => terminal,
            Some(Err(error)) => {
                let why = safe(&format!("{error:#}"));
                line(&format!(
                    "summaries: cannot sweep the claims of ended sessions: {why}"
                ));
                Vec::new()
            }
            None => Vec::new(),
        };
        self.run(|state, _, _| state.scheduler.set_terminal_claims(terminal));
        let alive: Vec<String> = live.iter().chain(left_alone).cloned().collect();
        let mut out_of_date = evaluation::out_of_date(&latest);
        out_of_date.extend(evaluation::swept(&latest, &alive));
        out_of_date.sort();
        out_of_date.dedup();
        self.apply_evaluation(latest, &out_of_date, &out_of_date);
    }

    /// The first Ctrl-C: drop the queue and the question; plan 7's drain ends a running session.
    pub(crate) fn server_stopping(&self) {
        self.run(|state, _, _| state.scheduler.server_stopping());
    }
}

fn kind_of(bridge: &Bridge, id: &str) -> Option<SessionKind> {
    let mut sessions = bridge.sessions().into_iter();
    sessions
        .find(|(found, ..)| found == id)
        .map(|(_, kind, _)| kind)
}
