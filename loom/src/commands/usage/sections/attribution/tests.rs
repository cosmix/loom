//! Contracts for the attribution section: the cost definitions, startup
//! exclusion, attachment rows and the section's place in the report.

use std::path::PathBuf;

use chrono::{DateTime, TimeZone, Utc};
use serde_json::json;

use super::{build, Attribution, AttributionRow};
use crate::commands::usage::accounting::Windowing;
use crate::commands::usage::codex_scope::CodexScope;
use crate::commands::usage::completeness::SessionInventory;
use crate::commands::usage::discovery::DiscoveredFile;
use crate::commands::usage::time_range::TimeRange;
use crate::commands::usage::transcript_types::{
    AttachmentEntry, Entry, Request, RequestNormalization, Scope, TokenUsage, ToolUse, Transcript,
    UserEntry,
};
use crate::commands::usage::{provider, sections, transcript, ProviderSelection};

// A `static` because a scope with a set inside cannot be promoted to `'static`.
static UNSCOPED: CodexScope = CodexScope::Unscoped;

const SOURCE_READ: &str = "read:source-other";

fn at(minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 30, 0, minute, 0)
        .single()
        .unwrap()
}

fn usage(input: u64, output: u64) -> TokenUsage {
    TokenUsage {
        input,
        output,
        ..TokenUsage::default()
    }
}

fn request(minute: u32, usage: TokenUsage, tool_uses: Vec<ToolUse>) -> Entry {
    Entry::Assistant(Box::new(Request {
        message_id: Some(format!("m{minute}")),
        timestamp: at(minute),
        model: "claude-sonnet-5".to_owned(),
        usage,
        tool_uses,
        thinking_chars: 0,
        text_chars: 0,
        normalization: RequestNormalization {
            usage_observed: true,
            ..RequestNormalization::default()
        },
    }))
}

fn read_source(id: &str) -> Vec<ToolUse> {
    vec![ToolUse {
        id: id.to_owned(),
        name: "Read".to_owned(),
        input: json!({"file_path": "loom/src/x.rs"}),
    }]
}

fn tool_result(minute: u32, id: &str, chars: usize) -> Entry {
    Entry::User(UserEntry {
        timestamp: at(minute),
        tool_use_id: Some(id.to_owned()),
        text: "x".repeat(chars),
    })
}

fn attachment(requests_before: usize, kind: &str, chars: usize) -> AttachmentEntry {
    AttachmentEntry {
        requests_before,
        kind: kind.to_owned(),
        chars,
    }
}

/// A main transcript whose first request carries `fresh`, the rest
/// `Some(false)`, as `transcript::mark_fresh_start` leaves them.
fn transcript_of(
    fresh: Option<bool>,
    mut entries: Vec<Entry>,
    attachments: Vec<AttachmentEntry>,
) -> Transcript {
    let mut first = true;
    for entry in &mut entries {
        if let Entry::Assistant(request) = entry {
            request.normalization.first_observed_in_range = first;
            request.normalization.true_fresh_start = if first { fresh } else { Some(false) };
            first = false;
        }
    }
    Transcript {
        path: PathBuf::from("test.jsonl"),
        scope: Scope::Main,
        project_slug: "project".to_owned(),
        project_path: None,
        session_id: "session-1".to_owned(),
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

/// The three requests the totals contracts share: a first request that writes
/// to both cache tiers, then two that read.
fn three_requests() -> Vec<Entry> {
    let first = TokenUsage {
        input: 10,
        cache_creation: 100,
        cache_read: 0,
        output: 20,
        ephemeral_5m: 60,
        ephemeral_1h: 40,
    };
    let second = TokenUsage {
        cache_read: 110,
        ..usage(1, 30)
    };
    let third = TokenUsage {
        cache_read: 5000,
        ..usage(1, 50)
    };
    vec![
        request(1, first, Vec::new()),
        request(2, second, Vec::new()),
        request(3, third, Vec::new()),
    ]
}

fn assert_close(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "{label}: expected {expected}, got {actual}"
    );
}

fn row<'a>(attribution: &'a Attribution, class: &str) -> &'a AttributionRow {
    attribution
        .rows
        .iter()
        .find(|row| row.class == class)
        .unwrap_or_else(|| panic!("no {class} row in {:?}", attribution.rows))
}

