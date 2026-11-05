//! Ending sessions: when an end is due, liveness, the teardown and its failure rules, the
//! marker end and its release.
//!
//! Teardown denies what is still pending first, then ends the tmux server; a failed teardown
//! keeps the record and the directory (never destroy the only handle) and is retried while
//! the process lives. Only then are verified children terminated and the directory removed,
//! except after a marker end, which keeps both until `release_session` (px-seams-01).

use std::time::{Duration, Instant};

use super::commands::Step;
use super::registry::{remove_session_dir, terminate_children, ChildRecord};
use super::requests::GONE;
use super::session::{Closing, EndPlan, Phase, Session, ENDED_KEPT};
use super::{Bridge, BridgeError, BridgeEvent, EndMode, EndOutcome};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::{DenyReason, Failure, FailureKind, SessionState};
use crate::plan_session::state::Input;
use crate::process::IdentityStatus;

/// How much longer than `final_write_bound` drain and recovery wait for the last teardowns.
pub(super) const END_SLACK: Duration = Duration::from_secs(5);
const DIED: &str = "The session stopped unexpectedly";
/// The start of every `EndOutcome::Failed` message whose teardown failed (the server's
/// terminal line tells those apart).
pub const TEARDOWN_FAILED: &str = "teardown failed";

/// What a claimed teardown needs after the lock.
pub(super) struct Teardown {
    socket: String,
    spawned: bool,
    children: Vec<ChildRecord>,
    dir: SessionDir,
}

impl Bridge {
    /// Tear a session down when its end is due.
    pub(super) fn end_step(&self, id: &str) {
        let stopping = self.lock().stopping;
        let bound = self.config.final_write_bound;
        let claimed = self.with_session(id, |session, step| {
            let (outcome, reason) = session.end_due(stopping, bound)?;
            Some((outcome, session.begin_teardown(reason, step)))
        });
        if let Some(Some((outcome, teardown))) = claimed {
            self.finish_teardown(id, outcome, teardown);
        }
    }

    /// A pressure session whose completion marker appeared ends with `Marker`.
    pub(super) fn marker_step(&self, id: &str) {
        let marker = {
            let inner = self.lock();
            let session = inner.sessions.get(id).filter(|session| {
                matches!(session.kind, SessionKind::Pressure { .. })
                    && matches!(session.phase, Phase::Live)
            });
            session.map(|session| session.dir.marker())
        };
        if marker.is_some_and(|marker| std::fs::symlink_metadata(marker).is_ok()) {
            self.with_session(id, |session, _| {
                if session.flow.close_ready.is_none() {
                    session.flow.close_ready = Some(EndOutcome::Marker);
                }
            });
        }
    }

    /// Every `identity_interval`: a dead process ends or fails the session; a failed
    /// teardown is retried while the process still verifies alive. A spawn that failed before
    /// its process was recorded has only its socket: its teardown is retried until it ends
    /// whatever still listens there (`teardown_socket` succeeds on a gone socket), as recovery
    /// does with such a record.
    pub(super) fn identity_step(&self, id: &str) {
        let interval = self.config.identity_interval;
        let check = self.with_session(id, |session, _| {
            let watched = matches!(session.phase, Phase::Live | Phase::TeardownFailed);
            if !watched || Instant::now() < session.next_identity {
                return None;
            }
            session.next_identity = Instant::now() + interval;
            let failed = matches!(session.phase, Phase::TeardownFailed);
            Some((session.record.identity(), failed))
        });
        let Some(Some((identity, failed))) = check else {
            return;
        };
        let Some(identity) = identity else {
            if failed {
                self.retry_teardown(id);
            }
            return;
        };
        match (self.host.identity(identity), failed) {
            (IdentityStatus::Dead, false) => self.process_gone(id),
            (IdentityStatus::VerifiedAlive, true) => self.retry_teardown(id),
            _ => {}
        }
    }

    /// Gone while ending: the end completes now. Gone otherwise: the session failed.
    fn process_gone(&self, id: &str) {
        let ending = self.with_session(id, |session, _| {
            let ending = session.flow.end.is_some();
            if ending && session.flow.close_ready.is_none() {
                let closed = session.closing() == Some(Closing::Confirmed);
                let outcome = if closed {
                    EndOutcome::Closed
                } else {
                    EndOutcome::TornDown
                };
                session.flow.close_ready = Some(outcome);
            }
            ending
        });
        match ending {
            Some(true) => self.end_step(id),
            Some(false) => {
                let died = DIED.to_owned();
                self.fail_live(id, Input::ProcessGone, FailureKind::Died, died);
            }
            None => {}
        }
    }

