//! A fake session bridge: records every call and runs each started session by a script on a
//! thread of its own, as the bridge's driver would; its events reach the subscribers.

use std::collections::VecDeque;
use std::fs::{self, DirBuilder};
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

use crate::plan_server::pressure::driver::PressureSessions;
use crate::plan_session::bridge::registry::{write_record, SessionRecord, SCHEMA};
use crate::plan_session::bridge::{
    BridgeError, BridgeEvent, EndCause, EndMode, EndOutcome, LaunchGuidance, RecoveryReport,
    StartRequest, Transcripts,
};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::{
    ChangedFile, ChatSnapshot, EndedChat, SessionState, SessionView, TokenCounts, Turn,
};
use crate::process::ProcessIdentity;

/// How a started session behaves.
#[derive(Debug, Clone)]
pub(in crate::plan_server::pressure) enum Script {
    /// Its marker appears after this long.
    Marker(Duration),
    /// It fails with this message.
    Fail(String),
    /// It runs until `end`.
    Hold,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::plan_server::pressure) enum Call {
    Start(String),
    Interrupt(String),
    End(String, EndMode, EndCause),
    RecordChild(String, ProcessIdentity),
    Release(String),
    AddTokens(String, PathBuf),
}

pub(in crate::plan_server::pressure) struct Started {
    pub(in crate::plan_server::pressure) kind: SessionKind,
    pub(in crate::plan_server::pressure) extra_env: Vec<(String, String)>,
    pub(in crate::plan_server::pressure) guidance: LaunchGuidance,
    /// The brief the guidance wrote, read at the start (a release removes the directory).
    pub(in crate::plan_server::pressure) brief: String,
    pub(in crate::plan_server::pressure) dir: SessionDir,
    pub(in crate::plan_server::pressure) at: Instant,
}

type Listener = Arc<dyn Fn(&BridgeEvent) + Send + Sync>;
type Hook = Box<dyn Fn(&str) + Send + Sync>;

/// Every token count a fake turn carries; a step of one turn uses 20 tokens.
pub(in crate::plan_server::pressure) const TURN_TOKENS: TokenCounts = TokenCounts {
    input: 10,
    cache_write: 2,
    cache_read: 3,
    output: 5,
};

pub(in crate::plan_server::pressure) struct FakeSessions {
    me: Weak<FakeSessions>,
    sessions_dir: PathBuf,
    next: AtomicU32,
    pub(in crate::plan_server::pressure) calls: Mutex<Vec<Call>>,
    pub(in crate::plan_server::pressure) started: Mutex<Vec<Started>>,
    /// `(id, outcome, when)` of every `Ended` emitted.
    pub(in crate::plan_server::pressure) ended: Mutex<Vec<(String, EndOutcome, Instant)>>,
    pub(in crate::plan_server::pressure) scripts: Mutex<VecDeque<Script>>,
    listed: Mutex<Vec<(String, SessionKind, SessionState)>>,
    listeners: Mutex<Vec<Listener>>,
    snapshot: Mutex<ChatSnapshot>,
    /// Returned by `end` of an authoring chat.
    pub(in crate::plan_server::pressure) end_texts: Mutex<Vec<String>>,
    /// How long an `end` takes to produce its `Ended`.
    pub(in crate::plan_server::pressure) end_delay: Mutex<Duration>,
    pub(in crate::plan_server::pressure) transcripts: Mutex<Option<Transcripts>>,
    pub(in crate::plan_server::pressure) recovery: Mutex<RecoveryReport>,
    /// Run inside `end`, with the session id.
    pub(in crate::plan_server::pressure) on_end: Mutex<Option<Hook>>,
    /// Run after each `Ended` was delivered, with the session id.
    pub(in crate::plan_server::pressure) after_ended: Mutex<Option<Hook>>,
    /// Run inside `release_session`, before the directory goes, with the session id.
    pub(in crate::plan_server::pressure) on_release: Mutex<Option<Hook>>,
}

impl FakeSessions {
    pub(in crate::plan_server::pressure) fn new(sessions_dir: PathBuf) -> Arc<FakeSessions> {
        Arc::new_cyclic(|me| FakeSessions {
            me: me.clone(),
            sessions_dir,
            next: AtomicU32::new(0),
            calls: Mutex::default(),
            started: Mutex::default(),
            ended: Mutex::default(),
            scripts: Mutex::default(),
            listed: Mutex::default(),
            listeners: Mutex::default(),
            snapshot: Mutex::new(empty_snapshot()),
            end_texts: Mutex::default(),
            end_delay: Mutex::new(Duration::from_millis(30)),
            transcripts: Mutex::default(),
            recovery: Mutex::default(),
            on_end: Mutex::default(),
            after_ended: Mutex::default(),
            on_release: Mutex::default(),
        })
    }