fn has_row(attribution: &Attribution, class: &str) -> bool {
    attribution.rows.iter().any(|row| row.class == class)
}

#[test]
fn tool_result_counts_admission_and_replay() {
    let attribution = build(&[transcript_of(
        Some(true),
        vec![
            request(1, usage(10, 0), Vec::new()),
            request(2, usage(1, 0), read_source("t1")),
            tool_result(2, "t1", 400),
            request(3, usage(1, 0), Vec::new()),
            request(4, usage(1, 0), Vec::new()),
            request(5, usage(1, 0), Vec::new()),
        ],
        Vec::new(),
    )]);

    let read = row(&attribution, SOURCE_READ);
    assert_close(read.s2, 145.0, SOURCE_READ);
    assert_eq!(read.items, 1);
    assert_eq!(read.chars, 400);
}

#[test]
fn startup_attachments_are_not_double_counted() {
    let attribution = build(&[transcript_of(
        Some(true),
        vec![
            request(1, usage(10, 0), Vec::new()),
            request(2, usage(1, 0), Vec::new()),
            request(3, usage(1, 0), Vec::new()),
        ],
        vec![
            attachment(0, "skill_listing", 800),
            attachment(2, "hook_additional_context", 400),
        ],
    )]);

    assert_close(
        row(&attribution, "attach:hook_additional_context").s2,
        125.0,
        "attach:hook_additional_context",
    );
    assert!(!has_row(&attribution, "attach:skill_listing"));
    let main = attribution
        .startup_by_agent
        .iter()
        .find(|row| row.agent_type == "main")
        .expect("a startup row for the main session");
    assert_eq!(main.transcripts, 1);
    assert_eq!(main.median_resident, 10);
    let kinds: Vec<(&str, u64)> = main
        .kinds
        .iter()
        .map(|kind| (kind.kind.as_str(), kind.median_tokens))
        .collect();
    assert_eq!(kinds, vec![("skill_listing", 200)]);
}

#[test]
fn report_carries_attribution() {
    let normalized = provider::normalize_provider_events(provider::ProviderEventInput {
        selection: ProviderSelection::Claude,
        range: TimeRange {
            since: at(0),
            until: None,
        },
        claude_transcripts: Vec::new(),
        claude_discovered_files: 0,
        claude_missing_roots: 0,
        claude_sessions: SessionInventory::default(),
        codex_files: &[],
        codex_missing_roots: 0,
        codex_unreadable_directories: 0,
        codex_scope: &UNSCOPED,
        receipts_root: None,
        selected_stages: None,
        selection_unavailable: false,
    });
    let transcripts = [transcript_of(Some(true), three_requests(), Vec::new())];

    let report = sections::build(&transcripts, Windowing::FiveHour, normalized.ledger);

    let value = serde_json::to_value(report).unwrap();
    let total = value["attribution"]["total_s2"]
        .as_f64()
        .expect("attribution.total_s2 in the report");
    assert!(total > 0.0, "total_s2 was {total}");
}

#[test]
fn attribution_totals_follow_the_definitions() {
    let attribution = build(&[transcript_of(Some(true), three_requests(), Vec::new())]);

    assert_close(attribution.total_s2, 1178.0, "total_s2");
    assert_close(attribution.startup_s2, 187.0, "startup_s2");
    assert_close(attribution.output_s2, 500.0, "output_s2");
    assert_close(attribution.assistant_replay_s2, 79.5, "assistant_replay_s2");
    assert_close(attribution.residual_s2, 411.5, "residual_s2");
    assert_close(attribution.over_attributed_s2, 0.0, "over_attributed_s2");
    assert_eq!(attribution.startup_excluded, 0);
}

