//! One session's bookkeeping behind the bridge's lock: its record, state machine, mapper,
//! queue, permission requests and timers, and how a step publishes it.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;

use super::commands::{Out, Step};
use super::host::LaunchSpec;
use super::registry::SessionRecord;
use super::{BridgeEvent, EndMode, EndOutcome, RecoveryReport};
use crate::plan_session::attachments::Attachment;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::{
    ChangedFile, ChatDelta, DenyReason, EndedChat, Failure, Origin, QueuedMessage, SessionState,
    SessionView, Turn,
};
use crate::plan_session::permission::Answer;
use crate::plan_session::state::StateMachine;
use crate::plan_session::transcript::Mapper;

/// A finished subagent transcript that has not grown for this long counts as stopped.
const SUBAGENT_STALL: Duration = Duration::from_secs(30);
/// A queued message's first line in the queue list.
const FIRST_LINE_CHARS: usize = 120;
/// Sessions whose transcripts `transcripts()` still answers for after they ended.
pub(super) const ENDED_KEPT: usize = 32;

/// A callback parked until the startup recovery pass reports.
pub(super) type RecoveryWaiter = Box<dyn FnOnce(&RecoveryReport) + Send>;

pub(super) enum Recovery {
    NotRun,
    Running,
    Done(RecoveryReport),
}

/// Everything behind the bridge's lock.
pub(super) struct Inner {
    pub(super) sessions: BTreeMap<String, Session>,
    pub(super) chat: Option<String>,
    pub(super) previous: Option<EndedChat>,
    pub(super) outbox: Vec<Out>,
    pub(super) recovery: Recovery,
    pub(super) waiting: Vec<RecoveryWaiter>,
    pub(super) stopping: bool,
    /// `(id, main transcript)` of the latest ended sessions.
    pub(super) ended: VecDeque<(String, Option<PathBuf>)>,
    pub(super) stall_after: Duration,
}

impl Inner {
    pub(super) fn new() -> Self {
        Inner {
            sessions: BTreeMap::new(),
            chat: None,
            previous: None,
            outbox: Vec::new(),
            recovery: Recovery::NotRun,
            waiting: Vec::new(),
            stopping: false,
            ended: VecDeque::new(),
            stall_after: SUBAGENT_STALL,
        }
    }
}

/// Where a session is in its life, as the bridge drives it.
pub(super) enum Phase {
    /// `start_session` reserved it and runs the guidance.
    Preparing,
    /// Waiting for the driver to launch it.
    Spawning(Box<LaunchSpec>),
    /// The driver is launching it.
    Launching,
    Live,
    /// A teardown claimed it.
    TearingDown,
    /// Marker end: children and directory kept until `release_session`.
    AwaitRelease,
    /// Failed; its directory stays until the next start clears it.
    Leftover,
    /// Failed teardown: record and directory kept, teardown retried while the process lives.
    TeardownFailed,
}

pub(super) struct Shown {
    pub(super) state: SessionState,
    pub(super) since: String,
    pub(super) working_since: Option<String>,
    pub(super) view: Option<SessionView>,
}

pub(super) struct Opening {
    pub(super) text: String,
    pub(super) origin: Option<Origin>,
}

pub(super) struct Queued {
    pub(super) id: String,
    /// The sanitised text as it will be pasted.
    pub(super) text: String,
    /// The user's text without the attachment block.
    pub(super) user_text: String,
    pub(super) attachments: Vec<Attachment>,
}

pub(super) struct Delivering {
    pub(super) item_id: String,
    pub(super) text: String,
    /// The sanitised user text a transcript record of this delivery carries.
    pub(super) match_text: String,
    /// `None` until the driver pasted it.
    pub(super) sent_at: Option<Instant>,
    pub(super) closing: bool,
    /// The opening: a failed paste of it fails the session (no turn ever ran).
    pub(super) opening: bool,
    pub(super) compact: bool,
}

