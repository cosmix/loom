//! One session's bookkeeping behind the bridge's lock: its record, state machine, mapper,
//! queue, permission requests and timers. `publish.rs` holds how a step publishes it.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;

use super::commands::Out;
use super::host::LaunchSpec;
use super::registry::SessionRecord;
use super::{team::TeamTracker, EndMode, EndOutcome, RecoveryReport};
use crate::plan_session::attachments::Attachment;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::{
    ChangedFile, Delivery, DenyReason, EndedChat, Failure, Item, Origin, SessionState, SessionView,
};
use crate::plan_session::permission::Answer;
use crate::plan_session::state::StateMachine;
use crate::plan_session::transcript::Mapper;

/// A finished subagent transcript that has not grown for this long counts as stopped.
const SUBAGENT_STALL: Duration = Duration::from_secs(30);
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
    /// The typed part (attachment block included), sanitised: `text` ends with it after a blank
    /// line, and the delivery shows the guidance before it and the typed text as two items.
    pub(super) typed: Option<String>,
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
    /// Every sent item of the delivery: one, or the guidance and the typed text of an opening.
    pub(super) item_ids: Vec<String>,
    pub(super) text: String,
    /// The sanitised user text a transcript record of this delivery carries.
    pub(super) match_text: String,
    /// `None` until the driver pasted it.
    pub(super) sent_at: Option<Instant>,
    pub(super) closing: bool,
    /// `Some(n)`: the opening, Enter pressed again `n` times. Its failed paste fails the session.
    pub(super) opening: Option<u8>,
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
    /// `(item ids, match text, closing)` of deliveries that timed out, latest last.
    pub(super) undelivered: Vec<(Vec<String>, String, bool)>,
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
    pub(super) team: Option<TeamTracker>,
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
            team: None,
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

    /// Whether the user sent this session a message of their own: a user item with no
    /// generated origin that is not `Delivery::Undelivered` (`Sent` is set when the item is
    /// created, before the paste reaches the terminal). An offer chat the user never wrote in has
    /// only the offer's opening, so it has nothing to write into the documents at its end.
    pub(super) fn user_wrote(&self) -> bool {
        self.mapper
            .turns()
            .iter()
            .flat_map(|turn| &turn.items)
            .any(|item| {
                matches!(
                    item,
                    Item::User { origin: None, delivery, .. } if *delivery != Delivery::Undelivered
                )
            })
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
