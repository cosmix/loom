//! Extracts request and context metrics from already-parsed transcript rows.
//! Keeping this separate from classification makes the liveness rules remain
//! focused while avoiding a second read of a concurrently appended transcript.

use std::collections::BTreeSet;

use serde_json::Value;

/// 75% of a 200k context window, matching the 75% hard stop that CLAUDE.md
/// rule 3 puts on context usage.
pub const PEAK_TOKENS_CEILING: u64 = 150_000;

/// Metrics that describe the requests represented by one subagent transcript.
#[derive(Debug, Default)]
pub(super) struct TranscriptMetrics {
    pub(super) model: Option<String>,
    pub(super) request_count: usize,
    pub(super) peak_resident_tokens: Option<u64>,
}

/// Extract all display metadata in one pass over the parsed rows. A row with
/// a `usage` object but absent token fields is meaningful zero usage; no usage
/// object on any assistant row stays `None` so the table can render `-`.
pub(super) fn extract(entries: &[Value]) -> TranscriptMetrics {
    let mut metrics = TranscriptMetrics::default();
    let mut request_ids = BTreeSet::new();

    for entry in entries {
        if let Some(request_id) = request_id(entry) {
            request_ids.insert(request_id);
        }
        if entry.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        if metrics.model.is_none() {
            metrics.model = entry
                .get("message")
                .and_then(|message| message.get("model"))
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if let Some(tokens) = resident_tokens(entry) {
            metrics.peak_resident_tokens =
                Some(metrics.peak_resident_tokens.unwrap_or(0).max(tokens));
        }
    }
    metrics.request_count = request_ids.len();
    metrics
}

fn request_id(entry: &Value) -> Option<String> {
    entry
        .get("requestId")
        .filter(|id| !id.is_null())
        .map(Value::to_string)
}

fn resident_tokens(entry: &Value) -> Option<u64> {
    let usage = entry.get("message")?.get("usage")?.as_object()?;
    let fields = [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
    ];
    Some(fields.into_iter().fold(0, |total, field| {
        total.saturating_add(usage.get(field).and_then(Value::as_u64).unwrap_or(0))
    }))
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
