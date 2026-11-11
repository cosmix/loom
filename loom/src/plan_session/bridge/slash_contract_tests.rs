//! Contracts of slash delivery: a queued command is checked against the session's table again
//! just before its paste, and a local command holds the queue until its output settles.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::fake_host::{settle, wait_until, Harness, Sess};
use super::SendOutcome;
use crate::plan_session::commands::CommandTable;
use crate::plan_session::kind::SessionKind::Authoring;
use crate::plan_session::model::{Item, NoticeKind};

const INITIALIZE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/plan_session/probe-notes/p2-side-initialize.side-loom-ui-disabled.jsonl"
));
const USAGE_CONTEXT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/plan_session/transcripts/p2-usage-context.jsonl"
));

/// The rows of the recorded `initialize` answer (line 1, `.response.response.commands`).
fn initialize_rows() -> Vec<Value> {
    let first = INITIALIZE
        .lines()
        .next()
        .expect("the initialize answer line");
    let answer: Value = serde_json::from_str(first).expect("the answer is JSON");
    let rows = answer["response"]["response"]["commands"].as_array();
    rows.expect("the answer's command rows").clone()
}

/// Swap `table` into the session's command cell, as the catalogue owner does.
fn set_table(h: &Harness, s: &Sess, table: CommandTable) {
    let cell = h
        .bridge
        .command_cell(&s.id)
        .expect("the session's command cell");
    *cell.write().unwrap() = Arc::new(table);
}

fn send(h: &Harness, s: &Sess, text: &str) -> SendOutcome {
    h.bridge.send(&s.id, text.to_owned(), Vec::new()).unwrap()
}

fn info_notices(h: &Harness) -> Vec<String> {
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

#[test]
fn a_queued_skill_whose_row_vanished_is_not_pasted() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    let rows = initialize_rows();
    set_table(&h, &s, CommandTable::from_initialize(&rows, 1));
    h.working(&s);
    let skill = send(&h, &s, "/probe-personal hi");
    assert!(matches!(skill, SendOutcome::Queued { .. }), "{skill:?}");
    let later = send(&h, &s, "later");
    assert!(matches!(later, SendOutcome::Queued { .. }), "{later:?}");

    let without: Vec<Value> = rows
        .iter()
        .filter(|row| row["name"] != json!("probe-personal"))
        .cloned()
        .collect();
    set_table(&h, &s, CommandTable::from_initialize(&without, 2));
    h.spool(&s, "Stop", json!({}));

    wait_until("later pasted", || h.sent(&s) == ["later"]);
    settle();
    assert_eq!(h.sent(&s), ["later"], "the vanished skill was pasted");
    assert!(h.bridge.chat_snapshot(true).queue.is_empty());
    let notices = info_notices(&h);
    let dropped = notices.iter().any(|text| {
        text.contains("/probe-personal hi") && text.contains("Not in this session's command list")
    });
    assert!(dropped, "no notice names the dropped command: {notices:?}");
}

/// The two `system` `local_command` records `/context` writes, in order.
fn context_records() -> Vec<Value> {
    let records = USAGE_CONTEXT.lines().map(|line| {
        let record: Value = serde_json::from_str(line).expect("a JSON record");
        record
    });
    let local = records.filter(|record| record["subtype"] == json!("local_command"));
    let local: Vec<Value> = local.collect();
    assert_eq!(local.len(), 2, "the fixture's /context records");
    assert_eq!(local[1]["commandRun"]["command"], json!("context"));
    local
}

#[test]
fn a_local_command_holds_the_queue_until_it_settles() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    set_table(&h, &s, CommandTable::from_initialize(&initialize_rows(), 1));
    let context = send(&h, &s, "/context");
    assert!(
        matches!(context, SendOutcome::Delivering { .. }),
        "{context:?}"
    );
    let hello = send(&h, &s, "hello");
    assert!(matches!(hello, SendOutcome::Queued { .. }), "{hello:?}");
    wait_until("/context pasted", || h.sent(&s) == ["/context"]);

    for record in context_records() {
        h.record(&s, record);
    }
    let last_record = Instant::now();
    while last_record.elapsed() < Duration::from_secs(1) {
        let sent = h.sent(&s);
        assert_eq!(
            sent,
            ["/context"],
            "hello was pasted before /context settled"
        );
        thread::sleep(Duration::from_millis(20));
    }
    while h.sent(&s) != ["/context", "hello"] {
        let waited = last_record.elapsed();
        assert!(
            waited < Duration::from_secs(4),
            "hello not pasted after the settle"
        );
        thread::sleep(Duration::from_millis(20));
    }
}