    /// The scripts of the next sessions, in start order (later starts finish by marker).
    pub(in crate::plan_server::pressure) fn script(&self, scripts: Vec<Script>) {
        self.scripts.lock().unwrap().extend(scripts);
    }

    /// A session the bridge already runs (an open authoring chat).
    pub(in crate::plan_server::pressure) fn list(&self, id: &str, kind: SessionKind) {
        let entry = (id.to_owned(), kind, SessionState::Idle);
        self.listed.lock().unwrap().push(entry);
    }

    pub(in crate::plan_server::pressure) fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    /// The child records of session `id`, in order.
    pub(in crate::plan_server::pressure) fn children(&self, id: &str) -> Vec<ProcessIdentity> {
        let calls = self.calls();
        let children = calls.into_iter().filter_map(|call| match call {
            Call::RecordChild(of, child) if of == id => Some(child),
            _ => None,
        });
        children.collect()
    }

    pub(in crate::plan_server::pressure) fn ended_with(&self, id: &str) -> Option<EndOutcome> {
        let ended = self.ended.lock().unwrap();
        let found = ended.iter().find(|(of, ..)| of == id);
        found.map(|(_, outcome, _)| outcome.clone())
    }

    fn record(&self, call: Call) {
        self.calls.lock().unwrap().push(call);
    }

    fn active(&self, id: &str) -> Option<SessionKind> {
        let listed = self.listed.lock().unwrap();
        let found = listed
            .iter()
            .find(|(of, _, state)| of == id && *state != SessionState::Ended);
        found.map(|(_, kind, _)| kind.clone())
    }

    /// End session `id` once: the snapshot names it as the previous chat, then `Ended`.
    fn finish(&self, id: &str, outcome: EndOutcome) {
        let kind = {
            let mut listed = self.listed.lock().unwrap();
            let entry = listed
                .iter_mut()
                .find(|(of, _, state)| of == id && *state != SessionState::Ended);
            let Some(entry) = entry else {
                return;
            };
            entry.2 = SessionState::Ended;
            entry.1.clone()
        };
        let ended = EndedChat {
            session: view(id),
            turns: vec![turn()],
        };
        *self.snapshot.lock().unwrap() = ChatSnapshot {
            previous: Some(ended),
            ..empty_snapshot()
        };
        let when = Instant::now();
        self.ended
            .lock()
            .unwrap()
            .push((id.to_owned(), outcome.clone(), when));
        let event = BridgeEvent::Ended {
            id: id.to_owned(),
            kind,
            outcome,
        };
        let listeners = self.listeners.lock().unwrap().clone();
        listeners.iter().for_each(|listener| listener(&event));
        if let Some(hook) = self.after_ended.lock().unwrap().as_ref() {
            hook(id);
        }
    }

    /// Run `f` on a thread of its own after `delay`, as the bridge's driver would.
    fn later(&self, delay: Duration, f: impl FnOnce(&FakeSessions) + Send + 'static) {
        let me = self.me.upgrade().expect("the fake outlives its sessions");
        thread::spawn(move || {
            thread::sleep(delay);
            f(&me);
        });
    }

    fn reserve(&self, kind: &SessionKind) -> (String, SessionDir) {
        let n = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let id = format!("s-{n}");
        let dir = SessionDir::new(self.sessions_dir.join(&id));
        DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(dir.answers())
            .unwrap();
        let record = SessionRecord {
            schema: SCHEMA,
            id: id.clone(),
            kind: kind.clone(),
            socket: format!("fake-socket-{n}"),
            tmux_session: "plan-pressure".into(),
            pid: None,
            start_time: None,
            transcript_path: None,
            started_at: "2026-10-29T00:00:00.000Z".into(),
            feature: None,
            plan: None,
            children: Vec::new(),
        };
        write_record(&dir, &record).unwrap();
        (id, dir)
    }
}

