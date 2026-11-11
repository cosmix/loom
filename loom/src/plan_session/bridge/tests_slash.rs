//! Bridge tests: slash deliveries per class over the P2 fixtures (plan step B4), each fixture
//! written into a harness session up to the moment a check is about.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::fake_host::{settle, wait_until, Harness, Sess};
use super::fixture_replay::{harness, session, Replay};
use super::{BridgeEvent, SendOutcome};
use crate::plan_session::commands::CommandTable;
use crate::plan_session::kind::SessionKind::Authoring;
use crate::plan_session::model::{ChatDelta, CommandRef, Delivery, Item, NoticeKind, SessionState};

const INITIALIZE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/plan_session/probe-notes/p2-side-initialize.side-loom-ui-disabled.jsonl"
));

/// The rows of the recorded `initialize` answer.
pub(super) fn initialize_rows() -> Vec<Value> {
    let first = INITIALIZE.lines().next().unwrap();
    let answer: Value = serde_json::from_str(first).unwrap();
    answer["response"]["response"]["commands"]
        .as_array()
        .unwrap()
        .clone()
}

/// Swap `table` into the session's cell, as the catalogue owner does.
pub(super) fn set_table(h: &Harness, s: &Sess, table: CommandTable) {
    let cell = h.bridge.command_cell(&s.id).unwrap();
    *cell.write().unwrap() = Arc::new(table);
}

/// The recorded list, plus `extra` rows.
pub(super) fn listed(h: &Harness, s: &Sess, extra: &[Value]) {
    let mut rows = initialize_rows();
    rows.extend_from_slice(extra);
    set_table(h, s, CommandTable::from_initialize(&rows, 1));
}

pub(super) fn send(h: &Harness, s: &Sess, text: &str) -> SendOutcome {
    h.bridge.send(&s.id, text.to_owned(), Vec::new()).unwrap()
}

/// Delivery and chip of every user item showing `text`.
pub(super) fn users(h: &Harness, text: &str) -> Vec<(Delivery, Option<CommandRef>)> {
    let turns = h.bridge.chat_snapshot(true).turns;
    let items = turns.into_iter().flat_map(|turn| turn.items);
    let users = items.filter_map(|item| match item {
        Item::User {
            text: shown,
            delivery,
            command,
            ..
        } if shown == text => Some((delivery, command)),
        _ => None,
    });
    users.collect()
}

pub(super) fn delivered(h: &Harness, text: &str) -> bool {
    users(h, text)
        .first()
        .is_some_and(|(delivery, _)| *delivery == Delivery::Delivered)
}

pub(super) fn info_notices(h: &Harness) -> Vec<String> {
    let turns = h.bridge.chat_snapshot(true).turns;
    let items = turns.into_iter().flat_map(|turn| turn.items);
    let notices = items.filter_map(|item| match item {
        Item::Notice {
            kind: NoticeKind::Info,
            text,
            ..
        } => Some(text),
        _ => None,
    });
    notices.collect()
}

pub(super) fn turns_started(h: &Harness) -> usize {
    h.count_events(|event| matches!(event, BridgeEvent::TurnStarted { .. }))
}

/// Paste `text` at once and wait for it in the pane.
fn pasted(h: &Harness, s: &Sess, text: &str) {
    let outcome = send(h, s, text);
    assert!(
        matches!(outcome, SendOutcome::Delivering { .. }),
        "{outcome:?}"
    );
    wait_until("the paste", || {
        h.sent(s).last().map(String::as_str) == Some(text)
    });
}

fn chip(name: &str, args: &str) -> Option<CommandRef> {
    let (name, args) = (name.to_owned(), args.to_owned());
    Some(CommandRef { name, args })
}

