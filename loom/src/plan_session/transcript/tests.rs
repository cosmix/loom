//! The mapper driven through the recorded-transcript fixtures under
//! `tests/fixtures/plan_session/transcripts/`, built from the shapes common.md captured.

use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use super::{Ingest, Mapper, RecordMark};
use crate::plan_session::model::{
    ChatDelta, Delivery, Item, NoticeKind, Origin, TokenCounts, ToolStatus, Turn,
};

const ROOT: &str = "/work/proj";

pub(super) fn lines(name: &str) -> Vec<String> {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/plan_session/transcripts"
    );
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{path}: {err}"));
    text.lines().map(str::to_owned).collect()
}

pub(super) fn mapper() -> Mapper {
    Mapper::new(PathBuf::from(ROOT))
}

pub(super) fn drive(mapper: &mut Mapper, lines: &[String]) -> Ingest {
    let mut all = Ingest::default();
    for line in lines {
        let one = mapper.ingest_main(line);
        all.deltas.extend(one.deltas);
        all.marks.extend(one.marks);
    }
    all
}

/// Ingesting the same lines again publishes nothing.
fn replay_is_silent(mapper: &mut Mapper, lines: &[String]) {
    let before = mapper.turns().to_vec();
    let again = drive(mapper, lines);
    assert!(again.deltas.is_empty(), "{:?}", again.deltas);
    assert!(again.marks.is_empty(), "{:?}", again.marks);
    assert_eq!(mapper.turns(), before.as_slice());
}

pub(super) fn at(secs: i64) -> DateTime<Utc> {
    let start: DateTime<Utc> = "2026-10-27T10:00:00Z".parse().unwrap();
    start + Duration::seconds(secs)
}

pub(super) fn tokens(input: u64, cache_write: u64, cache_read: u64, output: u64) -> TokenCounts {
    TokenCounts {
        input,
        cache_write,
        cache_read,
        output,
    }
}

pub(super) fn human(secs: i64, text: &str) -> RecordMark {
    let (at, text) = (at(secs), text.to_owned());
    RecordMark::HumanUser { at, text }
}

pub(super) fn user(id: &str, text: &str, origin: Option<Origin>) -> Item {
    Item::User {
        id: id.to_owned(),
        text: text.to_owned(),
        attachments: Vec::new(),
        delivery: Delivery::Delivered,
        origin,
    }
}

pub(super) fn notice(id: &str, kind: NoticeKind, text: &str) -> Item {
    let (id, text) = (id.to_owned(), text.to_owned());
    Item::Notice { id, kind, text }
}

fn thinking(id: &str, ms: u64) -> Item {
    let (id, duration_ms) = (id.to_owned(), Some(ms));
    Item::Thinking { id, duration_ms }
}

pub(super) fn text(id: &str, markdown: &str) -> Item {
    let (id, markdown) = (id.to_owned(), markdown.to_owned());
    Item::Text { id, markdown }
}

pub(super) fn tool(id: &str, tool_use_id: &str, name: &str, summary: &str, input: Value) -> Item {
    Item::Tool {
        id: id.to_owned(),
        tool_use_id: tool_use_id.to_owned(),
        name: name.to_owned(),
        summary: summary.to_owned(),
        status: ToolStatus::Pending,
        subagent: None,
        parent: None,
        input: serde_json::to_string_pretty(&input).unwrap(),
        result: None,
        result_truncated: false,
        error_line: None,
        added: None,
        removed: None,
        patch: None,
        nested_tools: 0,
    }
}

pub(super) fn finished(mut item: Item, done: ToolStatus, text: &str) -> Item {
    if let Item::Tool { status, result, .. } = &mut item {
        *status = done;
        *result = Some(text.to_owned());
    }
    item
}

pub(super) fn only_turn(mapper: &Mapper) -> &Turn {
    assert_eq!(mapper.turns().len(), 1, "{:?}", mapper.turns());
    &mapper.turns()[0]
}

