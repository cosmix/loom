//! One probe: a plan session launched under a copied HOME, the waits bounded by the probe's
//! deadline, and the teardown that leaves nothing running.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use loom::plan_session::bridge::registry::read_record;
use loom::plan_session::bridge::SendOutcome;
use loom::plan_session::kind::SessionDir;
use loom::plan_session::model::{Delivery, Item, Origin, PermissionStatus, SessionState};
use serde_json::Value;
use tempfile::TempDir;

use super::evidence::{lines_of, meta_of, parsed, subagent_files};
use super::home::{copy_home, identity, real_home, user_name, RealHome};
use super::output::Output;
use super::procs::{cmdline, kill_all, kill_server, Seen, Watchdog};
use super::redact::Redactor;
use super::setup::{git_init, probe_request, version_of};
use super::{secs, DEADLINE};
use crate::diagnose::{self, turn_ends, turn_starts};
use crate::{config, project, socket_of, Running};

/// Where the session's spool, transcript and turns stood at one moment.
pub struct Mark {
    pub spool: usize,
    pub transcript: usize,
    pub starts: usize,
    pub pane: String,
    pub at: Instant,
}

pub struct Probe {
    pub id: String,
    pub dir: SessionDir,
    pub home: PathBuf,
    pub base: PathBuf,
    pub root: PathBuf,
    pub out: Output,
    pub pid: u32,
    pub tmux_session: String,
    pub seen: Seen,
    deadline: Instant,
    misses: Vec<String>,
    real: RealHome,
    running: Option<Running>,
    watchdog: Option<Watchdog>,
    _tmp: TempDir,
}

impl Probe {
    /// Copies HOME, lets `prepare(home, base)` add to it, launches the session and waits for
    /// its opening turn to end.
    pub fn open(name: &'static str, prepare: impl FnOnce(&Path, &Path)) -> Probe {
        // Waits stop 10 s before the deadline, so a failed wait still preserves the pane
        // before the watchdog ends the server.
        let end = Instant::now() + DEADLINE;
        let deadline = end - secs(10);
        let claude = loom::claude::find_claude_path().expect("the probes need claude");
        let real = real_home();
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let watched = RealHome::watch(&real, &base);
        let home = copy_home(&real, &base).expect("cannot copy HOME");
        prepare(&home, &base);
        let (root, runtime) = project(&base);
        git_init(&root);
        let redactor = Redactor::new(&real, &base, &user_name(&real), &identity(&real));
        let mut out = Output::new(name, redactor, &version_of(&claude));
        let running = Running::new(config(&root, &runtime, claude, &home, secs(30)));
        running.bridge.recover();
        let id = running.bridge.start_session(probe_request()).unwrap();
        out.session(&id);
        let seen = Seen::default();
        let watchdog = Watchdog::arm(end, socket_of(&id), seen.clone());
        let mut probe = Probe {
            dir: SessionDir::new(runtime.sessions.join(&id)),
            id,
            home,
            base,
            root,
            out,
            pid: 0,
            tmux_session: "plan-authoring".to_owned(),
            seen,
            deadline,
            misses: Vec::new(),
            real: watched,
            running: Some(running),
            watchdog: Some(watchdog),
            _tmp: tmp,
        };
        probe.await_opening();
        probe
    }

    fn await_opening(&mut self) {
        let offer = |item: &Item| {
            matches!(
                item,
                Item::User {
                    origin: Some(Origin::Offer),
                    delivery: Delivery::Delivered,
                    ..
                }
            )
        };
        let arrived = || self.items().iter().any(offer);
        self.until("the opening's delivery", secs(90), arrived);
        self.await_turns(0, "the opening turn", secs(90));
        let record = read_record(&self.dir).ok();
        let pid = record.as_ref().and_then(|record| record.pid);
        self.check(pid.is_some(), "the session record has no pid");
        self.pid = pid.unwrap_or_default();
        self.seen.root.store(self.pid, Ordering::SeqCst);
        if let Some(record) = record {
            self.tmux_session = record.tmux_session;
        }
        let pane = self.pane();
        self.out.note("Pane after the opening turn", &pane);
    }

    pub fn r(&self) -> &Running {
        self.running
            .as_ref()
            .expect("the bridge runs until the probe drops")
    }

