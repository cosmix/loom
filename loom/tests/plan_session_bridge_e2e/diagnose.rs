//! What a failed operator step leaves behind: a report of the session's state, events, chat,
//! spool, transcript and tmux pane, copied under the target directory with the session's files
//! (the temp project and its session directory are deleted when the test ends), and the waits,
//! turn counts and spool reads the operator rounds share.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::PoisonError;
use std::thread;
use std::time::{Duration, Instant};

use loom::plan_session::bridge::registry::read_record;
use loom::plan_session::bridge::BridgeEvent;
use loom::plan_session::kind::SessionDir;
use serde_json::Value;

use super::{socket_of, Running};

/// `text` on one line, cut to `max` chars on a char boundary.
fn cut(text: &str, max: usize) -> String {
    text.replace(['\n', '\r'], " ").chars().take(max).collect()
}

/// Each value's `{:?}` on its own line, cut to 240 chars.
fn lines<T: std::fmt::Debug>(values: &[T]) -> Vec<String> {
    values.iter().map(|v| cut(&format!("{v:?}"), 240)).collect()
}

fn section(out: &mut String, heading: &str, lines: Vec<String>) {
    out.push_str(&format!("== {heading} ==\n{}\n", lines.join("\n")));
}

/// The last `count` lines of the file through `render`, or why it cannot be read.
fn tail(path: &Path, count: usize, render: impl Fn(&str) -> String) -> Vec<String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return vec![format!("cannot read {}: {error}", path.display())],
    };
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    let from = lines.len().saturating_sub(count);
    lines[from..].iter().map(|line| render(line)).collect()
}

/// A record's string content, or the text of its first text block.
fn content_text(record: &Value) -> String {
    let content = record.pointer("/message/content");
    match content.or_else(|| record.get("content")) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => {
            let text = blocks.iter().find(|block| block["type"] == "text");
            match (text, blocks.first()) {
                (Some(block), _) => block["text"].as_str().unwrap_or_default().to_owned(),
                (None, Some(block)) => format!("[{}]", block["type"].as_str().unwrap_or("?")),
                (None, None) => String::new(),
            }
        }
        _ => String::new(),
    }
}

/// A transcript record as `timestamp type subtype operation content`.
fn transcript_line(line: &str) -> String {
    let Ok(record) = serde_json::from_str::<Value>(line) else {
        return format!("unparsed: {}", cut(line, 100));
    };
    let field = |name: &str| record[name].as_str().unwrap_or("-").to_owned();
    let head = ["timestamp", "type", "subtype", "operation"].map(field);
    format!("{} {}", head.join(" "), cut(&content_text(&record), 100))
}

/// `$HOME/.claude/projects/*/<id>.jsonl`, the first match. Claude Code names the transcript
/// after the session id (the launch passes `--session-id`), and the file outlives the
/// session directory.
fn scan_transcript(id: &str) -> Option<PathBuf> {
    let projects = Path::new(&std::env::var_os("HOME")?).join(".claude/projects");
    let name = format!("{id}.jsonl");
    let entries = fs::read_dir(projects).ok()?;
    let candidates = entries.filter_map(Result::ok);
    candidates
        .map(|entry| entry.path().join(&name))
        .find(|path| path.is_file())
}

/// The recorded transcript path, else the file found under the projects directory.
fn transcript_path(dir: &SessionDir, id: &str) -> Result<PathBuf, String> {
    let recorded = read_record(dir)
        .ok()
        .and_then(|record| record.transcript_path);
    let none = || format!("no recorded transcript, and no {id}.jsonl under ~/.claude/projects");
    recorded.or_else(|| scan_transcript(id)).ok_or_else(none)
}

