//! Class rules and row bounds, beyond the frozen cost contracts.

use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use serde_json::json;

use super::classify::tool_result;
use super::{build, MAX_ROWS};
use crate::commands::usage::transcript_types::{
    AttachmentEntry, Entry, Request, RequestNormalization, Scope, TokenUsage, ToolUse, Transcript,
    UserEntry,
};

fn tool(name: &str, input: serde_json::Value) -> ToolUse {
    ToolUse {
        id: "t1".to_owned(),
        name: name.to_owned(),
        input,
    }
}

fn class_of(name: &str, input: serde_json::Value) -> String {
    tool_result(Some(&tool(name, input)))
}

#[test]
fn read_classes_follow_the_path() {
    let read = |path: &str| class_of("Read", json!({ "file_path": path }));
    assert_eq!(read("/h/.claude/loom-skill-catalog/x/y.md"), "read:skill");
    assert_eq!(read("/h/skills/a/SKILL.md"), "read:skill");
    assert_eq!(read("doc/loom/knowledge/INDEX.md"), "read:knowledge");
    assert_eq!(read("/r/doc/plans/PLAN-x.md"), "read:plan-or-signal");
    assert_eq!(read("/r/.loom/work/signals/s.md"), "read:plan-or-signal");
    assert_eq!(read("loom/src/x.rs"), "read:source-other");
}

#[test]
fn bash_classes_follow_the_command_word() {
    let bash = |command: &str| class_of("Bash", json!({ "command": command }));
    assert_eq!(bash("cd loom && cargo test"), "bash:build-test");
    assert_eq!(bash("loom stage list"), "bash:loom");
    assert_eq!(bash("git status"), "bash:git");
    assert_eq!(bash("rg -n foo"), "bash:search");
    assert_eq!(bash("cat README.md"), "bash:file-read");
    assert_eq!(bash("sed -n 1,5p x"), "bash:file-read");
    assert_eq!(bash("echo hi"), "bash:other");
}

#[test]
fn non_read_tools_have_their_own_classes() {
    assert_eq!(class_of("Agent", json!({})), "agent-report");
    assert_eq!(class_of("Task", json!({})), "agent-report");
    assert_eq!(class_of("Skill", json!({})), "skill-tool");
    assert_eq!(class_of("MultiEdit", json!({})), "edit-ack");
    assert_eq!(class_of("Glob", json!({})), "search-tool");
    assert_eq!(class_of("mcp__x__y", json!({})), "other-tool");
    assert_eq!(tool_result(None), "other-tool");
}

fn request(minute: u32) -> Entry {
    Entry::Assistant(Box::new(Request {
        message_id: Some(format!("m{minute}")),
        timestamp: Utc
            .with_ymd_and_hms(2026, 8, 30, 0, minute, 0)
            .single()
            .unwrap(),
        model: "claude-sonnet-5".to_owned(),
        usage: TokenUsage {
            input: 1,
            ..TokenUsage::default()
        },
        tool_uses: Vec::new(),
        thinking_chars: 0,
        text_chars: 0,
        normalization: RequestNormalization {
            usage_observed: true,
            true_fresh_start: Some(minute == 0),
            ..RequestNormalization::default()
        },
    }))
}

fn transcript(entries: Vec<Entry>, attachments: Vec<AttachmentEntry>) -> Transcript {
    Transcript {
        path: PathBuf::from("t.jsonl"),
        scope: Scope::Subagent,
        project_slug: "project".to_owned(),
        project_path: None,
        session_id: "s".to_owned(),
        agent_id: None,
        agent_type: None,
        agent_type_from_sidecar: false,
        requested_model: None,
        stage_id: None,
        loom_session_id: None,
        forward_receipt: None,
        forward_candidate: false,
        first_user_entry: None,
        entries,
        attachments,
        diagnostics: Default::default(),
    }
}

#[test]
fn rows_are_bounded_and_the_tail_merges_into_other() {
    let attachments = (0..MAX_ROWS + 6)
        .map(|index| AttachmentEntry {
            requests_before: 1,
            kind: format!("kind{index:02}"),
            chars: 400 + index,
        })
        .collect();
    let attribution = build(&[transcript(
        vec![request(0), request(1), request(2)],
        attachments,
    )]);

    assert_eq!(attribution.rows.len(), MAX_ROWS);
    let other = attribution.rows.last().unwrap();
    assert_eq!(other.class, "other");
    assert_eq!(other.items, 7);
    let sizes: Vec<f64> = attribution.rows.iter().map(|row| row.s2).collect();
    assert!(sizes[..MAX_ROWS - 1]
        .windows(2)
        .all(|pair| pair[0] >= pair[1]));
}

#[test]
fn user_text_is_a_row_and_untyped_subagents_group_as_unknown() {
    let message = Entry::User(UserEntry {
        timestamp: Utc.with_ymd_and_hms(2026, 8, 30, 0, 1, 0).single().unwrap(),
        tool_use_id: None,
        text: "x".repeat(40),
    });
    let attribution = build(&[transcript(
        vec![request(0), message, request(1), request(2)],
        Vec::new(),
    )]);

    let row = attribution
        .rows
        .iter()
        .find(|row| row.class == "user-text")
        .unwrap();
    assert_eq!((row.items, row.chars), (1, 40));
    assert_eq!(attribution.startup_by_agent[0].agent_type, "unknown");
}
