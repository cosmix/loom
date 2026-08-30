//! Regression test for the `<synthetic>`-row pairing defect in
//! `collect_transcript`: a synthetic API-error request has all-zero usage,
//! so pairing it as the `current` side of a rewrite window used to turn the
//! ENTIRE prior request's residency into a phantom rewrite. Split out to
//! keep `rewrites.rs` legible (CLAUDE.md Rule 17).

use super::*;
use crate::commands::usage::transcript::{Request, TokenUsage};

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
    }
}

/// A synthetic row following a large-residency request must not be counted
/// as a rewrite: no model row, and no inflated totals.
#[test]
fn synthetic_row_following_a_large_residency_request_is_not_a_rewrite() {
    let previous = request("claude-sonnet-5", 20_000);
    let current = request(SYNTHETIC_MODEL, 0);
    let transcript = Transcript {
        path: std::path::PathBuf::from("test.jsonl"),
        scope: Scope::Main,
        project_slug: "project".to_owned(),
        session_id: "session-1".to_owned(),
        agent_id: None,
        first_user_entry: None,
        entries: vec![Entry::Assistant(previous), Entry::Assistant(current)],
    };

    let report = build(&[transcript]);
    assert_eq!(
        report.rewrites, 0,
        "a synthetic row must not count as a rewrite"
    );
    assert_eq!(report.total_tokens, 0, "totals must not be inflated");
    assert!(
        report.by_model.is_empty(),
        "no phantom <synthetic> model row should appear"
    );
}