#[test]
fn simple_turn_opens_implicitly_and_fills_both_durations() {
    let mut mapper = mapper();
    mapper.expect_opening(Some(Origin::Offer));
    let lines = lines("simple-turn.jsonl");
    let out = drive(&mut mapper, &lines);

    let expected = Turn {
        id: "t1".into(),
        started_at: "2026-10-27T10:00:00.000Z".into(),
        duration_ms: Some(4200),
        open: true,
        tokens: tokens(2, 100, 1000, 50),
        items: vec![
            user("u-u1", "Summarise the plan.", Some(Origin::Offer)),
            thinking("a-a1-0", 3000),
            text("a-a2-0", "The plan has **two** stages."),
        ],
        changed_files: Vec::new(),
        summaries_rebuilding: Vec::new(),
    };
    assert_eq!(mapper.turns(), [expected]);
    assert!(matches!(out.deltas[0], ChatDelta::TurnAdded { .. }));
    let main = |secs| RecordMark::Main { at: at(secs) };
    let marks = [human(0, "Summarise the plan."), main(1), main(4)];
    assert_eq!(out.marks, marks);
    assert_eq!(mapper.last_main_record_at(), Some(at(4)));
    replay_is_silent(&mut mapper, &lines);

    let adopted = mapper.begin_turn(at(0));
    assert_eq!(adopted, ("t1".to_owned(), Vec::new()));
    let closed = mapper.end_turn();
    let closing = matches!(&closed[..], [ChatDelta::TurnUpdated { open: false, .. }]);
    assert!(closing, "{closed:?}");
    assert!(mapper.end_turn().is_empty());
    assert_eq!(mapper.open_turn(), None);
    let (next, deltas) = mapper.begin_turn(at(6));
    assert_eq!((next.as_str(), mapper.open_turn()), ("t2", Some("t2")));
    assert!(matches!(&deltas[..], [ChatDelta::TurnAdded { .. }]));
}

#[test]
fn tool_results_complete_rows_with_errors_patches_and_changed_paths() {
    let mut mapper = mapper();
    mapper.begin_turn(at(0));
    let lines = lines("tools.jsonl");
    drive(&mut mapper, &lines);

    let input = json!({"file_path": "/work/proj/doc/plans/PLAN-x.md", "offset": 10, "limit": 5});
    let read = tool(
        "a-a1-0",
        "toolu_01Rd",
        "Read",
        "doc/plans/PLAN-x.md:10-14",
        input,
    );
    let read = finished(
        read,
        ToolStatus::Done,
        "    10\tline ten\n    11\tline eleven",
    );
    let input = json!({"command": "cargo test\n--quiet", "description": "Run the tests"});
    let bash = tool("a-a3-0", "toolu_01Bh", "Bash", "cargo test", input);
    let output = "error: could not compile `loom`\nwarning: 1 warning emitted";
    let mut bash = finished(bash, ToolStatus::Error, output);
    if let Item::Tool { error_line, .. } = &mut bash {
        *error_line = Some("error: could not compile `loom`".into());
    }
    let input = json!({"file_path": "/work/proj/doc/product/x.md", "old_string": "b",
                       "new_string": "c\nd", "replace_all": false});
    let edit = tool("a-a5-0", "toolu_01Ed", "Edit", "doc/product/x.md", input);
    let updated = "The file /work/proj/doc/product/x.md has been updated successfully.";
    let mut edit = finished(edit, ToolStatus::Done, updated);
    if let Item::Tool {
        added,
        removed,
        patch,
        ..
    } = &mut edit
    {
        (*added, *removed) = (Some(2), Some(1));
        *patch = Some("@@ -1,2 +1,3 @@\n a\n-b\n+c\n+d\n".into());
    }

    let turn = only_turn(&mapper);
    assert_eq!(turn.items, [read, bash, edit]);
    assert_eq!(turn.tokens, tokens(3, 0, 600, 60));
    assert_eq!(mapper.changed_by_tool("t1"), ["doc/product/x.md"]);
    assert!(mapper.changed_by_tool("t9").is_empty());
    replay_is_silent(&mut mapper, &lines);
}

#[test]
fn streamed_message_counts_its_last_usage_once() {
    let mut mapper = mapper();
    mapper.begin_turn(at(0));
    let lines = lines("streamed.jsonl");
    drive(&mut mapper, &lines);

    let input = json!({"command": "gh run list --limit 5", "description": "List recent CI runs"});
    let turn = only_turn(&mapper);
    let items = [
        thinking("a-a1-0", 1000),
        text("a-a2-0", "Checking the runs."),
        tool(
            "a-a3-0",
            "toolu_01Rm",
            "Bash",
            "gh run list --limit 5",
            input,
        ),
    ];
    assert_eq!(turn.items, items);
    assert_eq!(turn.tokens, tokens(3, 810, 56000, 920));
    replay_is_silent(&mut mapper, &lines);
}