impl PressureSessions for FakeSessions {
    fn start_session(&self, request: StartRequest) -> Result<String, BridgeError> {
        let (id, dir) = self.reserve(&request.kind);
        let guidance =
            (request.guidance)(&dir).map_err(|e| BridgeError::Refused(format!("{e:#}")))?;
        let brief = fs::read_to_string(&guidance.brief_path).unwrap_or_default();
        let entry = (id.clone(), request.kind.clone(), SessionState::Working);
        self.listed.lock().unwrap().push(entry);
        self.record(Call::Start(id.clone()));
        self.started.lock().unwrap().push(Started {
            kind: request.kind,
            extra_env: request.extra_env,
            guidance,
            brief,
            dir,
            at: Instant::now(),
        });
        let script = self.scripts.lock().unwrap().pop_front();
        let ended = id.clone();
        match script.unwrap_or_else(|| Script::Marker(Duration::from_millis(150))) {
            Script::Marker(after) => {
                self.later(after, move |fake| fake.finish(&ended, EndOutcome::Marker))
            }
            Script::Fail(why) => self.later(Duration::from_millis(100), move |fake| {
                fake.finish(&ended, EndOutcome::Failed(why));
            }),
            Script::Hold => {}
        }
        Ok(id)
    }

    fn subscribe(&self, listener: Box<dyn Fn(&BridgeEvent) + Send + Sync>) {
        self.listeners.lock().unwrap().push(Arc::from(listener));
    }

    fn interrupt(&self, id: &str) -> Result<Vec<String>, BridgeError> {
        self.record(Call::Interrupt(id.to_owned()));
        Ok(Vec::new())
    }

    fn end(&self, id: &str, mode: EndMode, cause: EndCause) -> Result<Vec<String>, BridgeError> {
        self.record(Call::End(id.to_owned(), mode, cause));
        let kind = self.active(id).ok_or(BridgeError::NotRunning)?;
        if let Some(hook) = self.on_end.lock().unwrap().as_ref() {
            hook(id);
        }
        let (ended, delay) = (id.to_owned(), *self.end_delay.lock().unwrap());
        let outcome = match kind {
            SessionKind::Authoring => EndOutcome::Closed,
            _ => EndOutcome::TornDown,
        };
        self.later(delay, move |fake| fake.finish(&ended, outcome));
        match kind {
            SessionKind::Authoring => Ok(std::mem::take(&mut *self.end_texts.lock().unwrap())),
            _ => Ok(Vec::new()),
        }
    }

    fn record_child(
        &self,
        id: &str,
        _label: &str,
        child: ProcessIdentity,
    ) -> Result<(), BridgeError> {
        self.record(Call::RecordChild(id.to_owned(), child));
        Ok(())
    }

    fn sessions(&self) -> Vec<(String, SessionKind, SessionState)> {
        self.listed.lock().unwrap().clone()
    }

    fn chat_snapshot(&self, _for_holder: bool) -> ChatSnapshot {
        self.snapshot.lock().unwrap().clone()
    }

    fn release_session(&self, id: &str) -> Result<(), BridgeError> {
        self.record(Call::Release(id.to_owned()));
        if let Some(hook) = self.on_release.lock().unwrap().as_ref() {
            hook(id);
        }
        let _ = fs::remove_dir_all(self.sessions_dir.join(id));
        Ok(())
    }

    fn add_token_transcript(&self, id: &str, path: PathBuf) -> Result<(), BridgeError> {
        self.record(Call::AddTokens(id.to_owned(), path));
        Ok(())
    }

    fn transcripts(&self, _id: &str) -> Option<Transcripts> {
        self.transcripts.lock().unwrap().clone()
    }

    fn on_recovered(&self, f: Box<dyn FnOnce(&RecoveryReport) + Send>) {
        let report = self.recovery.lock().unwrap().clone();
        f(&report);
    }
}

fn empty_snapshot() -> ChatSnapshot {
    ChatSnapshot {
        session: None,
        turns: Vec::new(),
        queue: Vec::new(),
        previous: None,
    }
}

fn view(id: &str) -> SessionView {
    SessionView {
        id: id.to_owned(),
        kind: "pressure".into(),
        state: SessionState::Ended,
        name: "pressure".into(),
        model: "opus".into(),
        socket: String::new(),
        holder: None,
        away_until: None,
        queued: 0,
        started_at: String::new(),
        state_since: String::new(),
        working_since: None,
        subagents_running: 0,
        last_activity: None,
        failure: None,
        offer: None,
        feature: None,
        plan: None,
        fetch_grant: false,
    }
}

fn turn() -> Turn {
    Turn {
        id: "t1".into(),
        started_at: String::new(),
        duration_ms: Some(1),
        open: false,
        tokens: TURN_TOKENS,
        items: Vec::new(),
        changed_files: vec![ChangedFile {
            path: "doc/plans/PLAN-a.md".into(),
            before_hash: None,
            by_tool: true,
        }],
        summaries_rebuilding: Vec::new(),
    }
}
