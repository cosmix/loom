//! The bridge: owns the live planning sessions, their state machines, message queues, permission requests, endings and recovery.
//!
//! Concurrency: one `Mutex<Inner>` holds every session. Commands and the driver change it in
//! short steps and never call the host, the turn pins or the filesystem while they hold it; a
//! step queues its events and chat deltas in the outbox, and `flush` hands them to the
//! listeners after the lock is released, in order, from one thread at a time. A listener may
//! call back into the bridge: its own flush returns at once and the running one delivers what
//! it queued. A listener that panics is logged and skipped; the others and the flushing
//! thread (the driver, a command, recovery, drain) go on.

mod commands;
mod driver;
mod ending;
mod flow;
pub mod host;
mod recovery;
pub mod registry;
mod requests;
mod session;
mod start;
mod stopping;
mod turns;

#[cfg(test)]
mod fake_host;
#[cfg(test)]
mod harness;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_end;
#[cfg(test)]
mod tests_lifecycle;
#[cfg(test)]
mod tests_permission;
#[cfg(test)]
mod tests_queue;
#[cfg(test)]
mod tests_recovery;

use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub use self::ending::TEARDOWN_FAILED;

use self::commands::Out;
use self::host::SessionHost;
use self::registry::{transcripts_of, ChildRecord, Feeds};
use self::session::Inner;
use super::input::InputRefusal;
use super::kind::{SessionDir, SessionKind};
use super::model::{ChangedFile, ChatDelta, ChatSnapshot, Origin, SessionState};
use crate::planning::runtime::RuntimePaths;

/// What a session is launched with (plan 8 supplies it through the guidance seam).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchGuidance {
    pub brief_path: PathBuf,
    pub plugin_dir: PathBuf,
    /// Pasted after the first `SessionStart`, never on the launch line (px-security-04).
    pub opening_message: String,
    pub model: String,
    pub effort: String,
    pub display_name: String,
    /// Pass the opening as the launch line's prompt instead of pasting it; accepted only for
    /// a command and a plan path (px-seams-R04).
    pub opening_on_argv: bool,
}

/// Writes the session's guidance files into its directory and describes the launch.
pub type GuidanceFn = Box<dyn FnOnce(&SessionDir) -> anyhow::Result<LaunchGuidance> + Send>;

/// A request to start one session.
pub struct StartRequest {
    pub kind: SessionKind,
    pub guidance: GuidanceFn,
    /// The origin of the opening's user item.
    pub opening_origin: Option<Origin>,
    /// The `Offer::id` the chat started from.
    pub offer: Option<String>,
    pub extra_env: Vec<(String, String)>,
}

/// The bridge's paths, programs and timings.
#[derive(Clone)]
pub struct BridgeConfig {
    pub project_root: PathBuf,
    pub runtime: RuntimePaths,
    pub loom_bin: PathBuf,
    pub claude_bin: PathBuf,
    pub home: PathBuf,
    /// Called at the moment the closing message is delivered.
    pub closing_message: Arc<dyn Fn() -> String + Send + Sync>,
    /// How long a pasted message waits for its `UserPromptSubmit` (10 s).
    pub delivery_timeout: Duration,
    /// How long a final write may take from the end request (600 s).
    pub final_write_bound: Duration,
    /// When an unanswered permission request is denied (3590 s).
    pub permission_deny_after: Duration,
    /// How often a session's process identity is checked (2 s).
    pub identity_interval: Duration,
    /// The driver's period (200 ms).
    pub tick: Duration,
}

/// What subscribers hear about sessions.
#[derive(Debug, Clone, PartialEq)]
pub enum BridgeEvent {
    Started {
        id: String,
        kind: SessionKind,
    },
    State {
        id: String,
        state: SessionState,
    },
    TurnStarted {
        id: String,
        turn_id: String,
    },
    TurnEnded {
        id: String,
        turn_id: String,
        changed: Vec<ChangedFile>,
    },
    SubagentStopped {
        id: String,
        agent_id: String,
    },
    Ended {
        id: String,
        kind: SessionKind,
        outcome: EndOutcome,
    },
}

/// How a session ended. A `Marker` end keeps the session's children and its directory until
/// `release_session`; every other end terminates the children and removes the directory,
/// except a failure, which keeps the directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndOutcome {
    Closed,
    Marker,
    TornDown,
    Failed(String),
}

/// How `end` ends a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndMode {
    /// Return the queue, deliver the closing message at the next idle, tear down after the
    /// following Stop, all within `final_write_bound`.
    FinalWrite,
    /// Deny what is pending and tear down now.
    Teardown,
}

/// Who asked for the end. `GraceExpired` and `Draining` deny every pending and new permission
/// request at once; `Holder` and `Caller` keep them answerable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndCause {
    Holder,
    GraceExpired,
    Draining,
    Caller,
}

/// What `send` did with a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    Delivering { item_id: String },
    Queued { queue_id: String },
}

/// Why the bridge refused a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    NoSession,
    NotRunning,
    Busy(&'static str),
    Input(InputRefusal),
    Refused(String),
    Io(String),
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSession => write!(f, "no such session"),
            Self::NotRunning => write!(f, "the chat is closing"),
            Self::Busy(why) => write!(f, "{why}"),
            Self::Input(refusal) => write!(f, "{refusal}"),
            Self::Refused(why) | Self::Io(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for BridgeError {}

/// Turn boundaries recorded against the text store (plan 3); keys are `<session id>/<turn id>`.
pub trait TurnPins: Send + Sync {
    fn begin_turn(&self, turn: &str);
    /// The paths the turn changed, with their hash at the turn's start.
    fn end_turn(&self, turn: &str) -> Vec<(String, Option<String>)>;
}

/// A session recovery ended.
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveredSession {
    pub id: String,
    pub kind: SessionKind,
    pub children: Vec<ChildRecord>,
}

/// What `recover` found in the sessions directory.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecoveryReport {
    pub closed: Vec<RecoveredSession>,
    pub torn_down: Vec<RecoveredSession>,
    /// `(id, socket, why)`; the socket is empty when no record named one.
    pub left_alone: Vec<(String, String, String)>,
}

/// A session's transcripts, for plan 10's cost reader.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transcripts {
    pub main: Option<PathBuf>,
    pub subagents: Vec<(String, PathBuf)>,
}

