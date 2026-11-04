//! The commands the server calls on a running session (send, withdraw, permission answers,
//! interrupt, end, child records, token transcripts) and the step plumbing every change of a
//! session goes through.
//!
//! A command changes the session under the lock and returns; host calls happen on the driver.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::registry::ChildRecord;
use super::session::{Closing, EndPlan, Phase, Queued, Session};
use super::{Bridge, BridgeConfig, BridgeError, BridgeEvent, EndCause, EndMode, SendOutcome};
use crate::plan_session::attachments::{parse_unwrapped, Attachment};
use crate::plan_session::input::{check_delivered, sanitize};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::{ChatDelta, DenyReason, NoticeKind, SessionState};
use crate::plan_session::permission::{write_answer, Answer, Behavior};
use crate::plan_session::state::Input;
use crate::process::ProcessIdentity;

/// Messages that may wait in the queue.
const MAX_QUEUED: usize = 20;
/// How long the queue waits for the idle an Escape causes (px-state-04).
const QUIET: Duration = Duration::from_secs(5);
const INTERRUPTED: &str = "the user stopped this turn";
const HOLDER_GONE: &str = "the chat is closing: its holder is away";
const STOPPING: &str = "the server is stopping";

/// What a step queued for the listeners, in order.
pub(super) enum Out {
    Event(BridgeEvent),
    Delta(Box<ChatDelta>),
}

/// Work a step leaves for after the lock: answer files and turn pins.
pub(super) enum Effect {
    Answer {
        dir: PathBuf,
        request_id: String,
        answer: Answer,
    },
    BeginPins {
        turn: String,
    },
    EndPins {
        turn: String,
    },
}

/// One locked step on a session: what it publishes and what it leaves for after the lock.
pub(super) struct Step {
    pub(super) out: Vec<Out>,
    pub(super) effects: Vec<Effect>,
    /// The session is the chat session: its deltas are published.
    pub(super) chat: bool,
}

impl Step {
    pub(super) fn new(chat: bool) -> Self {
        Step {
            out: Vec::new(),
            effects: Vec::new(),
            chat,
        }
    }

    pub(super) fn event(&mut self, event: BridgeEvent) {
        self.out.push(Out::Event(event));
    }
}

impl Bridge {
    /// Run `f` on a session under the lock, settle it and queue what it published; then run
    /// its effects. `None` without such a session; the string is the first failed answer.
    pub(super) fn with_session_io<R>(
        &self,
        id: &str,
        f: impl FnOnce(&mut Session, &mut Step) -> R,
    ) -> Option<(R, Option<String>)> {
        let (result, effects) = {
            let mut inner = self.lock();
            let chat = inner.chat.as_deref() == Some(id);
            let session = inner.sessions.get_mut(id)?;
            let mut step = Step::new(chat);
            let result = f(session, &mut step);
            session.settle(&mut step);
            inner.outbox.append(&mut step.out);
            (result, step.effects)
        };
        let failed = self.run_effects(id, effects);
        Some((result, failed))
    }

    pub(super) fn with_session<R>(
        &self,
        id: &str,
        f: impl FnOnce(&mut Session, &mut Step) -> R,
    ) -> Option<R> {
        self.with_session_io(id, f).map(|(result, _)| result)
    }

    pub(super) fn step(&self, id: &str, f: impl FnOnce(&mut Session, &BridgeConfig, &mut Step)) {
        let config = &self.config;
        self.with_session(id, |session, step| f(session, config, step));
    }

    /// Answer files and turn pins in step order; a turn's end runs `finish_turn` at once.
    fn run_effects(&self, id: &str, effects: Vec<Effect>) -> Option<String> {
        let mut failed = None;
        for effect in effects {
            match effect {
                Effect::Answer {
                    dir,
                    request_id,
                    answer,
                } => {
                    if let Err(error) = write_answer(&dir, &request_id, &answer) {
                        tracing::warn!(session = %id, %request_id, %error, "answer not written");
                        if failed.is_none() {
                            failed = Some(format!("{error:#}"));
                        }
                    }
                }
                Effect::BeginPins { turn } => {
                    if let Some(pins) = &self.pins {
                        pins.begin_turn(&format!("{id}/{turn}"));
                    }
                }
                Effect::EndPins { turn } => {
                    let key = format!("{id}/{turn}");
                    let pinned = self.pins.as_ref().map(|pins| pins.end_turn(&key));
                    let pinned = pinned.unwrap_or_default();
                    self.with_session(id, |session, step| session.finish_turn(&turn, pinned, step));
                }
            }
        }
        failed
    }

