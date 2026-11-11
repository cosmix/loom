//! Replays a probe fixture (`tests/fixtures/plan_session/`, probes P1 and P2) into a harness
//! session: the spool lines with their recorded `received_at`, the main transcript, and each
//! `<prefix>-agent-<n>.jsonl` with its `.meta.json` as `agent-a<n>` beside the session's
//! transcript, the meta first. `<SESSION-1>` becomes the session's id and `<AGENT-n>` becomes
//! `a<n>`; the fixture's own startup `SessionStart` is skipped, since the harness sent the one
//! naming the session's transcript.
//!
//! Lines go out in the order of their times, each file's times floored to never go back as the
//! bridge's `Batch::in_time_order` floors them (a line without a time, or with one earlier than
//! the line before it, keeps its place after that line), so a test can write a fixture up to a
//! moment, all at once (a replay from offset 0), spool first, or as the live session wrote it.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::background::{BackgroundTask, TaskState};
use super::fake_host::{Harness, Sess};
use super::BridgeEvent;
use crate::plan_session::kind::SessionKind;

/// Where a fixture line goes: the spool, the main transcript, or agent `a<n>`'s transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stream {
    Spool,
    Main,
    Agent(String),
}

struct Line {
    at: DateTime<Utc>,
    stream: Stream,
    text: String,
    written: bool,
}

/// One fixture, ready to write into one session.
pub(super) struct Replay {
    lines: Vec<Line>,
    /// `(agent id, meta file text)`.
    metas: Vec<(String, String)>,
}

impl Replay {
    /// Fixture `prefix` (`p1-status`) for session `s`.
    pub(super) fn load(prefix: &str, s: &Sess) -> Replay {
        let read = |path: PathBuf| real_ids(&fs::read_to_string(path).unwrap(), &s.id);
        let mut lines = Vec::new();
        let spool = read(base().join(format!("spool/{prefix}.jsonl")));
        let spool = spool.lines().filter(|line| !startup(line));
        lines.extend(timed(spool, "received_at", &Stream::Spool));
        let main = read(base().join(format!("transcripts/{prefix}.jsonl")));
        lines.extend(timed(main.lines(), "timestamp", &Stream::Main));
        let mut metas = Vec::new();
        for n in 1..=9 {
            let agent = base().join(format!("transcripts/{prefix}-agent-{n}.jsonl"));
            if !agent.is_file() {
                continue;
            }
            let (id, meta) = (format!("a{n}"), agent.with_extension("meta.json"));
            let stream = Stream::Agent(id.clone());
            lines.extend(timed(read(agent).lines(), "timestamp", &stream));
            if meta.is_file() {
                metas.push((id, read(meta).trim().to_owned()));
            }
        }
        // Stable over floored times: each file keeps its own order.
        lines.sort_by_key(|line| line.at);
        Replay { lines, metas }
    }

    /// Drop every line `drop` matches.
    pub(super) fn without(mut self, drop: impl Fn(&Value) -> bool) -> Replay {
        self.lines.retain(|line| {
            let record: Result<Value, _> = serde_json::from_str(&line.text);
            !record.is_ok_and(|record| drop(&record))
        });
        self
    }

    /// The meta files as a running agent's read, without `finished` (probe P1, "Meta file").
    pub(super) fn unfinished(mut self) -> Replay {
        for (_, meta) in &mut self.metas {
            let mut value: Value = serde_json::from_str(meta).unwrap();
            value.as_object_mut().unwrap().remove("finished");
            *meta = value.to_string();
        }
        self
    }

    /// Every line not yet written up to and including `at`.
    pub(super) fn until(&mut self, s: &Sess, at: &str) {
        let at: DateTime<Utc> = at.parse().unwrap();
        self.write(s, |line| line.at <= at);
    }

    /// Every line not yet written, at once.
    pub(super) fn rest(&mut self, s: &Sess) {
        self.write(s, |_| true);
    }

    /// Every spool line not yet written.
    pub(super) fn spool(&mut self, s: &Sess) {
        self.write(s, |line| line.stream == Stream::Spool);
    }

    /// The whole spool, a pause for the bridge to read it, then the transcripts.
    pub(super) fn spool_first(&mut self, s: &Sess) {
        self.spool(s);
        thread::sleep(Duration::from_millis(300));
        self.rest(s);
    }

    /// Line by line as the session wrote them, letting the bridge read after each spool event
    /// and after each run of transcript lines.
    pub(super) fn live(&mut self, s: &Sess) {
        for at in 0..self.lines.len() {
            self.write_line(s, at);
            let spool = |at: usize| self.lines.get(at).map(|line| &line.stream);
            if spool(at) == Some(&Stream::Spool) || spool(at + 1) == Some(&Stream::Spool) {
                thread::sleep(Duration::from_millis(40));
            }
        }
    }

    fn write(&mut self, s: &Sess, due: impl Fn(&Line) -> bool) {
        for at in 0..self.lines.len() {
            if !self.lines[at].written && due(&self.lines[at]) {
                self.write_line(s, at);
            }
        }
    }

