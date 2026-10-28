//! The session state machine: pure transitions over hook events and transcript records.
//!
//! No clock reads and no I/O: every time comes in with its input (a spool event's
//! `received_at`, a record's `at`), so the bridge can replay a spool and get the same answer.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::model::{Activity, SessionState};
use super::spool::SpoolEvent;

/// One piece of evidence about what the session is doing.
#[derive(Debug, Clone, PartialEq)]
pub enum Input<'a> {
    /// A hook event from the spool.
    Spool(&'a SpoolEvent),
    /// A transcript record; `main` is false for a subagent transcript's record.
    Record { at: DateTime<Utc>, main: bool },
    /// The `tool_result` of the tool call a pending request names arrived.
    RequestResolved { request_id: String },
    /// An `[Request interrupted by user` record.
    Interrupted { at: DateTime<Utc> },
    /// The `agents_killed` system record.
    AgentsKilled { at: DateTime<Utc> },
    /// A turn Claude Code started without a prompt (a `task-notification` record).
    SelfStartedTurn { at: DateTime<Utc> },
    /// The Claude Code process is gone.
    ProcessGone,
    /// The session never started, or its opening could not be pasted so no turn ever ran.
    SpawnFailed,
    /// The bridge began ending the session.
    EndRequested,
    /// The bridge wrote an answer for this request.
    PermissionAnswered { request_id: String },
    /// The bridge's own wait for this request expired.
    PermissionExpired { request_id: String },
    /// The bridge delivered a `/compact`.
    CompactDelivered,
}

