//! Tests for synthetic-model filtering in the model breakdown.

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
        normalization: Default::default(),
    }
}

fn transcript(models: &[&str]) -> Transcript {
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
        first_user_entry: None,
        entries: models
            .iter()
            .map(|m| Entry::Assistant(Box::new(request(m))))
            .collect(),
        diagnostics: Default::default(),
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

#[test]
fn mixed_models_accumulate_and_keep_flat_json_fields() {
    let mut first = transcript(&["sonnet", "sonnet", "haiku"]);
    for (index, entry) in first.entries.iter_mut().enumerate() {
        let Entry::Assistant(request) = entry else {
            panic!("request fixture")
        };
        request.usage.input = (index as u64 + 1) * 10;
        request.usage.output = index as u64 + 1;
    }
    let report = build(&[first]);
    let sonnet = report
        .rows
        .iter()
        .find(|row| row.model == "sonnet")
        .unwrap();
    assert_eq!(sonnet.requests, 2);
    assert_eq!(sonnet.summary.usage.input, 30);
    assert_eq!(sonnet.summary.accounting.s1, 33);
    assert_eq!(sonnet.summary.accounting.s2, 45.0);
    let json = serde_json::to_value(sonnet).unwrap();
    assert_eq!(json["fresh_input"], 30);
    assert_eq!(json["s2"], 45.0);
    assert!(json.get("summary").is_none());
}
