//! Transcript record lines, mapper views and attachment values for the bridge-core contracts.
//!
//! Record lines follow the shapes captured from Claude Code transcripts in
//! `doc/plans/feat-planner/briefs/feat-planner-07-session-bridge/common.md` "Transcript mapping", keeping
//! every key those shapes show.

use loom::plan_session::attachments::{Attachment, AttachmentKind, LockedTag};
use loom::plan_session::model::{Item, ToolStatus, Turn};
use loom::plan_session::transcript::Mapper;
use serde_json::{json, Value};

use super::support::SID;

/// `2026-10-27T10:00:<secs>.000Z`.
pub fn stamp(secs: u32) -> String {
    format!("2026-10-27T10:00:{secs:02}.000Z")
}

pub fn usage(input: u64, cache_write: u64, cache_read: u64, output: u64) -> Value {
    json!({
        "input_tokens": input,
        "cache_creation_input_tokens": cache_write,
        "cache_read_input_tokens": cache_read,
        "output_tokens": output,
    })
}

/// One assistant record holding `block` of message `message_id`.
pub fn assistant(uuid: &str, message_id: &str, secs: u32, block: Value, usage: Value) -> String {
    json!({
        "type": "assistant", "uuid": uuid, "parentUuid": null, "isSidechain": false,
        "timestamp": stamp(secs), "sessionId": SID, "requestId": "req_011CXa",
        "message": {
            "id": message_id, "model": "claude-opus-5-5", "role": "assistant",
            "content": [block], "stop_reason": "tool_use", "usage": usage,
        },
        "apiBlockIndex": 0,
    })
    .to_string()
}

fn merge(mut record: Value, extra: Value) -> String {
    for (key, value) in extra.as_object().unwrap() {
        record[key.as_str()] = value.clone();
    }
    record.to_string()
}

/// A user record with `content` (a string or an array of blocks) and any extra top-level keys.
pub fn user(uuid: &str, secs: u32, content: Value, extra: Value) -> String {
    let record = json!({
        "type": "user", "uuid": uuid, "parentUuid": null, "isSidechain": false,
        "timestamp": stamp(secs), "sessionId": SID,
        "message": {"role": "user", "content": content},
    });
    merge(record, extra)
}

/// A human prompt record.
pub fn human(uuid: &str, secs: u32, text: &str) -> String {
    let origin =
        json!({"promptId": "f79d0c1e", "permissionMode": "auto", "origin": {"kind": "human"}});
    user(uuid, secs, json!(text), origin)
}

/// A record of any other `type`, with its type-specific keys.
fn other(kind: &str, uuid: &str, secs: u32, extra: Value) -> String {
    let record = json!({"type": kind, "uuid": uuid, "timestamp": stamp(secs), "sessionId": SID});
    merge(record, extra)
}

pub fn tool_use(id: &str, name: &str, input: Value) -> Value {
    json!({
        "type": "tool_use", "id": id, "name": name, "input": input, "caller": {"type": "direct"},
    })
}

/// A Bash `tool_use` record with no result.
pub fn pending_bash(uuid: &str, tool_use_id: &str, secs: u32) -> String {
    let input = json!({"command": "sleep 600", "description": "Wait for the run"});
    let block = tool_use(tool_use_id, "Bash", input);
    assistant(uuid, "msg_01Bash", secs, block, usage(2, 0, 100, 40))
}

/// Three records of one streamed message (thinking, text, tool_use), each with usage; the last
/// carries the final counts `(3, 810, 56000, 920)`.
pub fn streamed_records() -> [String; 3] {
    let message = "msg_011CStream";
    let thinking = json!({"type": "thinking", "thinking": "", "signature": "EqA1"});
    let text = json!({"type": "text", "text": "Checking the runs."});
    let input = json!({"command": "gh run list --limit 5", "description": "List recent CI runs"});
    let tool = tool_use("toolu_01Rm", "Bash", input);
    [
        assistant("a1", message, 1, thinking, usage(2, 706, 55975, 12)),
        assistant("a2", message, 2, text, usage(2, 706, 55975, 300)),
        assistant("a3", message, 3, tool, usage(3, 810, 56000, 920)),
    ]
}

/// Records that map to no item: skipped types, meta and wrapped user records, a task
/// notification, and a `stop_hook_summary` system record.
pub fn noise_records() -> Vec<String> {
    let notification = json!({"origin": {"kind": "task-notification", "producer": "session-task"}});
    let caveat =
        "<local-command-caveat>Caveat: the command below was run directly</local-command-caveat>";
    let summary = json!({"subtype": "stop_hook_summary", "hookCount": 1, "level": "suggestion"});
    vec![
        other(
            "attachment",
            "n1",
            1,
            json!({"attachment": {"type": "file", "filename": "a.md"}}),
        ),
        other(
            "file-history-snapshot",
            "n2",
            1,
            json!({"messageId": "m1", "snapshot": {}}),
        ),
        other("mode", "n3", 1, json!({"mode": "auto"})),
        other("ai-title", "n4", 1, json!({"title": "Fix the runs"})),
        other(
            "queue-operation",
            "n5",
            1,
            json!({"operation": "enqueue", "content": "x"}),
        ),
        user(
            "n6",
            2,
            json!("Continue from where you left off."),
            json!({"isMeta": true}),
        ),
        user("n7", 2, json!(caveat), json!({})),
        user("n8", 2, json!("background task finished"), notification),
        other("system", "n9", 3, summary),
    ]
}