#[test]
fn p2_skills_a_prompt_type_skill_confirms_once_and_its_record_gives_the_chip() {
    let (h, text) = (harness(), "/loom-plan:args alpha beta");
    let s = session(&h);
    listed(&h, &s, &[]);
    let mut replay = Replay::load("p2-skills", &s);
    replay.until(&s, "2026-11-10T06:39:54.754Z");
    h.wait_state(&s, SessionState::Idle);
    pasted(&h, &s, text);
    // Its prompt, its record and the skill body, the reply and the Stop.
    replay.until(&s, "2026-11-10T06:39:57.346Z");
    let shown = [(Delivery::Delivered, chip("loom-plan:args", "alpha beta"))];
    wait_until("the chip", || users(&h, text) == shown);
    h.wait_state(&s, SessionState::Idle);
    settle();
    assert_eq!(users(&h, text), shown, "a record made a second item");
    let deltas = h.deltas.lock().unwrap();
    let confirmed = deltas.iter().filter(|delta| {
        let ChatDelta::ItemReplaced { item, .. } = delta else {
            return false;
        };
        matches!(item, Item::User { text: t, delivery: Delivery::Delivered, command: None, .. } if t == text)
    });
    assert_eq!(confirmed.count(), 1, "confirmed more than once");
}

#[test]
fn p2_skills_a_fork_skill_settles_into_idle_and_its_notification_opens_the_next_turn() {
    let h = harness();
    let s = session(&h);
    listed(&h, &s, &[]);
    let mut replay = Replay::load("p2-skills", &s);
    replay.until(&s, "2026-11-10T06:40:01.380Z");
    h.wait_state(&s, SessionState::Idle);
    pasted(&h, &s, "/probe-fork");
    // Its prompt, its plain record, the launch record and the fork's SubagentStart.
    replay.until(&s, "2026-11-10T06:40:03.822Z");
    h.wait_state(&s, SessionState::Working);
    let launched = Instant::now();
    wait_until("the fork delivered", || delivered(&h, "/probe-fork"));
    h.wait_state(&s, SessionState::Idle);
    assert!(
        launched.elapsed() >= Duration::from_millis(1500),
        "idle before the settle"
    );
    let notice = "Running in the background as @probe-fork";
    assert!(info_notices(&h).iter().any(|text| text == notice));

    let before = turns_started(&h);
    replay.rest(&s);
    wait_until("the notification's turn", || {
        turns_started(&h) == before + 1
    });
    h.wait_state(&s, SessionState::Idle);
    let turns = h.bridge.chat_snapshot(true).turns;
    let last = turns.last().unwrap();
    let reply = "The `/probe-fork` background agent finished and returned `FORK-OK`.";
    let answered = last
        .items
        .iter()
        .any(|item| matches!(item, Item::Text { markdown, .. } if markdown == reply));
    assert!(answered && !last.open, "{last:?}");
}

#[test]
fn p2_usage_context_confirms_on_its_system_record_and_settles_its_own_turn() {
    let h = harness();
    let s = session(&h);
    listed(&h, &s, &[]);
    let mut replay = Replay::load("p2-usage-context", &s);
    replay.until(&s, "2026-11-10T06:40:49.227Z");
    h.wait_state(&s, SessionState::Idle);
    pasted(&h, &s, "/context");
    replay.rest(&s);
    let shown = || {
        info_notices(&h)
            .iter()
            .any(|text| text.contains("Context Usage"))
    };
    wait_until("/context delivered", || {
        delivered(&h, "/context") && shown()
    });
    let turns = h.bridge.chat_snapshot(true).turns;
    let own = turns.last().unwrap();
    let notice = own.items.iter().find_map(|item| match item {
        Item::Notice { text, .. } if text.contains("Context Usage") => Some(text.clone()),
        _ => None,
    });
    let notice = notice.unwrap_or_else(|| panic!("no notice in the command's turn: {own:?}"));
    assert!(!notice.contains('\u{1b}'), "{notice:?}");
    assert!(own.open, "the command's turn closed before its settle");
    let closed = || {
        h.bridge
            .chat_snapshot(true)
            .turns
            .last()
            .is_some_and(|turn| !turn.open)
    };
    wait_until("the settle closes the command's turn", closed);
    assert_eq!(h.state(&s), Some(SessionState::Idle));
}

