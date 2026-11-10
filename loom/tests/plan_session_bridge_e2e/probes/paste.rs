//! P2's paste: one slash line sent through the bridge after Idle, and everything that followed
//! it (spool events, recording-hook events, transcript records, the pane around and after
//! Enter), noted raw.

use std::thread;
use std::time::{Duration, Instant};

use loom::plan_session::bridge::SendOutcome;
use loom::plan_session::input::sanitize;
use loom::plan_session::model::{Delivery, Item, SessionState};
use loom::plan_session::tmux_io::{paste_text, press};

use super::evidence::lines_of;
use super::probe::{Mark, Probe};
use super::secs;
use super::setup::EXTRA_HOOKS;
use crate::socket_of;

/// What a paste must leave for the probe to pass.
pub enum Expect {
    /// A new transcript line containing this text.
    Record(&'static str),
    /// Only a pane change: a dialog or a local notice may write no record.
    Pane,
}

pub struct Spec {
    pub text: String,
    pub expect: Expect,
    /// How long records may take to arrive after Enter.
    pub wait: Duration,
    /// Press Escape after the capture, so a dialog the command opened cannot take the next
    /// pasted Enter.
    pub escape: bool,
}

impl Spec {
    /// A command whose transcript must name `needle`.
    pub fn record(text: impl Into<String>, needle: &'static str) -> Spec {
        let (text, expect) = (text.into(), Expect::Record(needle));
        let (wait, escape) = (secs(30), false);
        Spec {
            text,
            expect,
            wait,
            escape,
        }
    }

