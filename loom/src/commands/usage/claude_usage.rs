use chrono::{DateTime, Utc};
use serde_json::Value;

use super::transcript::{RequestNormalization, TokenUsage};

pub(super) fn parse(
    value: Option<&Value>,
    timestamp: DateTime<Utc>,
    ordinal: usize,
) -> (TokenUsage, RequestNormalization) {
    let Some(value) = value else {
        return (TokenUsage::default(), RequestNormalization::default());
    };
    let (normalized, invalid_cache_relation) = crate::provider_data::claude::parse(value);
    let usage = TokenUsage {
        input: normalized.input,
        cache_creation: normalized.cache_creation,
        cache_read: normalized.cache_read,
        output: normalized.output,
        ephemeral_5m: normalized.ephemeral_5m,
        ephemeral_1h: normalized.ephemeral_1h,
    };
    (
        usage,
        normalization(value, usage, timestamp, ordinal, invalid_cache_relation),
    )
}

fn normalization(
    value: &Value,
    usage: TokenUsage,
    timestamp: DateTime<Utc>,
    ordinal: usize,
    invalid_cache_relation: bool,
) -> RequestNormalization {
    let thinking_value = value.pointer("/output_tokens_details/thinking_tokens");
    let raw_thinking = thinking_value.and_then(Value::as_u64);
    let invalid_thinking_output = thinking_value.is_some_and(|raw| raw.as_u64().is_none())
        || raw_thinking.is_some_and(|thinking| thinking > usage.output);
    let thinking_output_tokens = if invalid_thinking_output {
        None
    } else {
        raw_thinking
    };
    RequestNormalization {
        usage_observed: true,
        first_usage: Some(usage),
        first_thinking_output_tokens: thinking_output_tokens,
        usage_observations: 1,
        thinking_output_tokens,
        invalid_thinking_output,
        invalid_usage: crate::provider_data::claude::invalid_usage(value),
        invalid_cache_relation,
        line_ordinal: ordinal,
        first_usage_timestamp: Some(timestamp),
        first_usage_ordinal: ordinal,
        ..RequestNormalization::default()
    }
}