/// The session's tmux pane with 200 lines of scrollback, then tmux's exit status.
fn pane_text(id: &str) -> String {
    let socket = socket_of(id);
    let capture = "capture-pane -p -J -S -200 -t plan-authoring";
    let mut tmux = Command::new("tmux");
    tmux.args(["-L", &socket]).args(capture.split(' '));
    match tmux.output() {
        Ok(out) => {
            let text = [out.stdout, out.stderr].concat();
            format!("{}tmux {}", String::from_utf8_lossy(&text), out.status)
        }
        Err(error) => format!("cannot run tmux: {error}"),
    }
}

/// Every bridge event seen so far. The lock is dropped on return: the bridge calls the
/// subscriber that takes it, so it must not be held while the bridge is queried.
fn event_lines(r: &Running) -> Vec<String> {
    let events = r.events.lock().unwrap_or_else(PoisonError::into_inner);
    lines(&events)
}

/// The session's state, events, chat, queue, spool tail, transcript tail and pane as text.
fn report(r: &Running, id: &str, dir: &SessionDir) -> String {
    report_with_pane(r, id, dir, &pane_text(id))
}

/// `report` over a pane the caller already captured, so one capture serves every file.
fn report_with_pane(r: &Running, id: &str, dir: &SessionDir, pane: &str) -> String {
    let mut out = String::new();
    section(&mut out, "state", lines(&[(id, r.state(id))]));
    section(&mut out, "bridge sessions", lines(&r.bridge.sessions()));
    section(&mut out, "bridge events", event_lines(r));
    section(&mut out, "chat items", lines(&r.items()));
    let queue = r.bridge.chat_snapshot(true).queue;
    section(&mut out, "queue", lines(&queue));
    let spool = tail(&dir.events(), 80, |line| cut(line, 600));
    section(&mut out, "spool, last 80", spool);
    let transcript = match transcript_path(dir, id) {
        Ok(path) => tail(&path, 60, transcript_line),
        Err(error) => vec![error],
    };
    section(&mut out, "transcript, last 60", transcript);
    section(&mut out, "pane", pane.lines().map(str::to_owned).collect());
    out
}

/// `step` as a directory-name fragment: lowercase words joined by `-`.
fn slug(step: &str) -> String {
    let lower = step.to_ascii_lowercase();
    let words = lower.split(|c: char| !c.is_ascii_alphanumeric());
    let words: Vec<&str> = words.filter(|word| !word.is_empty()).collect();
    words.join("-").chars().take(60).collect()
}

/// Copies the spool, the record and the whole transcript into `target` where they exist;
/// returns what failed.
fn copy_evidence(dir: &SessionDir, id: &str, target: &Path) -> Vec<String> {
    let transcript = transcript_path(dir, id).ok();
    let mut sources = vec![(dir.events(), "events.jsonl")];
    sources.push((dir.record(), "session.json"));
    sources.extend(transcript.map(|path| (path, "transcript.jsonl")));
    let mut problems = Vec::new();
    for (from, name) in sources.into_iter().filter(|(from, _)| from.exists()) {
        if let Err(error) = fs::copy(&from, target.join(name)) {
            problems.push(format!("cannot copy {} to {name}: {error}", from.display()));
        }
    }
    problems
}

/// Writes the report, the pane and copies of the spool, record and transcript under
/// `<target tmp>/plan-session-e2e/<unix seconds>-<step>/`; returns that directory.
pub fn preserve(step: &str, r: &Running, id: &str, dir: &SessionDir) -> PathBuf {
    let secs = chrono::Utc::now().timestamp();
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let target = tmp.join(format!("plan-session-e2e/{secs}-{}", slug(step)));
    let pane = pane_text(id);
    let mut text = report_with_pane(r, id, dir, &pane);
    let mut problems = Vec::new();
    if let Err(error) = fs::create_dir_all(&target) {
        problems.push(format!("cannot create {}: {error}", target.display()));
    }
    problems.extend(copy_evidence(dir, id, &target));
    if let Err(error) = fs::write(target.join("pane.txt"), &pane) {
        problems.push(format!("cannot write pane.txt: {error}"));
    }
    if !problems.is_empty() {
        section(&mut text, "preserve problems", problems);
    }
    if let Err(error) = fs::write(target.join("report.txt"), &text) {
        eprintln!("cannot write {}/report.txt: {error}", target.display());
    }
    target
}