    /// A live session failed (its process died, or its opening could not be pasted): Failed,
    /// pending requests denied, the tmux server and children ended; the directory stays until
    /// the next start clears it. Nothing restarts.
    pub(super) fn fail_live(
        &self,
        id: &str,
        input: Input<'static>,
        kind: FailureKind,
        message: String,
    ) {
        let claimed = self.with_session(id, |session, step| {
            if !matches!(session.phase, Phase::Live) {
                return None;
            }
            session.machine.apply(input);
            let message = message.clone();
            session.failure = Some(Failure { kind, message });
            Some(session.begin_teardown(DenyReason::SessionEnded, step))
        });
        let Some(Some(teardown)) = claimed else {
            return;
        };
        if let Err(error) = self.host.teardown(&teardown.socket) {
            // The failure's cause stays in the message the browser and `Ended` carry.
            return self.teardown_failed(id, kind, format!("{error:#} (after: {message})"));
        }
        terminate_children(&teardown.children);
        self.conclude(id, EndOutcome::Failed(message), Some(Phase::Leftover));
    }

    pub(super) fn finish_teardown(&self, id: &str, outcome: EndOutcome, teardown: Teardown) {
        if teardown.spawned {
            if let Err(error) = self.host.teardown(&teardown.socket) {
                return self.teardown_failed(id, FailureKind::Died, format!("{error:#}"));
            }
        }
        let keep = outcome == EndOutcome::Marker;
        if !keep {
            terminate_children(&teardown.children);
            if let Err(error) = remove_session_dir(&teardown.dir) {
                tracing::warn!(session = %id, %error, "cannot remove the session directory");
            }
        }
        self.conclude(id, outcome, keep.then_some(Phase::AwaitRelease));
    }

    /// Failed, record and directory kept; the event goes out once (px-state-18). The failure
    /// message is [`TEARDOWN_FAILED`] and `detail`.
    pub(super) fn teardown_failed(&self, id: &str, kind: FailureKind, detail: String) {
        let message = format!("{TEARDOWN_FAILED}: {detail}");
        tracing::warn!(session = %id, %message, "session teardown failed");
        let first = self.with_session(id, |session, _| {
            let first = session.forced != Some(SessionState::Failed);
            session.forced = Some(SessionState::Failed);
            let message = message.clone();
            session.failure = Some(Failure { kind, message });
            session.phase = Phase::TeardownFailed;
            first
        });
        if first == Some(true) {
            self.conclude(id, EndOutcome::Failed(message), Some(Phase::TeardownFailed));
        }
    }

    fn retry_teardown(&self, id: &str) {
        let claim = self.with_session(id, |session, _| {
            let failed = matches!(session.phase, Phase::TeardownFailed);
            let children = session.record.children.clone();
            failed.then(|| (session.socket().to_owned(), children, session.dir.clone()))
        });
        let Some(Some((socket, children, dir))) = claim else {
            return;
        };
        if self.host.teardown(&socket).is_err() {
            return;
        }
        terminate_children(&children);
        if let Err(error) = remove_session_dir(&dir) {
            tracing::warn!(session = %id, %error, "cannot remove the session directory");
        }
        self.lock().sessions.remove(id);
    }

    /// Publish `Ended`, move an ended chat to `previous`; `keep` leaves the session listed in
    /// that phase, `None` drops it.
    pub(super) fn conclude(&self, id: &str, outcome: EndOutcome, keep: Option<Phase>) {
        let mut inner = self.lock();
        let chat = inner.chat.as_deref() == Some(id);
        let Some(session) = inner.sessions.get_mut(id) else {
            return;
        };
        let mut step = Step::new(chat);
        if !matches!(
            session.machine.state(),
            SessionState::Ended | SessionState::Failed
        ) {
            session.machine.apply(Input::EndRequested);
            session.machine.apply(Input::ProcessGone);
        }
        session.sync(&mut step);
        let (ended, transcript) = (session.ended_chat(), session.transcript.clone());
        let kind = session.kind.clone();
        step.event(BridgeEvent::Ended {
            id: id.to_owned(),
            kind,
            outcome,
        });
        match keep {
            Some(phase) => session.phase = phase,
            None => {
                inner.sessions.remove(id);
            }
        }
        if chat {
            inner.previous = Some(ended);
            inner.chat = None;
        }
        inner.ended.push_back((id.to_owned(), transcript));
        if inner.ended.len() > ENDED_KEPT {
            inner.ended.pop_front();
        }
        inner.outbox.append(&mut step.out);
        drop(inner);
        self.flush();
    }