    /// A local command whose output, if any, arrives within `wait` seconds.
    pub fn local(text: &str, wait: u64) -> Spec {
        let (text, expect) = (text.to_owned(), Expect::Pane);
        let (wait, escape) = (secs(wait), true);
        Spec {
            text,
            expect,
            wait,
            escape,
        }
    }
}

/// What arrived after a mark.
struct After {
    spool: Vec<String>,
    extra: Vec<String>,
    transcript: Vec<String>,
    pane: String,
}

/// Pastes `spec` and notes what followed; returns the transcript lines it added.
pub fn paste(p: &mut Probe, spec: &Spec) -> Vec<String> {
    settle_delivery(p);
    let mark = p.mark();
    let extra = lines_of(&p.base.join(EXTRA_HOOKS)).len();
    let route = deliver(p, &spec.text);
    let frames = frames_around_enter(p);
    let first = first_record(p, &mark, spec);
    if p.within(secs(2), || turn_started(p, &mark)) {
        p.await_turns(mark.starts, "the turn the paste started", secs(120));
    }
    thread::sleep(secs(2));
    let after = observe(p, &mark, extra);
    let escaped = spec.escape.then(|| escape(p));
    let mut body = vec![format!("route: {route}"), delivery_of(p, &spec.text)];
    body.push(match first {
        Some(waited) => format!("first record after the paste: {waited:?}"),
        None => format!("no record within {:?}", spec.wait),
    });
    body.push(format!("pane frames around Enter:\n{}", frames.join("\n")));
    body.extend(sections(&after));
    if let Some(pane) = escaped {
        body.push(format!("pane 1 s after Escape:\n{pane}"));
    }
    let heading: String = spec.text.chars().take(60).collect();
    p.out.note(&format!("Paste: {heading}"), &body.join("\n\n"));
    judge(p, spec, &mark, &after);
    after.transcript
}

fn turn_started(p: &Probe, mark: &Mark) -> bool {
    crate::diagnose::turn_starts(p.r(), &p.id) > mark.starts
}

/// Waits until no user item still waits for its confirmation (the bridge times one out after
/// 10 s), then for Idle.
fn settle_delivery(p: &Probe) {
    let sent = |item: &Item| {
        matches!(
            item,
            Item::User {
                delivery: Delivery::Sent,
                ..
            }
        )
    };
    let confirmed = || !p.items().iter().any(sent);
    p.within(secs(12), confirmed);
    let idle = || p.state() == Some(SessionState::Idle);
    p.until("Idle before the paste", secs(60), idle);
}

/// Sends through the bridge. After an unconfirmed paste its queue waits for the next turn's
/// end, so a send that is only queued is withdrawn and pasted the way the bridge pastes.
fn deliver(p: &Probe, text: &str) -> String {
    let sent = p.r().bridge.send(&p.id, text.to_owned(), Vec::new());
    match sent.unwrap_or_else(|error| panic!("cannot send {text:?}: {error}")) {
        SendOutcome::Delivering { .. } => "the bridge pasted it".to_owned(),
        SendOutcome::Queued { queue_id } => {
            let withdrawn = p.r().bridge.withdraw(&p.id, &queue_id);
            let socket = socket_of(&p.id);
            let pasted = paste_text(&socket, &p.tmux_session, &sanitize(text));
            format!(
                "the bridge queued it (its queue waits after an unconfirmed paste); \
                 withdrawn: {withdrawn:?}; pasted with tmux_io::paste_text: {pasted:?}"
            )
        }
    }
}

/// The pane's last 12 lines each time they change in the 800 ms after the paste; Enter
/// follows the paste by 150 ms, so the first frames show the input before it.
fn frames_around_enter(p: &Probe) -> Vec<String> {
    let start = Instant::now();
    let mut frames: Vec<String> = Vec::new();
    let mut last = String::new();
    while start.elapsed() < Duration::from_millis(800) {
        let pane = p.pane();
        let lines: Vec<&str> = pane.lines().collect();
        let bottom = lines[lines.len().saturating_sub(12)..].join("\n");
        if bottom != last {
            let at = start.elapsed().as_millis();
            frames.push(format!("--- {at} ms after the paste ---\n{bottom}"));
            last = bottom;
        }
        thread::sleep(Duration::from_millis(20));
    }
    frames
}

/// How long after the paste the expected record (`Record`) or any record (`Pane`) arrived.
fn first_record(p: &Probe, mark: &Mark, spec: &Spec) -> Option<Duration> {
    let arrived = || {
        let lines = p.lines();
        let new = lines.get(mark.transcript..).unwrap_or_default();
        match spec.expect {
            Expect::Record(needle) => new.iter().any(|line| line.contains(needle)),
            Expect::Pane => !new.is_empty() || p.spool().len() > mark.spool,
        }
    };
    p.within(spec.wait, arrived).then(|| mark.at.elapsed())
}

fn observe(p: &Probe, mark: &Mark, extra: usize) -> After {
    let after = |lines: Vec<String>, from: usize| lines.into_iter().skip(from).collect();
    After {
        spool: after(lines_of(&p.dir.events()), mark.spool),
        extra: after(lines_of(&p.base.join(EXTRA_HOOKS)), extra),
        transcript: after(p.lines(), mark.transcript),
        pane: p.pane(),
    }
}

fn escape(p: &Probe) -> String {
    let pressed = press(&socket_of(&p.id), &p.tmux_session, "Escape");
    if let Err(error) = pressed {
        return format!("cannot press Escape: {error}");
    }
    thread::sleep(secs(1));
    p.pane()
}

/// The chat's user item for `text` and its delivery state.
fn delivery_of(p: &Probe, text: &str) -> String {
    let text = sanitize(text);
    let items = p.items();
    let found = items.iter().rev().find_map(|item| match item {
        Item::User {
            text: t, delivery, ..
        } if *t == text => Some(*delivery),
        _ => None,
    });
    format!("the chat's user item: {found:?}")
}

fn sections(after: &After) -> Vec<String> {
    let block = |title: &str, lines: &[String]| {
        let body = if lines.is_empty() {
            "(none)".to_owned()
        } else {
            lines.join("\n")
        };
        format!("{title}:\n{body}")
    };
    vec![
        block("spool entries after the paste", &after.spool),
        block(
            "recording hooks (UserPromptExpansion, PostModelSwitch; not in loom's SPOOL_EVENTS, \
             so the spool cannot hold them)",
            &after.extra,
        ),
        block("transcript records after the paste", &after.transcript),
        format!(
            "pane after Enter (2 s after the last record):\n{}",
            after.pane
        ),
    ]
}

fn judge(p: &mut Probe, spec: &Spec, mark: &Mark, after: &After) {
    let missing = match spec.expect {
        Expect::Record(needle) => {
            let found = after.transcript.iter().any(|line| line.contains(needle));
            (!found).then(|| format!("{}: no transcript record naming {needle}", spec.text))
        }
        Expect::Pane => {
            let quiet = after.transcript.is_empty() && after.spool.is_empty();
            let unchanged = quiet && after.pane == mark.pane;
            unchanged.then(|| format!("{}: neither a record nor a pane change", spec.text))
        }
    };
    if let Some(what) = missing {
        p.miss(&what);
    }
}
