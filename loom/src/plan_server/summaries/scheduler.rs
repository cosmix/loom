//! The pure rebuild scheduler: which summaries rebuild when, one summary session at a time
//! (section 04 "Who rebuilds what", "Asking before a large rebuild").
//!
//! No I/O, threads or clock reads: every input is a call with `now` passed in, and sessions start
//! and stop only through a [`SessionSpawner`]. Every call appends its events to an outbox:
//! [`Scheduler::on_update`] and [`Scheduler::tick`] return it drained, the other calls leave it
//! for [`Scheduler::take_events`].

mod queue;
mod session;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_session;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::{Duration, Instant};

/// Summaries rebuilt without asking.
pub const AUTO_LIMIT: usize = 5;
/// How long a summary session may go without a seal before it is stopped.
pub const NO_SEAL_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// The scheduler's fixed inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerConfig {
    /// Why the server is read-only; `Some` starts and asks nothing.
    pub read_only: Option<String>,
    /// Queued summaries started without asking.
    pub limit: usize,
    /// Time without a seal after which a session is stopped.
    pub no_seal_timeout: Duration,
}

impl SchedulerConfig {
    /// The planning server's rules: [`AUTO_LIMIT`] and [`NO_SEAL_TIMEOUT`].
    pub fn standard(read_only: Option<String>) -> Self {
        SchedulerConfig {
            read_only,
            limit: AUTO_LIMIT,
            no_seal_timeout: NO_SEAL_TIMEOUT,
        }
    }
}

/// Starts and stops summary sessions for the scheduler.
pub trait SessionSpawner: Send + Sync {
    /// Start one summary session rebuilding `ids`; returns its session id.
    fn start_summary(&self, ids: &[String]) -> anyhow::Result<String>;
    /// End `session`; with `release_claims`, also clear the claims it holds.
    fn stop(&self, session: &str, release_claims: bool);
}

/// The open question: the queued summaries waiting for approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionView {
    pub id: String,
    pub ids: Vec<String>,
}

/// Why a summary session stopped or ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// Every id it took was sealed or released.
    ClaimsDone,
    /// No seal within the timeout.
    Stalled,
    /// The session ended on its own.
    Ended,
    /// The session ended with this failure.
    Failed(String),
    /// The server is stopping; the session bridge's drain ends the session.
    ServerStopping,
}

/// What a call changed, for the facade to broadcast and print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerEvent {
    Started {
        session: String,
        ids: Vec<String>,
    },
    /// A question opened, or its ids changed.
    Asked(QuestionView),
    QuestionClosed {
        id: String,
    },
    Stopped {
        session: String,
        reason: StopReason,
    },
    Failed {
        ids: Vec<String>,
        error: String,
    },
}

/// Who answers a question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerRight {
    /// The browser holding authoring.
    Holder,
    /// Any browser while nobody holds authoring.
    FreeAuthoring,
    /// A browser while another holds authoring.
    NotAllowed,
}

/// Why nothing starts for a while.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendReason {
    /// An authoring turn is open.
    AuthoringTurn,
    /// A pressure run is active.
    PressureRun,
}

/// Why an answer was refused; the question stays open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerRefused {
    NotAllowed,
    NoSuchQuestion,
}

impl fmt::Display for AnswerRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnswerRefused::NotAllowed => {
                write!(
                    f,
                    "only the authoring holder answers while someone holds it"
                )
            }
            AnswerRefused::NoSuchQuestion => write!(f, "no such question is open"),
        }
    }
}

impl std::error::Error for AnswerRefused {}

/// Why concept mode marks a summary out of date, from the scheduler's side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebuildReason {
    /// In the open question, waiting for approval.
    AwaitingApproval,
    /// Its last rebuild failed with this error.
    Failed(String),
}