/// An Edit `tool_use` of the absolute `path` and its result record with a `structuredPatch`
/// of lines `[" a", "-b", "+c", "+d"]`.
pub fn edit_records(path: &str) -> [String; 2] {
    let input =
        json!({"file_path": path, "old_string": "b", "new_string": "c\nd", "replace_all": false});
    let call = assistant(
        "a1",
        "msg_01Edit",
        1,
        tool_use("toolu_01Ed", "Edit", input),
        usage(2, 0, 100, 60),
    );
    let content = format!("The file {path} has been updated successfully.");
    let result = json!([{
        "tool_use_id": "toolu_01Ed", "type": "tool_result", "content": content, "is_error": false,
    }]);
    let patch = json!([{
        "oldStart": 1, "oldLines": 2, "newStart": 1, "newLines": 3,
        "lines": [" a", "-b", "+c", "+d"],
    }]);
    let tool_result = json!({
        "filePath": path, "oldString": "b", "newString": "c\nd", "originalFile": "a\nb\n",
        "structuredPatch": patch, "userModified": false, "replaceAll": false,
    });
    let extra = json!({"toolUseResult": tool_result, "sourceToolAssistantUUID": "a1"});
    [call, user("u2", 2, result, extra)]
}

/// The turn `id` of the mapper.
pub fn turn<'a>(mapper: &'a Mapper, id: &str) -> &'a Turn {
    let turns = mapper.turns();
    turns
        .iter()
        .find(|turn| turn.id == id)
        .expect("the turn exists")
}

/// Every item of every turn.
pub fn all_items(turns: &[Turn]) -> Vec<&Item> {
    turns.iter().flat_map(|turn| turn.items.iter()).collect()
}

/// How many of `items` match `pred`.
pub fn count<'a>(items: impl IntoIterator<Item = &'a Item>, pred: fn(&Item) -> bool) -> usize {
    items.into_iter().filter(|item| pred(item)).count()
}

/// Status, added and removed of every tool item of the turn.
pub fn tool_rows(turn: &Turn) -> Vec<(ToolStatus, Option<u32>, Option<u32>)> {
    let row = |item: &Item| match item {
        Item::Tool {
            status,
            added,
            removed,
            ..
        } => Some((status.clone(), *added, *removed)),
        _ => None,
    };
    turn.items.iter().filter_map(row).collect()
}

/// An attachment with no lines, stage, sentence, rests-on, lock or change.
pub fn attachment(kind: AttachmentKind, path: &str, anchor: &str, quote: &str) -> Attachment {
    Attachment {
        kind,
        path: path.to_string(),
        anchor: Some(anchor.to_string()),
        lines: None,
        stage: None,
        summary_sentence: None,
        rests_on: Vec::new(),
        quote: quote.to_string(),
        changed_since_selected: false,
        locked: None,
        file: None,
    }
}

/// Section 10's example entries: a source with lines 40-44, a locked and changed stage address,
/// and a summary sentence resting on two sections.
pub fn sample_attachments() -> [Attachment; 3] {
    let quote = "`loom plan` starts the daemon if it is not running and\n\
                 registers the planning server with it.";
    let mut lines = attachment(
        AttachmentKind::Source,
        "doc/plans/PLAN-plan-server.md",
        "lifetime",
        quote,
    );
    lines.lines = Some((40, 44));
    let quote = "Integration verify runs the dashboard against a live daemon.";
    let mut stage = attachment(
        AttachmentKind::Source,
        "doc/plans/DONE-PLAN-token-flow.md",
        "stage:integration-verify",
        quote,
    );
    stage.stage = Some("integration-verify".into());
    stage.locked = Some(LockedTag::Done);
    stage.changed_since_selected = true;
    let quote = "The planning server runs inside the loom daemon.";
    let mut summary = attachment(
        AttachmentKind::Summary,
        "doc/loom/summaries/plans/plan-server.md",
        "s-3fa9c2d1",
        quote,
    );
    summary.summary_sentence = Some("s-3fa9c2d1".into());
    summary.rests_on = vec![
        "doc/plans/PLAN-plan-server.md#lifetime".into(),
        "doc/plans/PLAN-token-flow.md#stages".into(),
    ];
    [lines, stage, summary]
}

/// 60 lines of 45 characters, each ending in a multi-byte character (é, 中, an emoji).
pub fn multibyte_lines() -> Vec<String> {
    let ends = ['é', '中', '😀'];
    let lines: Vec<String> = (0..60)
        .map(|i| format!("{i:02} {}{}", "x".repeat(41), ends[i % 3]))
        .collect();
    assert!(lines.iter().all(|line| line.chars().count() == 45));
    lines
}

/// The block lines after the entry header starting `header`, up to `</loom-attachments>`.
pub fn entry_lines<'a>(formatted: &'a str, header: &str) -> Vec<&'a str> {
    formatted
        .lines()
        .skip_while(|line| !line.starts_with(header))
        .skip(1)
        .take_while(|line| *line != "</loom-attachments>")
        .collect()
}