#[test]
fn interrupt_closes_the_turn_and_leaves_pending_tools_not_finished() {
    let mut mapper = mapper();
    let lines = lines("interrupt.jsonl");
    let out = drive(&mut mapper, &lines);

    let input = json!({"file_path": "/work/proj/doc/a.md"});
    let read = tool("a-a1-0", "toolu_01Rd", "Read", "doc/a.md", input);
    let input = json!({"command": "sleep 600", "description": "Wait for the run"});
    let mut bash = tool("a-a1-1", "toolu_01Sl", "Bash", "sleep 600", input);
    if let Item::Tool { status, .. } = &mut bash {
        *status = ToolStatus::NotFinished;
    }
    let turn = only_turn(&mapper);
    let items = [
        user("u-u1", "Watch the run.", None),
        finished(read, ToolStatus::Done, "     1\tA"),
        bash,
        notice("n-u3", NoticeKind::Interrupted, "Interrupted"),
    ];
    assert_eq!(turn.items, items);
    assert!(!turn.open);
    let interrupted = RecordMark::Interrupted { at: at(5) };
    assert!(out.marks.contains(&interrupted), "{:?}", out.marks);
    assert!(
        mapper.end_turn().is_empty(),
        "the interrupt already ended the turn"
    );
    assert!(mapper.interrupted_since(at(4)) && !mapper.interrupted_since(at(5)));
    replay_is_silent(&mut mapper, &lines);
}

#[test]
fn an_interrupt_after_a_stopped_notice_adds_no_second_notice() {
    let mut mapper = mapper();
    let lines = lines("interrupt.jsonl");
    let (interrupt, before) = lines.split_last().expect("fixture lines");
    drive(&mut mapper, before);
    mapper.add_server_item(notice("stopped-1", NoticeKind::Stopped, "Stopped"));
    mapper.ingest_main(interrupt);

    let input = json!({"file_path": "/work/proj/doc/a.md"});
    let read = tool("a-a1-0", "toolu_01Rd", "Read", "doc/a.md", input);
    let input = json!({"command": "sleep 600", "description": "Wait for the run"});
    let mut bash = tool("a-a1-1", "toolu_01Sl", "Bash", "sleep 600", input);
    if let Item::Tool { status, .. } = &mut bash {
        *status = ToolStatus::NotFinished;
    }
    let items = [
        user("u-u1", "Watch the run.", None),
        finished(read, ToolStatus::Done, "     1\tA"),
        bash,
        notice("stopped-1", NoticeKind::Stopped, "Stopped"),
    ];
    assert_eq!(only_turn(&mapper).items, items);
}

#[test]
fn compaction_and_api_errors_become_notices() {
    let mut mapper = mapper();
    let lines = lines("compaction.jsonl");
    let out = drive(&mut mapper, &lines);
    let items = [
        user("u-u1", "/compact keep the plan", None),
        notice("n-c1", NoticeKind::Compacted, "Context compacted"),
    ];
    assert_eq!(only_turn(&mapper).items, items);
    let boundary = RecordMark::CompactBoundary { at: at(9) };
    assert!(out.marks.contains(&boundary), "{:?}", out.marks);
    replay_is_silent(&mut mapper, &lines);

    let mut mapper = self::mapper();
    mapper.begin_turn(at(0));
    let lines = self::lines("api-error.jsonl");
    drive(&mut mapper, &lines);
    let items = [
        notice("n-e1", NoticeKind::ApiError, "API Error: 529 Overloaded"),
        notice("n-e2", NoticeKind::ApiError, "Internal server error"),
    ];
    assert_eq!(only_turn(&mapper).items, items);
    replay_is_silent(&mut mapper, &lines);
}

#[test]
fn noise_records_produce_no_items_and_only_their_marks() {
    let mut mapper = mapper();
    mapper.begin_turn(at(0));
    let lines = lines("noise.jsonl");
    let out = drive(&mut mapper, &lines);

    let prompt = user("u-u1", "commit these fixes.", None);
    assert_eq!(only_turn(&mapper).items, [prompt]);
    let marks = [
        RecordMark::AgentsKilled { at: at(2) },
        RecordMark::SelfStartedTurn { at: at(3) },
        human(4, "commit these fixes."),
    ];
    assert_eq!(out.marks, marks);
    replay_is_silent(&mut mapper, &lines);
}