    /// `text` is the formatted message (attachment block included); `attachments` its chips.
    pub fn send(
        &self,
        id: &str,
        text: String,
        attachments: Vec<Attachment>,
    ) -> Result<SendOutcome, BridgeError> {
        let text = sanitize(&text);
        check_delivered(&text).map_err(BridgeError::Input)?;
        let sent = self.with_session(id, |session, step| session.send(text, attachments, step));
        self.flush();
        sent.unwrap_or(Err(BridgeError::NoSession))
    }

    /// Remove a queued message; the one being delivered is no longer queued.
    pub fn withdraw(&self, id: &str, queue_id: &str) -> Result<(), BridgeError> {
        let withdrawn = self.with_session(id, |session, step| {
            if !session.accepts_input() {
                return Err(BridgeError::NotRunning);
            }
            let queue = &mut session.flow.queue;
            let at = queue.iter().position(|queued| queued.id == queue_id);
            let gone = || BridgeError::Refused("the message is no longer queued".to_owned());
            queue.remove(at.ok_or_else(gone)?);
            session.publish_queue(step);
            Ok(())
        });
        self.flush();
        withdrawn.unwrap_or(Err(BridgeError::NoSession))
    }

    /// Allow, or deny with the holder's note; a closed card is refused (px-state-R03).
    pub fn answer_permission(
        &self,
        id: &str,
        request_id: &str,
        allow: bool,
        note: Option<String>,
    ) -> Result<(), BridgeError> {
        let answered = self.with_session_io(id, |session, step| {
            if !session
                .flow
                .requests
                .get(request_id)
                .is_some_and(|request| request.open)
            {
                let why = "the session no longer waits for this answer";
                return Err(BridgeError::Refused(why.to_owned()));
            }
            let (behavior, message, reason) = if allow {
                (Behavior::Allow, None, None)
            } else {
                (Behavior::Deny, note, Some(DenyReason::Holder))
            };
            let answer = Answer {
                behavior,
                message,
                reason,
            };
            session.answer(request_id, answer, false, step);
            Ok(())
        });
        self.flush();
        match answered {
            None => Err(BridgeError::NoSession),
            Some((Err(refused), _)) => Err(refused),
            Some((Ok(()), Some(failed))) => Err(BridgeError::Io(failed)),
            Some((Ok(()), None)) => Ok(()),
        }
    }

    /// Stop the running turn: deny what is pending, return the queue and a message not yet
    /// pasted, press Escape once.
    pub fn interrupt(&self, id: &str) -> Result<Vec<String>, BridgeError> {
        let stopped = self.with_session(id, |session, step| session.interrupt(step));
        self.flush();
        stopped.unwrap_or(Err(BridgeError::NoSession))
    }

    /// Start ending a session; returns the queued texts it will not deliver.
    pub fn end(
        &self,
        id: &str,
        mode: EndMode,
        cause: EndCause,
    ) -> Result<Vec<String>, BridgeError> {
        let ended = self.with_session(id, |session, step| session.request_end(mode, cause, step));
        self.flush();
        ended.unwrap_or(Err(BridgeError::NoSession))
    }

    /// Record a process started for the session (kept by a marker end, terminated otherwise).
    pub fn record_child(
        &self,
        id: &str,
        label: &str,
        identity: ProcessIdentity,
    ) -> Result<(), BridgeError> {
        let child = ChildRecord {
            label: label.to_owned(),
            pid: identity.pid,
            start_time: identity.start_time,
        };
        let recorded = self.with_session(id, |session, _| {
            if !matches!(session.phase, Phase::Live) {
                return Err(BridgeError::NotRunning);
            }
            session.record.children.push(child);
            session.record_dirty = true;
            Ok(())
        });
        recorded.unwrap_or(Err(BridgeError::NoSession))
    }

    /// Also tail `path` and add each line's usage to the open turn (px-seams-15).
    pub fn add_token_transcript(&self, id: &str, path: PathBuf) -> Result<(), BridgeError> {
        let added = self.with_session(id, |session, _| {
            if !session.active() {
                return Err(BridgeError::NotRunning);
            }
            if !session.token_paths.contains(&path) {
                session.token_paths.push(path);
            }
            Ok(())
        });
        added.unwrap_or(Err(BridgeError::NoSession))
    }
}