    pub fn items(&self) -> Vec<Item> {
        self.r().items()
    }

    pub fn state(&self) -> Option<SessionState> {
        self.r().state(&self.id)
    }

    /// The time left before the probe's deadline.
    pub fn left(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    /// `diagnose::until`, bounded by the deadline.
    pub fn until(&self, what: &str, limit: Duration, ready: impl FnMut() -> bool) {
        let bounded = limit.min(self.left());
        let what = if bounded < limit {
            format!("{what} (cut short by the probe's 180 s deadline)")
        } else {
            what.to_owned()
        };
        diagnose::until(self.r(), &self.id, &self.dir, &what, bounded, ready);
    }

    /// `diagnose::within`, bounded by the deadline.
    pub fn within(&self, limit: Duration, ready: impl FnMut() -> bool) -> bool {
        diagnose::within(limit.min(self.left()), ready)
    }

    /// A failed hard assertion preserves the evidence and panics.
    pub fn check(&self, ok: bool, what: &str) {
        diagnose::check(self.r(), &self.id, &self.dir, ok, what);
    }

    /// Preserves the evidence and fails the probe with `what`.
    pub fn fail(&self, what: &str) -> ! {
        self.check(false, what);
        unreachable!("a failed check panics")
    }

    /// Records missing evidence; `finish` fails the probe with it after writing the fixtures,
    /// so one missing record does not lose the rest of the run.
    pub fn miss(&mut self, what: &str) {
        self.out.note("Missing evidence", what);
        self.misses.push(what.to_owned());
    }

    /// Allows every permission request still pending. `items()` is a copy: the bridge is
    /// called with no lock held.
    pub fn allow_pending(&self) {
        for item in self.items() {
            if let Item::Permission {
                request_id,
                tool,
                status: PermissionStatus::Pending,
                ..
            } = item
            {
                match self
                    .r()
                    .bridge
                    .answer_permission(&self.id, &request_id, true, None)
                {
                    Ok(()) => println!("permission: allowed {tool} ({request_id})"),
                    Err(error) => println!("permission: {tool} not answered: {error}"),
                }
            }
        }
    }

    /// Waits for a turn after the first `starts`, then for every started turn to end and the
    /// state to read Idle; permissions are allowed meanwhile.
    pub fn await_turns(&self, starts: usize, what: &str, limit: Duration) {
        let (r, id) = (self.r(), self.id.as_str());
        let started = || {
            self.allow_pending();
            turn_starts(r, id) > starts
        };
        self.until(&format!("{what}: its start"), limit, started);
        let ended = || {
            self.allow_pending();
            let done = turn_ends(r, id) >= turn_starts(r, id);
            done && self.state() == Some(SessionState::Idle)
        };
        self.until(&format!("{what}: its end"), limit, ended);
    }

    /// Every started turn ended and the state reads Idle, without waiting for a new one.
    pub fn settle(&self, what: &str, limit: Duration) {
        self.await_turns(
            turn_starts(self.r(), &self.id).saturating_sub(1),
            what,
            limit,
        );
    }

    /// Sends `text` through the bridge and notes the pane after its Enter.
    pub fn send(&mut self, text: &str) -> SendOutcome {
        let sent = self.r().bridge.send(&self.id, text.to_owned(), Vec::new());
        let outcome = sent.unwrap_or_else(|error| panic!("cannot send {text:?}: {error}"));
        thread::sleep(Duration::from_millis(1500));
        let pane = self.pane();
        self.out.note(&format!("Pane after Enter: {text}"), &pane);
        outcome
    }

    /// Sends `text`, then waits for its turn and any turn it starts to end.
    pub fn run_turn(&mut self, text: &str, what: &str) {
        let starts = turn_starts(self.r(), &self.id);
        self.send(text);
        self.await_turns(starts, what, secs(150));
    }

    /// The visible pane, trailing blank lines dropped.
    pub fn pane(&self) -> String {
        let socket = socket_of(&self.id);
        let capture = ["capture-pane", "-p", "-J", "-t", &self.tmux_session];
        let out = Command::new("tmux")
            .args(["-L", &socket])
            .args(capture)
            .output();
        match out {
            Ok(out) => String::from_utf8_lossy(&out.stdout).trim_end().to_owned(),
            Err(error) => format!("cannot run tmux: {error}"),
        }
    }

    /// The main transcript: the recorded path, else `<home>/.claude/projects/*/<id>.jsonl`.
    pub fn transcript(&self) -> PathBuf {
        let recorded = read_record(&self.dir).ok().and_then(|r| r.transcript_path);
        recorded.unwrap_or_else(|| {
            let name = format!("{}.jsonl", self.id);
            let projects = self.home.join(".claude/projects");
            let dirs = std::fs::read_dir(&projects).into_iter().flatten();
            let found = dirs.filter_map(Result::ok).map(|e| e.path().join(&name));
            let mut found = found;
            found.find(|p| p.is_file()).unwrap_or(projects.join(name))
        })
    }

    pub fn lines(&self) -> Vec<String> {
        lines_of(&self.transcript())
    }

    pub fn records(&self) -> Vec<Value> {
        parsed(&self.lines())
    }

    /// The spool's entries, each `{received_at, event, session_id, payload, ...}`.
    pub fn spool(&self) -> Vec<Value> {
        parsed(&lines_of(&self.dir.events()))
    }

    /// The raw spool lines after the first `from`.
    pub fn spool_after(&self, from: usize) -> Vec<String> {
        lines_of(&self.dir.events())
            .into_iter()
            .skip(from)
            .collect()
    }

    /// Waits `limit` (bounded by the deadline), allowing permissions meanwhile.
    pub fn linger(&self, limit: Duration) {
        self.within(limit, || {
            self.allow_pending();
            false
        });
    }

    pub fn mark(&self) -> Mark {
        Mark {
            spool: lines_of(&self.dir.events()).len(),
            transcript: self.lines().len(),
            starts: turn_starts(self.r(), &self.id),
            pane: self.pane(),
            at: Instant::now(),
        }
    }

    /// Writes the transcripts, subagent files and spool as fixtures, then fails the probe if
    /// any evidence was missing.
    pub fn finish(mut self) {
        let name = self.out.name();
        let main = self.transcript();
        let lines = lines_of(&main);
        self.out
            .fixture(&format!("transcripts/{name}.jsonl"), &lines);
        for (agent, path) in subagent_files(&main) {
            let n = self.out.agent(&agent);
            let stem = format!("transcripts/{name}-agent-{n}");
            self.out.fixture(&format!("{stem}.jsonl"), &lines_of(&path));
            let meta = lines_of(&meta_of(&path));
            if !meta.is_empty() {
                self.out.fixture(&format!("{stem}.meta.json"), &meta);
            }
        }
        let spool = lines_of(&self.dir.events());
        self.out.fixture(&format!("spool/{name}.jsonl"), &spool);
        let extra = lines_of(&self.base.join(super::setup::EXTRA_HOOKS));
        if !extra.is_empty() {
            self.out
                .fixture(&format!("spool/{name}.extra.jsonl"), &extra);
        }
        let misses = std::mem::take(&mut self.misses);
        self.check(misses.is_empty(), &misses.join("; "));
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        // A probe that failed before its opening turn ended never stored the root pid.
        if self.pid == 0 {
            let pid = read_record(&self.dir).ok().and_then(|record| record.pid);
            self.seen
                .root
                .store(pid.unwrap_or_default(), Ordering::SeqCst);
        }
        self.seen.sample();
        drop(self.running.take());
        kill_server(&socket_of(&self.id));
        let left = kill_all(&self.seen.all());
        drop(self.watchdog.take());
        let mut problems = Vec::new();
        if !left.is_empty() {
            let names: Vec<String> = left.iter().map(|p| cmdline(p.pid)).collect();
            problems.push(format!("still running after SIGKILL: {names:?}"));
        }
        let changed = self.real.changed();
        if !changed.is_empty() {
            problems.push(format!(
                "the real HOME changed: {changed:?} (settings.json edited, or ~/.claude.json \
                 names the probe's temp base)"
            ));
        }
        if problems.is_empty() {
            return;
        }
        let text = problems.join("; ");
        if thread::panicking() {
            eprintln!("probe teardown: {text}");
        } else {
            panic!("probe teardown: {text}");
        }
    }
}