#[test]
fn p2_compact_confirms_on_pre_compact_and_holds_the_queue_until_its_boundary_settles() {
    let h = Harness::ready(|c| c.delivery_timeout = Duration::from_millis(600));
    h.bridge.lock().stall_budget = Duration::from_secs(100 * 365 * 86_400);
    let s = h.idle(Authoring);
    let mut replay = Replay::load("p2-compact", &s);
    replay.until(&s, "2026-11-10T06:40:26.485Z");
    h.wait_state(&s, SessionState::Idle);
    pasted(&h, &s, "/compact");
    assert!(matches!(send(&h, &s, "next"), SendOutcome::Queued { .. }));
    replay.until(&s, "2026-11-10T06:40:26.763Z");
    wait_until("confirmed by PreCompact", || delivered(&h, "/compact"));
    // Past the delivery timeout: the compaction runs on, and nothing else goes in meanwhile.
    thread::sleep(Duration::from_millis(1200));
    assert!(delivered(&h, "/compact"), "{:?}", users(&h, "/compact"));
    assert_eq!(h.sent(&s), ["/compact"], "pasted before the boundary");
    replay.rest(&s);
    let boundary = Instant::now();
    wait_until("next after the settle", || {
        h.sent(&s) == ["/compact", "next"]
    });
    assert!(
        boundary.elapsed() >= Duration::from_millis(1500),
        "pasted before the settle"
    );
    assert_eq!(h.state(&s), Some(SessionState::Idle));
}

#[test]
fn p2_unlisted_lengths_a_name_that_left_the_list_confirms_on_its_warning() {
    let h = harness();
    let s = session(&h);
    let gone =
        json!({"name": "probe-unlisted-zz", "description": "gone (user)", "argumentHint": ""});
    listed(&h, &s, &[gone]);
    let mut replay = Replay::load("p2-unlisted-lengths", &s);
    replay.until(&s, "2026-11-10T06:43:01.134Z");
    h.wait_state(&s, SessionState::Idle);
    pasted(&h, &s, "/probe-unlisted-zz");
    assert!(matches!(send(&h, &s, "next"), SendOutcome::Queued { .. }));
    replay.until(&s, "2026-11-10T06:43:01.375Z");
    wait_until("confirmed by its warning", || {
        delivered(&h, "/probe-unlisted-zz")
    });
    let warning = "Unknown command: /probe-unlisted-zz";
    assert!(info_notices(&h).iter().any(|text| text == warning));
    settle();
    assert_eq!(
        h.sent(&s),
        ["/probe-unlisted-zz"],
        "pasted before the settle"
    );
    wait_until("next", || h.sent(&s) == ["/probe-unlisted-zz", "next"]);
}

#[test]
fn p2_unlisted_lengths_a_300_character_line_is_pasted_and_confirmed() {
    let h = harness();
    let s = session(&h);
    listed(&h, &s, &[]);
    let mut replay = Replay::load("p2-unlisted-lengths", &s);
    replay.until(&s, "2026-11-10T06:43:17.025Z");
    h.wait_state(&s, SessionState::Idle);
    let words: Vec<String> = (1..=57).map(|n| format!("w{n:03}")).collect();
    let line = format!("/probe-personal {}", words.join(" "));
    assert_eq!(line.chars().count(), 300);
    pasted(&h, &s, &line);
    replay.until(&s, "2026-11-10T06:43:22.680Z");
    let shown = [(
        Delivery::Delivered,
        chip("probe-personal", &words.join(" ")),
    )];
    wait_until("the long line confirmed", || users(&h, &line) == shown);
}

#[test]
fn p2_dialogs_a_paste_that_leaves_no_record_times_out_and_is_never_resent() {
    let h = Harness::ready(|c| c.delivery_timeout = Duration::from_millis(400));
    h.bridge.lock().stall_budget = Duration::from_secs(100 * 365 * 86_400);
    let s = h.idle(Authoring);
    let mut replay = Replay::load("p2-dialogs", &s);
    replay.until(&s, "2026-11-10T06:41:45.314Z");
    h.wait_state(&s, SessionState::Idle);
    pasted(&h, &s, "/context");
    assert!(matches!(send(&h, &s, "next"), SendOutcome::Queued { .. }));
    let undelivered = [(Delivery::Undelivered, None)];
    wait_until("timed out", || users(&h, "/context") == undelivered);
    // The `/model` records written after the dialog's Escape name another command.
    replay.rest(&s);
    wait_until("their notice", || {
        info_notices(&h).iter().any(|t| t.contains("Kept model"))
    });
    settle();
    assert_eq!(users(&h, "/context"), undelivered);
    assert_eq!(
        h.sent(&s),
        ["/context"],
        "the queue went on, or the paste was resent"
    );
    h.spool(&s, "Stop", json!({}));
    wait_until("next after a Stop", || h.sent(&s) == ["/context", "next"]);
}
