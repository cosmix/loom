//! Tests for the window-filtering defect: `build` used to count every
//! subagent transcript file, including ones the `--since` window filtered
//! down to zero in-window entries. Split out to keep `lifecycle.rs` legible
//! (CLAUDE.md Rule 17).

use super::*;
use crate::commands::usage::transcript::{Request, TokenUsage, UserEntry};

fn user_entry(text: &str) -> UserEntry {
    UserEntry {
        timestamp: chrono::Utc::now(),
        tool_use_id: None,
        text: text.to_owned(),
    }
}

fn request() -> Request {
    Request {
        message_id: None,
        timestamp: chrono::Utc::now(),
        model: "claude-sonnet-5".to_owned(),
        usage: TokenUsage::default(),
        tool_uses: Vec::new(),
        thinking_chars: 0,
        text_chars: 0,
        normalization: Default::default(),
    }
}

fn subagent(prompt: &str, in_window_requests: usize) -> Transcript {
    Transcript {
        path: std::path::PathBuf::from("test.jsonl"),
        scope: Scope::Subagent,
        project_slug: "project".to_owned(),
        project_path: None,
        session_id: "session-1".to_owned(),
        agent_id: Some("agent-1".to_owned()),
        agent_type: None,
        requested_model: None,
        stage_id: None,
        loom_session_id: None,
        forward_receipt: None,
        forward_candidate: false,
        first_user_entry: Some(user_entry(prompt)),
        entries: (0..in_window_requests)
            .map(|_| Entry::Assistant(Box::new(request())))
            .collect(),
        diagnostics: Default::default(),
    }
}

/// A transcript the `--since` window filtered down to zero in-window
/// entries must not be counted, even though its file is still present and
/// its (pre-window) first user entry is still available for classification.
#[test]
fn a_transcript_with_no_in_window_request_is_excluded() {
    let transcript = subagent("WORKER RESTRICTIONS - some brief", 0);
    let lifecycle = build(&[transcript]);
    assert_eq!(lifecycle.subagent_transcripts, 0);
}

/// A transcript with at least one in-window request is still counted and
/// classified from its first user entry as before.
#[test]
fn a_transcript_with_an_in_window_request_is_counted_and_classified() {
    let transcript = subagent("WORKER RESTRICTIONS - some brief", 1);
    let lifecycle = build(&[transcript]);
    assert_eq!(lifecycle.subagent_transcripts, 1);
    assert_eq!(lifecycle.classes.len(), 1);
    assert_eq!(lifecycle.classes[0].class, "worker preamble");
    assert_eq!(lifecycle.classes[0].transcripts, 1);
}

/// A mix of in-window and window-emptied transcripts counts only the ones
/// with surviving requests.
#[test]
fn only_in_window_transcripts_contribute_to_the_count() {
    let transcripts = vec![
        subagent("none", 0),
        subagent("none", 2),
        subagent("none", 0),
    ];
    let lifecycle = build(&transcripts);
    assert_eq!(lifecycle.subagent_transcripts, 1);
}

fn tool_request(tool: &str, input: u64) -> Entry {
    let mut item = request();
    item.usage = TokenUsage {
        input,
        output: 1,
        ..TokenUsage::default()
    };
    item.tool_uses
        .push(crate::commands::usage::transcript::ToolUse {
            id: "id".to_owned(),
            name: tool.to_owned(),
            input: serde_json::json!({}),
        });
    Entry::Assistant(Box::new(item))
}

#[test]
fn a_main_session_orients_until_its_first_spawn() {
    let mut main = subagent("none", 0);
    main.scope = Scope::Main;
    main.entries = vec![
        tool_request("Read", 10),
        tool_request("Grep", 20),
        tool_request("Agent", 30),
    ];
    let orientation = lifecycle_orientation::build(&[main]);
    assert_eq!(orientation.main.samples, 1);
    assert_eq!(orientation.main.median_requests, 2);
    assert_eq!(orientation.main.median_tokens, 32);
}

#[test]
fn read_only_subagents_are_skipped_and_never_ending_ones_counted() {
    let mut reviewer = subagent("none", 0);
    reviewer.agent_type = Some("loom-code-reviewer".to_owned());
    reviewer.entries = vec![tool_request("Read", 10)];
    let mut reader = subagent("none", 0);
    reader.entries = vec![tool_request("Read", 10)];
    let orientation = lifecycle_orientation::build(&[reviewer, reader]);
    assert_eq!(orientation.skipped_read_only, 1);
    assert_eq!(orientation.subagent.samples, 0);
    assert_eq!(orientation.subagent.without_end, 1);
}

#[test]
fn main_orientation_alone_counts_as_lifecycle_data() {
    let mut main = subagent("none", 0);
    main.scope = Scope::Main;
    main.entries = vec![tool_request("Read", 10), tool_request("Agent", 30)];
    let lifecycle = build(&[main]);
    assert_eq!(lifecycle.subagent_transcripts, 0);
    assert!(has_data(&lifecycle));
}

#[test]
fn an_empty_lifecycle_has_no_data() {
    assert!(!has_data(&build(&[])));
}