/// A copy of the scheduler's state for the panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchedulerView {
    /// The running session and its ids.
    pub running: Option<(String, Vec<String>)>,
    /// The ids of the running session sealed so far.
    pub sealed: Vec<String>,
    /// Waiting ids, Button and approved ids first, then arrival order.
    pub queue: Vec<String>,
    pub declined: Vec<String>,
    /// `(id, error)` of the summaries whose rebuild failed.
    pub failed: Vec<(String, String)>,
    pub question: Option<QuestionView>,
    pub read_only: Option<String>,
    /// Claims made from a terminal, offered for release.
    pub terminal_claims: Vec<String>,
    /// The last failed start, cleared by the next start.
    pub start_error: Option<String>,
}

/// How an id joined the queue: a Button or an approved question skips the limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Via {
    Batch,
    Button,
    Approved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Queued {
    id: String,
    via: Via,
}

#[derive(Debug)]
struct Running {
    session: String,
    ids: Vec<String>,
    vias: BTreeMap<String, Via>,
    /// The start or the last seal.
    since: Instant,
    sealed: BTreeSet<String>,
    /// Every id seen claimed by the session.
    claimed: BTreeSet<String>,
    /// A stop was asked for; the session runs until its end arrives.
    stopping: bool,
    /// The failure a stall reports, from the session's last failed seal.
    seal_error: Option<String>,
}

impl Running {
    fn pending(&self, id: &str) -> bool {
        self.ids.iter().any(|own| own == id) && !self.sealed.contains(id)
    }
}

/// The rebuild scheduler.
#[derive(Debug)]
pub struct Scheduler {
    config: SchedulerConfig,
    queue: Vec<Queued>,
    declined: BTreeSet<String>,
    failed: BTreeMap<String, String>,
    /// Ids a normal end left claimed but unsealed and requeued; a second such end fails them.
    unsealed: BTreeSet<String>,
    question: Option<QuestionView>,
    running: Option<Running>,
    suspensions: BTreeMap<u64, SuspendReason>,
    next_token: u64,
    /// The suspension of the open authoring turn.
    turn: Option<u64>,
    terminal_claims: Vec<String>,
    start_error: Option<String>,
    stopping: bool,
    asked: u64,
    out: Vec<SchedulerEvent>,
}

impl Scheduler {
    pub fn new(config: SchedulerConfig) -> Scheduler {
        Scheduler {
            config,
            queue: Vec::new(),
            declined: BTreeSet::new(),
            failed: BTreeMap::new(),
            unsealed: BTreeSet::new(),
            question: None,
            running: None,
            suspensions: BTreeMap::new(),
            next_token: 0,
            turn: None,
            terminal_claims: Vec::new(),
            start_error: None,
            stopping: false,
            asked: 0,
            out: Vec::new(),
        }
    }

    /// A watcher batch: queue `invalidated` minus the declined, the session's pending and the
    /// queued ids, after dropping every id no longer out of date; then try a start.
    pub fn on_update(
        &mut self,
        invalidated: &[String],
        out_of_date: &[String],
        now: Instant,
        spawner: &dyn SessionSpawner,
    ) -> Vec<SchedulerEvent> {
        self.prune(out_of_date);
        for id in invalidated {
            if self.declined.contains(id) || self.in_session(id) || self.is_queued(id) {
                continue;
            }
            self.failed.remove(id);
            self.push_batch(id);
        }
        self.try_start(now, spawner);
        self.take_events()
    }

    /// An authoring turn opened: hold an [`SuspendReason::AuthoringTurn`] suspension.
    pub fn turn_started(&mut self) {
        if self.turn.is_none() {
            self.turn = Some(self.suspend(SuspendReason::AuthoringTurn));
        }
    }

    /// The authoring turn ended: release its suspension, queue every id still out of date; a
    /// call without an open turn (a late `TurnEnded`) changes nothing.
    pub fn turn_ended(
        &mut self,
        out_of_date: &[String],
        now: Instant,
        spawner: &dyn SessionSpawner,
    ) {
        if let Some(token) = self.turn.take() {
            self.resume(token, out_of_date, now, spawner);
        }
    }