pub(super) struct Request {
    pub(super) tool: String,
    pub(super) input: Value,
    pub(super) expires_at: DateTime<Utc>,
    /// The `tool_use_id` of the pending tool call this request is bound to.
    pub(super) tool_use: Option<String>,
    pub(super) open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Closing {
    /// No closing message (summary and pressure sessions).
    Skip,
    NotSent,
    Sent,
    Confirmed,
}

pub(super) struct EndPlan {
    pub(super) mode: EndMode,
    pub(super) requested_at: Instant,
    pub(super) closing: Closing,
    /// Deny every request at once while ending, with this reason and message.
    pub(super) deny_new: Option<(DenyReason, &'static str)>,
    /// The reason teardown denies what is still pending.
    pub(super) reason: DenyReason,
}

#[derive(Default)]
pub(super) struct AgentWatch {
    /// When the transcript last grew with a final assistant record as its last line.
    pub(super) final_at: Option<Instant>,
    pub(super) stopped: bool,
}

/// `received_at`, event, request id and a hash of the payload of an applied spool event.
pub(super) type EventKey = (DateTime<Utc>, String, Option<String>, u64);

/// The conversation's moving parts: deliveries, requests, turns, timers and the end.
#[derive(Default)]
pub(super) struct Flow {
    pub(super) opening: Option<Opening>,
    pub(super) queue: VecDeque<Queued>,
    pub(super) delivering: Option<Delivering>,
    /// `(item id, match text, closing)` of deliveries that timed out, latest last.
    pub(super) undelivered: Vec<(String, String, bool)>,
    /// After a delivery timeout the queue waits for the next Stop or idle prompt.
    pub(super) paused: bool,
    /// After an Escape the queue waits for the interrupt's idle, at most until this instant.
    pub(super) quiet_until: Option<Instant>,
    /// The idle that ended an interrupted turn delivers nothing.
    pub(super) skip_delivery: bool,
    pub(super) counter: u64,
    pub(super) requests: BTreeMap<String, Request>,
    pub(super) applied: HashSet<EventKey>,
    /// The turn `TurnStarted` was published for and not yet ended.
    pub(super) turn: Option<String>,
    /// That turn's `UserPromptSubmit` arrived (a turn opened by its records has not seen it).
    pub(super) prompted: bool,
    pub(super) changed: HashMap<String, Vec<ChangedFile>>,
    pub(super) press_pending: bool,
    /// Escape was pressed in this turn: no second press until it ends.
    pub(super) pressed: bool,
    pub(super) end: Option<EndPlan>,
    pub(super) close_ready: Option<EndOutcome>,
    /// A recovered session: the answers its spool's requests had at recovery.
    pub(super) recovered: Option<HashMap<String, Answer>>,
    pub(super) agents: HashMap<String, AgentWatch>,
    /// A delivered `/compact` waiting for its idle; the instant its boundary record arrived.
    pub(super) compact: Option<Option<Instant>>,
}

pub(super) struct Session {
    pub(super) id: String,
    pub(super) kind: SessionKind,
    pub(super) dir: SessionDir,
    pub(super) phase: Phase,
    pub(super) record: SessionRecord,
    pub(super) record_dirty: bool,
    pub(super) machine: StateMachine,
    pub(super) mapper: Mapper,
    pub(super) name: String,
    pub(super) model: String,
    pub(super) offer: Option<String>,
    pub(super) failure: Option<Failure>,
    /// Overrides the machine's state (a failed teardown is Failed, not Ended).
    pub(super) forced: Option<SessionState>,
    pub(super) shown: Shown,
    pub(super) transcript: Option<PathBuf>,
    pub(super) token_paths: Vec<PathBuf>,
    pub(super) next_identity: Instant,
    pub(super) flow: Flow,
}

impl Session {
    pub(super) fn new(record: SessionRecord, dir: SessionDir, project_root: PathBuf) -> Self {
        Session {
            id: record.id.clone(),
            kind: record.kind.clone(),
            dir,
            phase: Phase::Preparing,
            record_dirty: false,
            machine: StateMachine::new(),
            mapper: Mapper::new(project_root),
            name: String::new(),
            model: String::new(),
            offer: None,
            failure: None,
            forced: None,
            shown: Shown {
                state: SessionState::Starting,
                since: record.started_at.clone(),
                working_since: None,
                view: None,
            },
            transcript: None,
            token_paths: Vec::new(),
            next_identity: Instant::now(),
            flow: Flow::default(),
            record,
        }
    }

    pub(super) fn state(&self) -> SessionState {
        self.forced.unwrap_or_else(|| self.machine.state())
    }

    pub(super) fn socket(&self) -> &str {
        &self.record.socket
    }

