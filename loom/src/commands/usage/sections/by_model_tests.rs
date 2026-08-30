//! Tests for `<synthetic>`-model filtering in the by-model breakdown. Split
//! out to keep `by_model.rs` legible (CLAUDE.md Rule 17).

use super::*;
use crate::commands::usage::transcript::{Entry, Request, TokenUsage};

fn request(model: &str) -> Request {
    Request {
        message_id: None,
        timestamp: chrono::Utc::now(),
        model: model.to_owned(),
        usage: TokenUsage::default(),
        tool_uses: Vec::new(),
        thinking_chars: 0,
        text_chars: 0,
    }
}

fn transcript(models: &[&str]) -> Transcript {
    Transcript {
        path: std::path::PathBuf::from("test.jsonl"),
        scope: Scope::Subagent,
        project_slug: "project".to_owned(),
        session_id: "session-1".to_owned(),
        agent_id: Some("agent-1".to_owned()),
        agent_type: None,
        first_user_entry: None,
        entries: models
            .iter()
            .map(|m| Entry::Assistant(request(m)))
            .collect(),
    }
}

/// A synthetic API-error row (Claude Code's `"<synthetic>"` model for an
/// entry with `isApiErrorMessage: true`) must not create a phantom
/// `<synthetic>` row in the by-model breakdown.
#[test]
fn synthetic_rows_are_excluded_from_the_model_breakdown() {
    let transcripts = vec![transcript(&[SYNTHETIC_MODEL, "claude-sonnet-5"])];
    let report = build(&transcripts);
    assert_eq!(report.rows.len(), 1);
    assert_eq!(report.rows[0].model, "claude-sonnet-5");
    assert_eq!(report.rows[0].requests, 1);
}