/// Preserves the evidence, then panics with `headline`, the directory and the report.
fn fail(r: &Running, id: &str, dir: &SessionDir, step: &str, headline: &str) -> ! {
    let saved = preserve(step, r, id, dir);
    let read = fs::read_to_string(saved.join("report.txt"));
    let text = read.unwrap_or_else(|_| report(r, id, dir));
    panic!("{headline}; diagnostics in {}\n{text}", saved.display());
}

/// Polls `ready` every 100 ms until it holds or `limit` passes; whether it held.
pub fn within(limit: Duration, mut ready: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while !ready() {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(100));
    }
    true
}

/// `within`, but a timeout preserves the evidence under `what` and panics.
pub fn until(
    r: &Running,
    id: &str,
    dir: &SessionDir,
    what: &str,
    limit: Duration,
    ready: impl FnMut() -> bool,
) {
    if !within(limit, ready) {
        let headline = format!("timed out after {limit:?} waiting for {what}");
        fail(r, id, dir, what, &headline);
    }
}

/// Each spooled event's name and payload keys, in order.
pub fn spool_events(dir: &SessionDir) -> Vec<String> {
    let text = fs::read_to_string(dir.events()).unwrap_or_default();
    let lines = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok());
    let describe = |line: Value| {
        let keys = line["payload"]
            .as_object()
            .map(|o| o.keys().cloned().collect());
        let keys: Vec<String> = keys.unwrap_or_default();
        format!("{} {keys:?}", line["event"].as_str().unwrap_or("?"))
    };
    lines.map(describe).collect()
}

/// The events spooled after the first `seen`.
pub fn spooled_after(dir: &SessionDir, seen: usize) -> Vec<String> {
    spool_events(dir).into_iter().skip(seen).collect()
}

/// Which of `Stop` and `StopFailure` the spooled events name.
pub fn stops_in(events: &[String]) -> Vec<&'static str> {
    let names: Vec<&str> = events.iter().filter_map(|l| l.split(' ').next()).collect();
    ["Stop", "StopFailure"]
        .into_iter()
        .filter(|name| names.contains(name))
        .collect()
}

/// How many published bridge events satisfy `keep`. The lock is dropped on return, so the
/// bridge, whose subscriber takes it, is never called while it is held.
fn count_events(r: &Running, keep: impl Fn(&BridgeEvent) -> bool) -> usize {
    let events = r.events.lock().unwrap_or_else(PoisonError::into_inner);
    events.iter().filter(|event| keep(event)).count()
}

/// How many turns of session `id` have started.
pub fn turn_starts(r: &Running, id: &str) -> usize {
    let started = |e: &BridgeEvent| matches!(e, BridgeEvent::TurnStarted { id: s, .. } if s == id);
    count_events(r, started)
}

/// How many turns of session `id` have ended.
pub fn turn_ends(r: &Running, id: &str) -> usize {
    let ended = |e: &BridgeEvent| matches!(e, BridgeEvent::TurnEnded { id: s, .. } if s == id);
    count_events(r, ended)
}

/// Waits until `count` turns of the session have ended in all.
pub fn await_turn_ends(r: &Running, id: &str, dir: &SessionDir, count: usize, limit: Duration) {
    let reached = || turn_ends(r, id) >= count;
    until(r, id, dir, &format!("turn end {count}"), limit, reached);
}

/// A failed session assertion preserves the evidence under `what` and panics with it.
pub fn check(r: &Running, id: &str, dir: &SessionDir, ok: bool, what: &str) {
    if !ok {
        fail(r, id, dir, what, what);
    }
}