    /// After a marker end: terminate the children still verified and remove the directory.
    pub fn release_session(&self, id: &str) -> Result<(), BridgeError> {
        let released = {
            let mut inner = self.lock();
            let waiting = inner
                .sessions
                .get(id)
                .map(|s| matches!(s.phase, Phase::AwaitRelease));
            match waiting {
                None => return Err(BridgeError::NoSession),
                Some(false) => {
                    let why = "the session has not ended with its marker";
                    return Err(BridgeError::Refused(why.to_owned()));
                }
                Some(true) => inner.sessions.remove(id),
            }
        };
        if let Some(session) = released {
            terminate_children(&session.record.children);
            let removed = remove_session_dir(&session.dir);
            removed.map_err(|error| BridgeError::Io(format!("{error:#}")))?;
        }
        Ok(())
    }
}

impl Session {
    /// Ready to close, a teardown asked for, the final-write bound passed, never launched, its
    /// opening never delivered (no turn ran, so there is no task to finish and no closing
    /// message to paste), or the server stopping: the outcome and the reason pending
    /// requests are denied with.
    fn end_due(&mut self, stopping: bool, bound: Duration) -> Option<(EndOutcome, DenyReason)> {
        if !matches!(self.phase, Phase::Live | Phase::Spawning(_)) {
            return None;
        }
        let plan = self.flow.end.as_ref();
        let reason = match stopping {
            true => DenyReason::ServerStopping,
            false => plan.map_or(DenyReason::SessionEnded, |plan| plan.reason),
        };
        if let Some(outcome) = self.flow.close_ready.take() {
            return Some((outcome, reason));
        }
        let unlaunched = matches!(self.phase, Phase::Spawning(_));
        let unopened = self.flow.opening.is_some();
        let due = match plan {
            Some(plan) => {
                let late = plan.requested_at.elapsed() >= bound;
                plan.mode == EndMode::Teardown || unlaunched || unopened || late
            }
            None => stopping,
        };
        due.then_some((EndOutcome::TornDown, reason))
    }

    /// Deny what is pending, drop what was never pasted and claim the session for its
    /// teardown. An end nobody asked for (the process died, a `SessionEnd`) has no caller to
    /// return the queue to: it is cleared, and the tabs hear it.
    pub(super) fn begin_teardown(&mut self, reason: DenyReason, step: &mut Step) -> Teardown {
        let message = match self.flow.end.as_ref().and_then(|plan| plan.deny_new) {
            Some((_, message)) if reason != DenyReason::SessionEnded => message,
            _ if reason == DenyReason::ServerStopping => "the server is stopping",
            _ => GONE,
        };
        self.deny_open(reason, message, step);
        self.return_unsent(step);
        let spawned = matches!(self.phase, Phase::Live | Phase::TeardownFailed);
        self.phase = Phase::TearingDown;
        Teardown {
            socket: self.record.socket.clone(),
            spawned,
            children: self.record.children.clone(),
            dir: self.dir.clone(),
        }
    }

    /// The first end request sets the plan; a later one may only harden it.
    pub(super) fn plan_end(
        &mut self,
        mut mode: EndMode,
        deny_new: Option<(DenyReason, &'static str)>,
    ) {
        let reason = deny_new.map_or(DenyReason::SessionEnded, |(reason, _)| reason);
        if let Some(plan) = &mut self.flow.end {
            if mode == EndMode::Teardown {
                plan.mode = mode;
            }
            if deny_new.is_some() {
                (plan.deny_new, plan.reason) = (deny_new, reason);
            }
            return;
        }
        self.machine.apply(Input::EndRequested);
        // The closing write needs something to write: a session the user never wrote in
        // (and that was not recovered, whose transcript is not read back yet) is torn down.
        let writable = self.flow.recovered.is_some() || self.user_wrote();
        let closing = match self.kind {
            SessionKind::Authoring if writable => Closing::NotSent,
            SessionKind::Authoring => {
                mode = EndMode::Teardown;
                Closing::Skip
            }
            _ => Closing::Skip,
        };
        self.flow.end = Some(EndPlan {
            mode,
            requested_at: Instant::now(),
            closing,
            deny_new,
            reason,
        });
    }
}