#[test]
fn windowed_transcript_has_no_startup() {
    let windowed = build(&[transcript_of(
        Some(false),
        three_requests(),
        vec![attachment(0, "skill_listing", 800)],
    )]);

    assert_close(windowed.startup_s2, 0.0, "startup_s2");
    assert_eq!(windowed.startup_excluded, 1);
    assert!(windowed.startup_by_agent.is_empty());
    assert!(!has_row(&windowed, "attach:skill_listing"));
    assert_close(windowed.residual_s2, 598.5, "residual_s2");

    let uncertain = build(&[transcript_of(None, three_requests(), Vec::new())]);

    assert_eq!(uncertain.startup_excluded, 1);
    assert_close(uncertain.startup_s2, 0.0, "startup_s2 when uncertain");
}

#[test]
fn terminal_result_costs_nothing() {
    let attribution = build(&[transcript_of(
        Some(true),
        vec![
            request(1, usage(10, 20), read_source("t1")),
            tool_result(1, "t1", 400),
        ],
        Vec::new(),
    )]);

    assert_close(attribution.total_s2, 110.0, "total_s2");
    assert_close(attribution.startup_s2, 10.0, "startup_s2");
    assert_close(attribution.output_s2, 100.0, "output_s2");
    assert_close(attribution.assistant_replay_s2, 0.0, "assistant_replay_s2");
    assert_close(attribution.residual_s2, 0.0, "residual_s2");
    assert_close(attribution.over_attributed_s2, 0.0, "over_attributed_s2");
    let rows: f64 = attribution.rows.iter().map(|row| row.s2).sum();
    assert_close(rows, 0.0, "rows");
}

#[test]
fn over_attribution_is_reported() {
    let attribution = build(&[transcript_of(
        Some(true),
        vec![
            request(1, usage(10, 0), read_source("t1")),
            tool_result(1, "t1", 4000),
            request(2, usage(1, 0), Vec::new()),
        ],
        Vec::new(),
    )]);

    assert_close(attribution.total_s2, 11.0, "total_s2");
    assert_close(attribution.startup_s2, 11.0, "startup_s2");
    assert_close(row(&attribution, SOURCE_READ).s2, 1250.0, SOURCE_READ);
    assert_close(attribution.residual_s2, 0.0, "residual_s2");
    assert_close(attribution.over_attributed_s2, 1250.0, "over_attributed_s2");
}

fn assistant_row(timestamp: &str, id: &str) -> String {
    json!({
        "type": "assistant",
        "timestamp": timestamp,
        "message": {
            "id": id,
            "model": "claude-sonnet-5",
            "content": [{"type": "text", "text": "ok"}],
            "usage": {"input_tokens": 1}
        }
    })
    .to_string()
}

fn attachment_row(timestamp: &str, kind: &str, content: &str) -> String {
    json!({
        "type": "attachment",
        "timestamp": timestamp,
        "attachment": {"type": kind},
        "rendered": [{"content": content}]
    })
    .to_string()
}

#[test]
fn attachment_rows_count_requests_once_and_honour_since() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("s1.jsonl");
    let snapshot = json!({
        "type": "attachment",
        "timestamp": "2026-08-30T00:04:00Z",
        "attachment": {"type": "prompt_snapshot", "text": "never sent"}
    })
    .to_string();
    let rows = [
        assistant_row("2026-08-28T00:00:00Z", "m0"),
        attachment_row("2026-08-28T00:01:00Z", "old_context", "older"),
        assistant_row("2026-08-30T00:01:00Z", "m1"),
        assistant_row("2026-08-30T00:02:00Z", "m1"),
        attachment_row("2026-08-30T00:03:00Z", "hook_additional_context", "abcd"),
        snapshot,
    ];
    std::fs::write(&path, format!("{}\n", rows.join("\n"))).unwrap();
    let file = DiscoveredFile {
        path,
        project_slug: "project".to_owned(),
        scope: Scope::Main,
        session_id: "s1".to_owned(),
        agent_id: None,
    };
    let range = TimeRange {
        since: Utc.with_ymd_and_hms(2026, 8, 29, 0, 0, 0).single().unwrap(),
        until: None,
    };

    let parsed = transcript::parse(&file, &range).unwrap();

    assert_eq!(parsed.attachments.len(), 1, "{:?}", parsed.attachments);
    let only = &parsed.attachments[0];
    assert_eq!(only.requests_before, 1);
    assert_eq!(only.kind, "hook_additional_context");
    assert_eq!(only.chars, 4);
}