type Listener<T> = Arc<dyn Fn(&T) + Send + Sync>;

/// The session bridge.
pub struct Bridge {
    config: BridgeConfig,
    host: Arc<dyn SessionHost>,
    pins: Option<Arc<dyn TurnPins>>,
    inner: Mutex<Inner>,
    listeners: Mutex<Vec<Listener<BridgeEvent>>>,
    chat_listeners: Mutex<Vec<Listener<ChatDelta>>>,
    flushing: AtomicBool,
}

impl Bridge {
    pub fn new(
        config: BridgeConfig,
        host: Arc<dyn SessionHost>,
        pins: Option<Arc<dyn TurnPins>>,
    ) -> Arc<Bridge> {
        Arc::new(Bridge {
            config,
            host,
            pins,
            inner: Mutex::new(Inner::new()),
            listeners: Mutex::default(),
            chat_listeners: Mutex::default(),
            flushing: AtomicBool::new(false),
        })
    }

    /// The driver thread: one tick every `config.tick` until `stop` is set.
    pub fn run(self: &Arc<Self>, stop: Arc<AtomicBool>) -> JoinHandle<()> {
        let bridge = Arc::clone(self);
        thread::spawn(move || {
            let mut feeds = Feeds::default();
            while !stop.load(Ordering::SeqCst) {
                if catch_unwind(AssertUnwindSafe(|| bridge.tick(&mut feeds))).is_err() {
                    tracing::error!("a session bridge tick panicked");
                }
                thread::sleep(bridge.config.tick);
            }
        })
    }

    /// Hear every event. A listener must return quickly and never wait on the bridge; one that
    /// panics misses only the event it panicked on.
    pub fn subscribe(&self, listener: Box<dyn Fn(&BridgeEvent) + Send + Sync>) {
        guard(&self.listeners).push(Arc::from(listener));
    }

    /// Hear every delta of the chat session. `Queue` deltas carry the queued texts: strip them
    /// for anyone but the holder.
    pub fn subscribe_chat(&self, listener: Box<dyn Fn(&ChatDelta) + Send + Sync>) {
        guard(&self.chat_listeners).push(Arc::from(listener));
    }

    pub fn chat_snapshot(&self, for_holder: bool) -> ChatSnapshot {
        let inner = self.lock();
        let current = inner.chat.as_ref().and_then(|id| inner.sessions.get(id));
        ChatSnapshot {
            session: current.map(|session| session.view()),
            turns: current.map(|session| session.turns()).unwrap_or_default(),
            queue: current
                .map(|session| session.queue_view(for_holder))
                .unwrap_or_default(),
            previous: inner.previous.clone(),
        }
    }

    /// The authoring or pressure session shown in the chat.
    pub fn chat_session_id(&self) -> Option<String> {
        self.lock().chat.clone()
    }

    /// The chat session is Starting, Working or Permission.
    pub fn turn_running(&self) -> bool {
        let inner = self.lock();
        let chat = inner.chat.as_ref().and_then(|id| inner.sessions.get(id));
        matches!(
            chat.map(|session| session.state()),
            Some(SessionState::Starting | SessionState::Working | SessionState::Permission)
        )
    }

    pub fn transcripts(&self, id: &str) -> Option<Transcripts> {
        let main = {
            let inner = self.lock();
            match inner.sessions.get(id) {
                Some(session) => session.transcript.clone(),
                None => inner.ended.iter().find(|(ended, _)| ended == id)?.1.clone(),
            }
        };
        Some(transcripts_of(main))
    }

    pub fn sessions(&self) -> Vec<(String, SessionKind, SessionState)> {
        let inner = self.lock();
        let sessions = inner.sessions.values();
        sessions
            .map(|session| (session.id.clone(), session.kind.clone(), session.state()))
            .collect()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        guard(&self.inner)
    }

    /// Hand the outbox to the listeners; returns at once while another call is doing it.
    fn flush(&self) {
        loop {
            let claimed =
                self.flushing
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
            if claimed.is_err() {
                return;
            }
            loop {
                let batch = std::mem::take(&mut self.lock().outbox);
                if batch.is_empty() {
                    break;
                }
                self.publish(batch);
            }
            self.flushing.store(false, Ordering::SeqCst);
            // A step that queued while this call was finishing found the flag still set.
            if self.lock().outbox.is_empty() {
                return;
            }
        }
    }

    fn publish(&self, batch: Vec<Out>) {
        let events = guard(&self.listeners).clone();
        let deltas = guard(&self.chat_listeners).clone();
        for out in batch {
            match out {
                Out::Event(event) => events.iter().for_each(|listener| hear(listener, &event)),
                Out::Delta(delta) => deltas.iter().for_each(|listener| hear(listener, &*delta)),
            }
        }
    }
}

/// Call one listener. A panic stays here: unwinding into the flush would drop the rest of the
/// batch for every listener and stop the flushing thread (recovery would never finish).
fn hear<T>(listener: &Listener<T>, value: &T) {
    if catch_unwind(AssertUnwindSafe(|| listener(value))).is_err() {
        tracing::error!("a session bridge listener panicked");
    }
}

/// Lock, recovering the data a panicking holder left: each step leaves it consistent.
fn guard<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