    fn write_line(&mut self, s: &Sess, at: usize) {
        let line = &self.lines[at];
        let path = match &line.stream {
            Stream::Spool => s.dir.events(),
            Stream::Main => s.transcript.clone(),
            Stream::Agent(id) => {
                let dir = subagents(s);
                let meta = dir.join(format!("agent-{id}.meta.json"));
                if !meta.exists() {
                    let text = self.metas.iter().find(|(agent, _)| agent == id);
                    if let Some((_, text)) = text {
                        fs::write(&meta, text).unwrap();
                    }
                }
                dir.join(format!("agent-{id}.jsonl"))
            }
        };
        // One write per line: the bridge never reads half of one.
        let mut file = OpenOptions::new().append(true).create(true).open(path);
        let text = format!("{}\n", line.text);
        file.as_mut().unwrap().write_all(text.as_bytes()).unwrap();
        self.lines[at].written = true;
    }
}

/// A record of fixture `prefix`'s main transcript that `pick` matches, with real ids.
pub(super) fn fixture_record(prefix: &str, s: &Sess, pick: impl Fn(&Value) -> bool) -> Value {
    let path = base().join(format!("transcripts/{prefix}.jsonl"));
    let text = real_ids(&fs::read_to_string(path).unwrap(), &s.id);
    let mut records = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok());
    records.find(pick).expect(prefix)
}

/// `<transcript stem>/subagents`, created.
pub(super) fn subagents(s: &Sess) -> PathBuf {
    let dir = s.transcript.with_extension("").join("subagents");
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn base() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plan_session")
}

fn real_ids(text: &str, session: &str) -> String {
    let mut text = text.replace("<SESSION-1>", session);
    for n in 1..=9 {
        text = text.replace(&format!("<AGENT-{n}>"), &format!("a{n}"));
    }
    text
}

fn startup(line: &str) -> bool {
    let event: Value = serde_json::from_str(line).unwrap();
    event["event"] == "SessionStart" && event["payload"]["source"] == "startup"
}

/// Each line with its time at `key`, floored to the time of the line before it.
fn timed<'a>(lines: impl Iterator<Item = &'a str>, key: &str, stream: &Stream) -> Vec<Line> {
    let mut floor = DateTime::<Utc>::MIN_UTC;
    let line = |text: &str| {
        let value: Option<Value> = serde_json::from_str(text).ok();
        let at = value
            .as_ref()
            .and_then(|v| v.get(key)?.as_str()?.parse().ok());
        floor = at.unwrap_or(floor).max(floor);
        let (stream, text) = (stream.clone(), text.to_owned());
        let written = false;
        Line {
            at: floor,
            stream,
            text,
            written,
        }
    };
    lines
        .filter(|text| !text.trim().is_empty())
        .map(line)
        .collect()
}

/// A ready harness whose stall budget no fixture's age reaches.
pub(super) fn harness() -> Harness {
    let h = Harness::ready(|_| {});
    h.bridge.lock().stall_budget = Duration::from_secs(100 * 365 * 86_400);
    h
}

/// An idle authoring session, ready for a fixture.
pub(super) fn session(h: &Harness) -> Sess {
    h.idle(SessionKind::Authoring)
}

/// The tasks of the last `Background` event of `s`.
pub(super) fn tasks(h: &Harness, s: &Sess) -> Vec<BackgroundTask> {
    let events = h.events.lock().unwrap();
    let last = events.iter().rev().find_map(|event| match event {
        BridgeEvent::Background { id, tasks } if *id == s.id => Some(tasks.clone()),
        _ => None,
    });
    last.unwrap_or_default()
}

pub(super) fn task(h: &Harness, s: &Sess, id: &str) -> Option<BackgroundTask> {
    tasks(h, s).into_iter().find(|task| task.id == id)
}

pub(super) fn state_of(h: &Harness, s: &Sess, id: &str) -> Option<TaskState> {
    task(h, s, id).map(|task| task.state)
}

/// Wait up to 5 s for task `id` of `s` to read `state`; a timeout shows the table.
pub(super) fn wait_task(h: &Harness, s: &Sess, id: &str, state: TaskState) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while state_of(h, s, id) != Some(state) {
        let table = tasks(h, s);
        assert!(
            Instant::now() < deadline,
            "{id} never read {state:?}: {table:#?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

/// The `SubagentStopped` events for `agent`.
pub(super) fn stopped(h: &Harness, agent: &str) -> usize {
    h.count_events(
        |e| matches!(e, BridgeEvent::SubagentStopped { agent_id, .. } if agent_id == agent),
    )
}

/// The `Background` events of `s`.
pub(super) fn published(h: &Harness, s: &Sess) -> usize {
    h.count_events(|e| matches!(e, BridgeEvent::Background { id, .. } if *id == s.id))
}

/// `2026-11-10T07:20:25.055Z` as a time.
pub(super) fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap()
}
