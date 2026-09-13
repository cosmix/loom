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
    let creation = number(value, "cache_creation_input_tokens");
    let (ephemeral_5m, ephemeral_1h) = cache_split(value, creation);
    let (cache_creation, ephemeral_5m, ephemeral_1h, invalid_cache_relation) =
        reconcile_cache_creation(creation, ephemeral_5m, ephemeral_1h);
    let usage = TokenUsage {
        input: number(value, "input_tokens"),
        cache_creation,
        cache_read: number(value, "cache_read_input_tokens"),
        output: number(value, "output_tokens"),
        ephemeral_5m,
        ephemeral_1h,
    };
    (
        usage,
        normalization(value, usage, timestamp, ordinal, invalid_cache_relation),
    )
}

fn cache_split(value: &Value, creation: u64) -> (u64, u64) {
    match value.get("cache_creation") {
        Some(split) => (
            number(split, "ephemeral_5m_input_tokens"),
            number(split, "ephemeral_1h_input_tokens"),
        ),
        None => (creation, 0),
    }
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
        invalid_usage: invalid_usage(value),
        invalid_cache_relation,
        line_ordinal: ordinal,
        first_usage_timestamp: Some(timestamp),
        first_usage_ordinal: ordinal,
        ..RequestNormalization::default()
    }
}

fn reconcile_cache_creation(
    creation: u64,
    ephemeral_5m: u64,
    ephemeral_1h: u64,
) -> (u64, u64, u64, bool) {
    let Some(split_total) = ephemeral_5m.checked_add(ephemeral_1h) else {
        return (creation, creation, 0, true);
    };
    if creation == 0 && split_total != 0 {
        return (split_total, ephemeral_5m, ephemeral_1h, false);
    }
    if split_total != creation {
        return (creation, creation, 0, true);
    }
    (creation, ephemeral_5m, ephemeral_1h, false)
}

fn number(value: &Value, field: &str) -> u64 {
    value.get(field).and_then(Value::as_u64).unwrap_or(0)
}

fn invalid_usage(value: &Value) -> bool {
    let flat = [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
        "output_tokens",
    ];
    flat.into_iter().any(|field| invalid_number(value, field))
        || value.get("cache_creation").is_some_and(|split| {
            ["ephemeral_5m_input_tokens", "ephemeral_1h_input_tokens"]
                .into_iter()
                .any(|field| invalid_number(split, field))
        })
}

fn invalid_number(value: &Value, field: &str) -> bool {
    value
        .get(field)
        .is_some_and(|token| token.as_u64().is_none())
}
