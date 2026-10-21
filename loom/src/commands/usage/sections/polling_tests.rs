//! Tests for observation-only request accounting in `polling.rs`.

use super::*;
use crate::commands::usage::transcript::{Entry, Scope, TokenUsage};

fn request(tools: &[(&str, &str)], input: u64) -> Entry {
    Entry::Assistant(Box::new(Request {
        message_id: None,
        timestamp: chrono::Utc::now(),
        model: "claude-sonnet-5".to_owned(),
        usage: TokenUsage {
            input,
            output: 7,
            ..TokenUsage::default()
        },
        tool_uses: tools
            .iter()
            .map(|(name, command)| ToolUse {
                id: "id".to_owned(),
                name: (*name).to_owned(),
                input: serde_json::json!({ "command": command }),
            })
            .collect(),
        thinking_chars: 0,
        text_chars: 0,
        normalization: Default::default(),
    }))
}

fn transcript(entries: Vec<Entry>) -> Transcript {
    Transcript {
        path: std::path::PathBuf::from("test.jsonl"),
        scope: Scope::Main,
        project_slug: "project".to_owned(),
        project_path: None,
        session_id: "session-1".to_owned(),
        agent_id: None,
        agent_type: None,
        requested_model: None,
        stage_id: None,
        loom_session_id: None,
        forward_receipt: None,
        forward_candidate: false,
        first_user_entry: None,
        entries,
        diagnostics: Default::default(),
    }
}

fn kind_of<'a>(polling: &'a Polling, kind: &str) -> Option<&'a ObservationKind> {
    polling.observation.by_kind.iter().find(|k| k.kind == kind)
}

#[test]
fn wait_and_monitor_requests_are_counted_with_resident_tokens() {
    let polling = build(&[transcript(vec![
        request(&[("Bash", "loom subagents watch --all")], 100),
        request(&[("Monitor", "")], 40),
    ])]);
    assert_eq!(polling.observation.requests, 2);
    assert_eq!(polling.observation.resident_input_tokens, 140);
    assert_eq!(polling.observation.output_tokens, 14);
    assert_eq!(
        kind_of(&polling, "subagents-wait")
            .unwrap()
            .resident_input_tokens,
        100
    );
    assert_eq!(kind_of(&polling, "monitor").unwrap().requests, 1);
    assert_eq!(polling.observation.by_kind[0].kind, "subagents-wait");
}

#[test]
fn a_request_that_also_edits_is_not_observation_only() {
    let polling = build(&[transcript(vec![request(
        &[("Monitor", ""), ("Edit", "")],
        100,
    )])]);
    assert_eq!(polling.observation.requests, 0);
}

#[test]
fn a_command_repeated_three_times_counts_two_times_does_not() {
    let thrice = || request(&[("Bash", "cargo test foo")], 10);
    let counted = build(&[transcript(vec![thrice(), thrice(), thrice()])]);
    assert_eq!(kind_of(&counted, "repeated").unwrap().requests, 3);
    let twice = build(&[transcript(vec![thrice(), thrice()])]);
    assert_eq!(twice.observation.requests, 0);
}

#[test]
fn observation_tokens_alone_make_the_section_have_data() {
    let waited = build(&[transcript(vec![request(&[("Bash", "sleep 5")], 10)])]);
    assert!(has_data(&waited));
    let observed_only = Polling {
        repeated_commands: Vec::new(),
        sleep_invocations: 0,
        total_sleep_seconds: 0.0,
        observation: ObservationTokens {
            requests: 1,
            ..ObservationTokens::default()
        },
    };
    assert!(has_data(&observed_only));
    assert!(!has_data(&build(&[])));
}