impl Session {
    /// Neither ending nor ended: sends and withdrawals are taken.
    fn accepts_input(&self) -> bool {
        let ended = matches!(self.state(), SessionState::Ended | SessionState::Failed);
        let torn = matches!(self.phase, Phase::TearingDown);
        self.flow.end.is_none() && self.active() && !torn && !ended
    }

    pub(super) fn send(
        &mut self,
        text: String,
        attachments: Vec<Attachment>,
        step: &mut Step,
    ) -> Result<SendOutcome, BridgeError> {
        if !self.accepts_input() {
            return Err(BridgeError::NotRunning);
        }
        let idle = matches!(self.phase, Phase::Live) && self.state() == SessionState::Idle;
        let waits = self.flow.paused || self.flow.quiet_until.is_some() || self.flow.skip_delivery;
        let busy = self.flow.delivering.is_some() || self.flow.opening.is_some();
        if idle && !waits && !busy && self.flow.queue.is_empty() {
            let delivering = self.delivery(text, attachments, None, step);
            let item_id = delivering.item_ids.first().cloned().unwrap_or_default();
            self.flow.delivering = Some(delivering);
            return Ok(SendOutcome::Delivering { item_id });
        }
        if self.flow.queue.len() >= MAX_QUEUED {
            let why = format!("at most {MAX_QUEUED} messages can wait");
            return Err(BridgeError::Refused(why));
        }
        let queue_id = self.next_id("q-");
        let (user_text, _) = parse_unwrapped(&text);
        let id = queue_id.clone();
        let queued = Queued {
            id,
            text,
            user_text,
            attachments,
        };
        self.flow.queue.push_back(queued);
        self.publish_queue(step);
        Ok(SendOutcome::Queued { queue_id })
    }

    /// Only while a turn runs; one Escape per turn (px-state-04). A message accepted at an
    /// idle and held since (a task notification started a turn first) comes back too: nothing
    /// is pasted at the idle the interrupt causes.
    fn interrupt(&mut self, step: &mut Step) -> Result<Vec<String>, BridgeError> {
        let running = matches!(
            self.state(),
            SessionState::Working | SessionState::Permission
        );
        if !running || !matches!(self.phase, Phase::Live) {
            return Err(BridgeError::NotRunning);
        }
        self.deny_open(DenyReason::Interrupt, INTERRUPTED, step);
        let texts = self.return_unsent(step);
        if !self.flow.pressed {
            self.flow.pressed = true;
            self.flow.press_pending = true;
            self.flow.quiet_until = Some(Instant::now() + QUIET);
            let text = if texts.is_empty() {
                "Stopped"
            } else {
                "Stopped; the queued messages went back to the tab that stopped"
            };
            self.notice(NoticeKind::Stopped, text, step);
        }
        Ok(texts)
    }

    fn request_end(
        &mut self,
        mode: EndMode,
        cause: EndCause,
        step: &mut Step,
    ) -> Result<Vec<String>, BridgeError> {
        let ended = matches!(self.state(), SessionState::Ended | SessionState::Failed);
        if ended || !self.active() || matches!(self.phase, Phase::Preparing | Phase::TearingDown) {
            return Err(BridgeError::NotRunning);
        }
        let deny_new = match cause {
            EndCause::GraceExpired => Some((DenyReason::Holder, HOLDER_GONE)),
            EndCause::Draining => Some((DenyReason::ServerStopping, STOPPING)),
            EndCause::Holder | EndCause::Caller => None,
        };
        // A pressure session never gets a closing message: its final write is a teardown.
        let mode = match self.kind {
            SessionKind::Pressure { .. } => EndMode::Teardown,
            _ => mode,
        };
        self.plan_end(mode, deny_new);
        if let Some((reason, message)) = deny_new {
            self.deny_open(reason, message, step);
        }
        Ok(self.return_unsent(step))
    }

    /// The first end request sets the plan; a later one may only harden it.
    pub(super) fn plan_end(&mut self, mode: EndMode, deny_new: Option<(DenyReason, &'static str)>) {
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
        let closing = match self.kind {
            SessionKind::Authoring => Closing::NotSent,
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