    /// The authoring session went Idle or Ended with its turn still open (Claude Code may write
    /// no Stop after an interrupt): end the turn as [`Scheduler::turn_ended`] would. The facade
    /// only reports the authoring session.
    pub fn authoring_state_idle(
        &mut self,
        _session: &str,
        out_of_date: &[String],
        now: Instant,
        spawner: &dyn SessionSpawner,
    ) {
        self.turn_ended(out_of_date, now, spawner);
    }

    /// Start nothing until the returned token is resumed.
    pub fn suspend(&mut self, reason: SuspendReason) -> u64 {
        self.next_token += 1;
        self.suspensions.insert(self.next_token, reason);
        self.next_token
    }

    /// Release a suspension: queue every id still out of date (minus the declined and failed)
    /// and try a start, which waits while another suspension lives. Unknown tokens are ignored.
    pub fn resume(
        &mut self,
        token: u64,
        out_of_date: &[String],
        now: Instant,
        spawner: &dyn SessionSpawner,
    ) {
        if self.suspensions.remove(&token).is_none() {
            return;
        }
        self.refresh(out_of_date);
        self.try_start(now, spawner);
    }

    /// Answer the open question: yes starts its ids without a second check, no declines them.
    pub fn answer(
        &mut self,
        question: &str,
        rebuild: bool,
        right: AnswerRight,
        now: Instant,
        spawner: &dyn SessionSpawner,
    ) -> Result<(), AnswerRefused> {
        if right == AnswerRight::NotAllowed {
            return Err(AnswerRefused::NotAllowed);
        }
        let Some(open) = self.question.take_if(|open| open.id == question) else {
            return Err(AnswerRefused::NoSuchQuestion);
        };
        self.out
            .push(SchedulerEvent::QuestionClosed { id: open.id });
        if rebuild {
            self.promote(&open.ids, Via::Approved);
        } else {
            self.queue.retain(|queued| !open.ids.contains(&queued.id));
            self.declined.extend(open.ids);
        }
        self.try_start(now, spawner);
        Ok(())
    }

    /// A rebuild button: undecline the ids, queue them first and start without asking.
    pub fn press(&mut self, ids: &[String], now: Instant, spawner: &dyn SessionSpawner) {
        self.forget(ids);
        self.promote(ids, Via::Button);
        self.try_start(now, spawner);
    }

    /// Claims made from a terminal, offered for release in the panel.
    pub fn set_terminal_claims(&mut self, ids: Vec<String>) {
        self.terminal_claims = ids;
    }

    /// The events of the calls since the last drain.
    pub fn take_events(&mut self) -> Vec<SchedulerEvent> {
        std::mem::take(&mut self.out)
    }

    pub fn panel_state(&self) -> SchedulerView {
        let running = self.running.as_ref();
        SchedulerView {
            running: running.map(|r| (r.session.clone(), r.ids.clone())),
            sealed: running
                .map(|r| r.sealed.iter().cloned().collect())
                .unwrap_or_default(),
            queue: self.queue.iter().map(|queued| queued.id.clone()).collect(),
            declined: self.declined.iter().cloned().collect(),
            failed: self.failed.clone().into_iter().collect(),
            question: self.question.clone(),
            read_only: self.config.read_only.clone(),
            terminal_claims: self.terminal_claims.clone(),
            start_error: self.start_error.clone(),
        }
    }

    /// Why concept mode marks `id` out of date, when the scheduler knows.
    pub fn reason(&self, id: &str) -> Option<RebuildReason> {
        let asked = self.question.as_ref();
        if asked.is_some_and(|open| open.ids.iter().any(|asked| asked == id)) {
            return Some(RebuildReason::AwaitingApproval);
        }
        self.failed.get(id).cloned().map(RebuildReason::Failed)
    }
}
