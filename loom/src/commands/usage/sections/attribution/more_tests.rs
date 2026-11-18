//! Class rules and row bounds, beyond the frozen cost contracts.

use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use serde_json::json;

use super::classify::tool_result;
use super::{build, Attribution, MAX_ROWS};
use crate::commands::usage::transcript_types::{
    AttachmentEntry, Entry, Request, RequestNormalization, Scope, TokenUsage, ToolUse, Transcript,
    UserEntry, SYNTHETIC_MODEL,
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
fn the_bash_class_comes_from_the_shared_program_word() {
    let bash = |command: &str| class_of("Bash", json!({ "command": command }));
    assert_eq!(bash("RUST_LOG=debug cargo test"), "bash:build-test");
    assert_eq!(bash("/usr/bin/cargo test"), "bash:build-test");
    assert_eq!(bash("cd \"a b\" && cd c && git status"), "bash:git");
    assert_eq!(bash("FOO=1 cd x && BAR=2 rg y"), "bash:search");
    assert_eq!(bash("cd x"), "bash:other");
    assert_eq!(bash("   "), "bash:other");
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
    let usage = TokenUsage {
        input: 1,
        ..TokenUsage::default()
    };
    request_of(minute, "claude-sonnet-5", usage)
}

fn request_of(minute: u32, model: &str, usage: TokenUsage) -> Entry {
    Entry::Assistant(Box::new(Request {
        message_id: Some(format!("m{minute}")),
        timestamp: Utc
            .with_ymd_and_hms(2026, 8, 30, 0, minute, 0)
            .single()
            .unwrap(),
        model: model.to_owned(),
        usage,
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

/// An API-error row that never reached the API, with usage large enough to show
/// in any total that lets it in.
fn synthetic(minute: u32) -> Entry {
    let usage = TokenUsage {
        input: 1000,
        output: 1000,
        ..TokenUsage::default()
    };
    request_of(minute, SYNTHETIC_MODEL, usage)
}

fn assert_close(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "{label}: expected {expected}, got {actual}"
    );
}

#[test]
fn a_synthetic_row_adds_nothing_to_the_totals() {
    let attribution = build(&[transcript(
        vec![request(0), synthetic(1), request(2)],
        Vec::new(),
    )]);

    assert_close(attribution.total_s2, 2.0, "total_s2");
    assert_close(attribution.output_s2, 0.0, "output_s2");
}

#[test]
fn a_synthetic_row_does_not_move_an_admission_later() {
    let message = Entry::User(UserEntry {
        timestamp: Utc.with_ymd_and_hms(2026, 8, 30, 0, 1, 0).single().unwrap(),
        tool_use_id: None,
        text: "x".repeat(400),
    });
    let attribution = build(&[transcript(
        vec![request(0), synthetic(1), message, request(2), request(3)],
        Vec::new(),
    )]);

    // Admitted after one real request of three: request 2 writes it, request 3 reads it.
    let row = attribution
        .rows
        .iter()
        .find(|row| row.class == "user-text")
        .unwrap();
    assert_close(row.s2, 135.0, "user-text");
}

fn parsed(rows: &[serde_json::Value]) -> Transcript {
    use crate::commands::usage::discovery::DiscoveredFile;
    use crate::commands::usage::time_range::TimeRange;

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("s.jsonl");
    let lines: Vec<String> = rows.iter().map(ToString::to_string).collect();
    std::fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();
    let file = DiscoveredFile {
        path,
        project_slug: "project".to_owned(),
        scope: Scope::Main,
        session_id: "s".to_owned(),
        agent_id: None,
    };
    let range = TimeRange {
        since: Utc.with_ymd_and_hms(2026, 8, 29, 0, 0, 0).single().unwrap(),
        until: None,
    };
    crate::commands::usage::transcript::parse(&file, &range).unwrap()
}

#[test]
fn a_row_copied_under_the_same_uuid_is_charged_once() {
    let assistant = |minute: u32| {
        json!({
            "type": "assistant",
            "timestamp": format!("2026-08-30T00:{minute:02}:00Z"),
            "message": {
                "id": format!("m{minute}"),
                "model": "claude-sonnet-5",
                "content": [],
                "usage": {"input_tokens": 1}
            }
        })
    };
    let message = json!({
        "type": "user",
        "uuid": "u1",
        "timestamp": "2026-08-30T00:01:00Z",
        "message": {"content": "x".repeat(400)}
    });
    let context = json!({
        "type": "attachment",
        "uuid": "a1",
        "timestamp": "2026-08-30T00:01:00Z",
        "attachment": {"type": "hook_additional_context"},
        "rendered": [{"content": "y".repeat(400)}]
    });
    let attribution = build(&[parsed(&[
        assistant(0),
        message.clone(),
        message,
        context.clone(),
        context,
        assistant(2),
        assistant(3),
    ])]);

    for class in ["user-text", "attach:hook_additional_context"] {
        let row = attribution
            .rows
            .iter()
            .find(|row| row.class == class)
            .unwrap();
        assert_eq!(row.items, 1, "{class}");
        assert_close(row.s2, 135.0, class);
    }
}

fn accounted(attribution: &Attribution) -> f64 {
    let rows: f64 = attribution.rows.iter().map(|row| row.s2).sum();
    attribution.startup_s2
        + attribution.output_s2
        + attribution.assistant_replay_s2
        + rows
        + attribution.residual_s2
        - attribution.over_attributed_s2
}

#[test]
fn each_transcript_settles_alone_and_the_merged_tail_keeps_its_cost() {
    let extra = MAX_ROWS + 6;
    let attachments = (0..extra)
        .map(|index| AttachmentEntry {
            requests_before: 1,
            kind: format!("kind{index:02}"),
            chars: 400 + index,
        })
        .collect();
    let excess = transcript(vec![request(0), request(1), request(2)], attachments);
    let reads = TokenUsage {
        input: 1,
        cache_read: 50_000,
        ..TokenUsage::default()
    };
    let unexplained = transcript(
        vec![
            request(0),
            request(1),
            request_of(2, "claude-sonnet-5", reads),
        ],
        Vec::new(),
    );

    let excess_alone = build(std::slice::from_ref(&excess));
    let unexplained_alone = build(std::slice::from_ref(&unexplained));
    let both = build(&[excess, unexplained]);

    assert!(excess_alone.over_attributed_s2 > 0.0);
    assert!(unexplained_alone.residual_s2 > 0.0);
    assert_close(accounted(&both), both.total_s2, "the sum over both");
    assert_close(
        both.over_attributed_s2,
        excess_alone.over_attributed_s2,
        "over_attributed_s2",
    );
    assert_close(
        both.residual_s2,
        unexplained_alone.residual_s2,
        "residual_s2",
    );

    // Each attachment costs chars / 4 * (1.25 + 0.1); the tail is the smallest kinds.
    let tail_kinds = extra - (MAX_ROWS - 1);
    let tail_chars: usize = (0..tail_kinds).map(|index| 400 + index).sum();
    let other = both.rows.last().unwrap();
    assert_eq!(other.class, "other");
    assert_eq!(other.items as usize, tail_kinds);
    assert_close(other.s2, tail_chars as f64 / 4.0 * 1.35, "the merged tail");
}
