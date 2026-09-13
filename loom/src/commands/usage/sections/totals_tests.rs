//! Regression test for the `<synthetic>`-request defect in `build`: the
//! report's own `requests` count must exclude a synthetic API-error row, the
//! same as `by_model`/`by_agent_model`, so the breakdown rows still sum back
//! to this total. Split out to keep `totals.rs` legible (CLAUDE.md Rule 17).

use super::*;
use crate::commands::usage::transcript::{Entry, Request, Scope};

fn request(model: &str, resident_input: u64) -> Request {
    Request {
        message_id: None,
        timestamp: chrono::Utc::now(),
        model: model.to_owned(),
        usage: TokenUsage {
            input: resident_input,
            ..TokenUsage::default()
        },
        tool_uses: Vec::new(),
        thinking_chars: 0,
        text_chars: 0,
        normalization: Default::default(),
    }
}

#[test]
fn totals_excludes_synthetic_requests_from_the_report_count() {
    let real = request("claude-sonnet-5", 100);
    let synthetic = request(SYNTHETIC_MODEL, 0);
    let transcript = Transcript {
        path: std::path::PathBuf::from("test.jsonl"),
        scope: Scope::Main,
        project_slug: "project".to_owned(),
        project_path: None,
        session_id: "session-1".to_owned(),
        agent_id: None,
        agent_type: None,
        stage_id: None,
        loom_session_id: None,
        forward_receipt: None,
        forward_candidate: false,
        first_user_entry: None,
        entries: vec![
            Entry::Assistant(Box::new(real)),
            Entry::Assistant(Box::new(synthetic)),
        ],
        diagnostics: Default::default(),
    };

    let totals = build(&[transcript]);
    assert_eq!(
        totals.requests, 1,
        "a synthetic row must not count as a request"
    );
    assert_eq!(totals.fresh_input, 100);
}

#[test]
fn measured_thinking_subfield_does_not_change_legacy_totals_shape() {
    let mut measured = request("claude-sonnet-5", 100);
    measured.usage.output = 10;
    measured.thinking_chars = 20;
    measured.normalization.thinking_output_tokens = Some(4);
    let transcript = Transcript {
        path: std::path::PathBuf::from("test.jsonl"),
        scope: Scope::Main,
        project_slug: "project".to_owned(),
        project_path: None,
        session_id: "session-1".to_owned(),
        agent_id: None,
        agent_type: None,
        stage_id: None,
        loom_session_id: None,
        forward_receipt: None,
        forward_candidate: false,
        first_user_entry: None,
        entries: vec![Entry::Assistant(Box::new(measured))],
        diagnostics: Default::default(),
    };

    let totals = build(&[transcript]);

    assert_eq!(totals.output, 10);
    assert_eq!(totals.thinking_tokens_estimate, 5);
}