    pub(super) fn target(&self) -> &str {
        &self.record.tmux_session
    }

    /// Not yet ended: it counts for the one-chat rule and can still be ended.
    pub(super) fn active(&self) -> bool {
        matches!(
            self.phase,
            Phase::Preparing
                | Phase::Spawning(_)
                | Phase::Launching
                | Phase::Live
                | Phase::TearingDown
        )
    }

    pub(super) fn next_id(&mut self, prefix: &str) -> String {
        self.flow.counter += 1;
        format!("{prefix}{}", self.flow.counter)
    }

    pub(super) fn view(&self) -> SessionView {
        SessionView {
            id: self.id.clone(),
            kind: self.kind.label().to_owned(),
            state: self.state(),
            name: self.name.clone(),
            model: self.model.clone(),
            socket: self.record.socket.clone(),
            holder: None,
            away_until: None,
            queued: u32::try_from(self.flow.queue.len()).unwrap_or(u32::MAX),
            started_at: self.record.started_at.clone(),
            state_since: self.shown.since.clone(),
            working_since: self.shown.working_since.clone(),
            subagents_running: self.machine.subagents_running(),
            last_activity: self.mapper.last_main_record_at().map(stamp),
            failure: self.failure.clone(),
            offer: self.offer.clone(),
        }
    }

    /// Publish a state change as an event and a changed view as a `Session` delta.
    pub(super) fn sync(&mut self, step: &mut Step) {
        let state = self.state();
        if state != self.shown.state {
            let now = stamp(Utc::now());
            let busy = |state: SessionState| {
                matches!(state, SessionState::Working | SessionState::Permission)
            };
            if !busy(state) {
                self.shown.working_since = None;
            } else if !busy(self.shown.state) {
                self.shown.working_since = Some(now.clone());
            }
            self.shown.state = state;
            self.shown.since = now;
            let id = self.id.clone();
            step.event(BridgeEvent::State { id, state });
        }
        if step.chat {
            let view = self.view();
            if self.shown.view.as_ref() != Some(&view) {
                let session = view.clone();
                step.out
                    .push(Out::Delta(Box::new(ChatDelta::Session { session })));
                self.shown.view = Some(view);
            }
        }
    }

    /// Publish mapper deltas, with the bridge's changed files on every turn update.
    pub(super) fn emit(&self, deltas: Vec<ChatDelta>, step: &mut Step) {
        if !step.chat {
            return;
        }
        for mut delta in deltas {
            if let ChatDelta::TurnUpdated {
                turn_id,
                changed_files,
                ..
            } = &mut delta
            {
                if let Some(changed) = self.flow.changed.get(turn_id.as_str()) {
                    changed_files.clone_from(changed);
                }
            }
            step.out.push(Out::Delta(Box::new(delta)));
        }
    }

    pub(super) fn publish_queue(&self, step: &mut Step) {
        if step.chat {
            let queue = self.queue_view(true);
            step.out
                .push(Out::Delta(Box::new(ChatDelta::Queue { queue })));
        }
    }

    pub(super) fn turns(&self) -> Vec<Turn> {
        let mut turns = self.mapper.turns().to_vec();
        for turn in &mut turns {
            if let Some(changed) = self.flow.changed.get(&turn.id) {
                turn.changed_files.clone_from(changed);
            }
        }
        turns
    }

    pub(super) fn queue_view(&self, for_holder: bool) -> Vec<QueuedMessage> {
        let entry = |queued: &Queued| QueuedMessage {
            id: queued.id.clone(),
            first_line: first_line(&queued.user_text),
            text: for_holder.then(|| queued.user_text.clone()),
        };
        self.flow.queue.iter().map(entry).collect()
    }

    /// The chat's view of an ended authoring or pressure session.
    pub(super) fn ended_chat(&self) -> EndedChat {
        EndedChat {
            session: self.view(),
            turns: self.turns(),
        }
    }
}

pub(super) fn stamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// `at + wait`, saturating.
pub(super) fn later(at: DateTime<Utc>, wait: Duration) -> DateTime<Utc> {
    chrono::Duration::from_std(wait)
        .ok()
        .and_then(|wait| at.checked_add_signed(wait))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or("");
    line.chars().take(FIRST_LINE_CHARS).collect()
}