/// The session state, derived from the inputs applied so far.
#[derive(Debug, Clone, PartialEq)]
pub struct StateMachine {
    /// `SessionStart { startup }` arrived.
    started: bool,
    /// `EndRequested` arrived: the state stays Ending until Ended.
    ending: bool,
    /// Ended or Failed; nothing moves the machine once set.
    terminal: Option<SessionState>,
    /// Idle or Working underneath any pending permission.
    busy: bool,
    pending: BTreeSet<String>,
    last_idle_at: Option<DateTime<Utc>>,
    last_main_at: Option<DateTime<Utc>>,
    subagents: u32,
    compact_delivered: bool,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl StateMachine {
    /// A machine for a session just spawned: Starting. Its activity underneath is Working, so
    /// an Ending session is never taken for idle before Claude Code reported its start.
    pub fn new() -> Self {
        Self {
            started: false,
            ending: false,
            terminal: None,
            busy: true,
            pending: BTreeSet::new(),
            last_idle_at: None,
            last_main_at: None,
            subagents: 0,
            compact_delivered: false,
        }
    }

    /// Apply one input; `Some` with the new state when `state()` changed.
    pub fn apply(&mut self, input: Input<'_>) -> Option<SessionState> {
        if self.terminal.is_some() {
            return None;
        }
        let before = self.state();
        match input {
            Input::Spool(event) => self.spool(event),
            Input::Record { at, main } => self.record(at, main),
            Input::RequestResolved { request_id }
            | Input::PermissionAnswered { request_id }
            | Input::PermissionExpired { request_id } => {
                self.pending.remove(&request_id);
            }
            Input::Interrupted { at } => {
                self.idle(at);
                self.pending.clear();
                self.subagents = 0;
            }
            Input::AgentsKilled { .. } => self.subagents = 0,
            Input::SelfStartedTurn { at } => {
                self.note_main(at);
                // A task-notification only counts after the last idle event, as a record does.
                if self.last_idle_at.is_none_or(|idle| at > idle) {
                    self.busy = true;
                }
            }
            Input::ProcessGone => {
                let end = if self.ending {
                    SessionState::Ended
                } else {
                    SessionState::Failed
                };
                self.terminal = Some(end);
            }
            Input::SpawnFailed => self.terminal = Some(SessionState::Failed),
            Input::EndRequested => self.ending = true,
            Input::CompactDelivered => self.compact_delivered = true,
        }
        let after = self.state();
        (after != before).then_some(after)
    }

    /// The state shown to the browser.
    pub fn state(&self) -> SessionState {
        if let Some(end) = self.terminal {
            return end;
        }
        if self.ending {
            return SessionState::Ending;
        }
        if !self.started {
            return SessionState::Starting;
        }
        match self.activity() {
            Activity::Idle => SessionState::Idle,
            Activity::Working => SessionState::Working,
            Activity::Permission => SessionState::Permission,
        }
    }

    /// What the session is doing, also underneath Ending.
    pub fn activity(&self) -> Activity {
        if !self.pending.is_empty() {
            Activity::Permission
        } else if self.busy {
            Activity::Working
        } else {
            Activity::Idle
        }
    }

    /// The permission requests waiting for an answer.
    pub fn pending_requests(&self) -> &BTreeSet<String> {
        &self.pending
    }

    /// Subagents started and not yet stopped.
    pub fn subagents_running(&self) -> u32 {
        self.subagents
    }

    /// `received_at` (or the record's `at`) of the last idle evidence that won.
    pub fn last_idle_at(&self) -> Option<DateTime<Utc>> {
        self.last_idle_at
    }

    /// `at` of the newest main transcript record applied.
    pub fn last_main_at(&self) -> Option<DateTime<Utc>> {
        self.last_main_at
    }

    fn spool(&mut self, event: &SpoolEvent) {
        let at = event.received_at;
        let payload = &event.payload;
        match event.event.as_str() {
            "SessionStart" => self.session_start(at, text(payload, "source")),
            "UserPromptSubmit" => self.busy = true,
            "PermissionRequest" => self.permission_request(event),
            "Notification" if is_idle_prompt(payload) => {
                self.idle(at);
            }
            // Claude Code ends a turn whose API call failed with StopFailure, never Stop.
            "Stop" | "StopFailure" => {
                if self.idle(at) {
                    self.pending.clear();
                }
            }
            "SubagentStart" => self.subagents = self.subagents.saturating_add(1),
            "SubagentStop" => self.subagents = self.subagents.saturating_sub(1),
            "SessionEnd" => self.terminal = Some(SessionState::Ended),
            _ => {}
        }
    }

    fn session_start(&mut self, at: DateTime<Utc>, source: Option<&str>) {
        match source {
            Some("startup") => {
                self.started = true;
                self.subagents = 0;
                self.idle(at);
            }
            // Only a `/compact` the bridge delivered ends in Idle; an auto-compaction mid-turn
            // reports the same source and changes nothing (px-state-10).
            Some("compact") if self.compact_delivered => {
                self.compact_delivered = false;
                self.subagents = 0;
                self.idle(at);
            }
            Some("compact") | None => {}
            Some(_) => self.subagents = 0,
        }
    }

    fn permission_request(&mut self, event: &SpoolEvent) {
        // The hook answered an auto-denied request itself: nobody waits on it.
        if event.auto_denied() {
            return;
        }
        let id = event
            .request_id
            .as_deref()
            .or_else(|| text(&event.payload, "request_id"));
        if let Some(id) = id {
            self.pending.insert(id.to_owned());
            self.busy = true;
        }
    }

    fn record(&mut self, at: DateTime<Utc>, main: bool) {
        // Subagent records never move the state.
        if !main {
            return;
        }
        self.note_main(at);
        // A record after the last idle event means working, whatever the spool says.
        if self.last_idle_at.is_none_or(|idle| at > idle) {
            self.busy = true;
        }
    }

    /// Idle evidence at `at`; it loses (returns false) to a main record already seen later.
    fn idle(&mut self, at: DateTime<Utc>) -> bool {
        if self.last_main_at.is_some_and(|main| main > at) {
            return false;
        }
        self.busy = false;
        self.last_idle_at = Some(self.last_idle_at.map_or(at, |idle| idle.max(at)));
        true
    }

    fn note_main(&mut self, at: DateTime<Utc>) {
        self.last_main_at = Some(self.last_main_at.map_or(at, |main| main.max(at)));
    }
}

/// The string at `key` in a hook payload.
pub(crate) fn text<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

/// `notification_type: idle_prompt`, or a message saying Claude waits for input (older
/// Claude Code versions send no type).
pub(crate) fn is_idle_prompt(payload: &Value) -> bool {
    match text(payload, "notification_type") {
        Some(kind) => kind == "idle_prompt",
        None => text(payload, "message")
            .is_some_and(|message| message.contains("waiting for your input")),
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "state_tests_more.rs"]
mod tests_more;
