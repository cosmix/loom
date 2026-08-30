//! Tests for metadata extracted from parsed transcript entries. Kept apart
//! from `metrics.rs` so the implementation remains comfortably below the
//! command's per-file maintainability ceiling.

use super::*;

#[test]
fn counts_only_distinct_non_null_request_ids() {
    let entries = vec![
        serde_json::json!({
            "type": "assistant",
            "requestId": "request-1",
            "message": {"model": "claude-sonnet-5"},
        }),
        serde_json::json!({"type": "user", "requestId": null, "message": {}}),
        serde_json::json!({
            "type": "assistant",
            "requestId": "request-1",
            "message": {"model": "claude-opus-5"},
        }),
        serde_json::json!({
            "type": "assistant",
            "requestId": "request-2",
            "message": {},
        }),
    ];

    let metrics = extract(&entries);
    assert_eq!(metrics.request_count, 2);
    assert_eq!(metrics.model.as_deref(), Some("claude-sonnet-5"));
}

#[test]
fn peak_resident_tokens_includes_caches_and_reaches_ceiling() {
    let entries = vec![
        serde_json::json!({
            "type": "assistant",
            "message": {
                "usage": {
                    "input_tokens": 100_000,
                    "cache_creation_input_tokens": 20_000,
                    "cache_read_input_tokens": 30_000,
                    "output_tokens": 99_999,
                },
            },
        }),
        serde_json::json!({
            "type": "assistant",
            "message": {"usage": {"input_tokens": 149_999}},
        }),
    ];

    let metrics = extract(&entries);
    assert_eq!(metrics.peak_resident_tokens, Some(PEAK_TOKENS_CEILING));
}

/// An API-error/synthetic first assistant row with no `message.model` must
/// not latch the model as permanently unknown -- the first row that actually
/// names one should win.
#[test]
fn model_is_taken_from_first_assistant_row_that_names_one() {
    let entries = vec![
        serde_json::json!({"type": "assistant", "message": {}}),
        serde_json::json!({
            "type": "assistant",
            "message": {"model": "claude-opus-5"},
        }),
    ];

    let metrics = extract(&entries);
    assert_eq!(metrics.model.as_deref(), Some("claude-opus-5"));
}

/// A synthetic API-error row (Claude Code writes `model: "<synthetic>"` on an
/// entry that never reached the API) must not latch as the subagent's model
/// either -- same rule as an absent `message.model`, just a different shape.
#[test]
fn synthetic_error_row_does_not_latch_as_the_model() {
    let entries = vec![
        serde_json::json!({
            "type": "assistant",
            "message": {"model": "<synthetic>"},
        }),
        serde_json::json!({
            "type": "assistant",
            "message": {"model": "claude-sonnet-5"},
        }),
    ];

    let metrics = extract(&entries);
    assert_eq!(metrics.model.as_deref(), Some("claude-sonnet-5"));
}
