use serde_json::Value;

use super::provider_types::{ProviderDiagnostics, ProviderTokenVector};

pub(super) fn token_vector(
    value: &Value,
    diagnostics: &mut ProviderDiagnostics,
) -> ProviderTokenVector {
    if has_invalid_tokens(value) {
        diagnostics.invalid_token_rows += 1;
    }
    let input = number(value, "input_tokens");
    let cached = number(value, "cached_input_tokens").or_else(|| {
        value
            .pointer("/input_tokens_details/cached_tokens")
            .and_then(Value::as_u64)
    });
    let output = number(value, "output_tokens");
    let reasoning = number(value, "reasoning_output_tokens").or_else(|| {
        value
            .pointer("/output_tokens_details/reasoning_tokens")
            .and_then(Value::as_u64)
    });
    if value
        .get("reasoning_output_tokens")
        .or_else(|| value.pointer("/output_tokens_details/reasoning_tokens"))
        .is_some_and(|token| token.as_u64().is_none())
    {
        diagnostics.invalid_thinking_rows += 1;
    }
    let thinking = valid_reasoning(reasoning, output, diagnostics);
    let total = number(value, "total_tokens");
    if total_mismatch(total, input, output) {
        diagnostics.total_token_mismatches += 1;
    }
    let fresh = input
        .zip(cached)
        .and_then(|(all, hit)| all.checked_sub(hit));
    if input.zip(cached).is_some() && fresh.is_none() {
        diagnostics.invalid_cache_relations += 1;
    }
    ProviderTokenVector {
        input_tokens: input,
        fresh_input_tokens: fresh,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: cached,
        cache_write_5m_input_tokens: None,
        cache_write_1h_input_tokens: None,
        output_tokens: output,
        thinking_output_tokens: thinking,
        resident_input_tokens: input,
        total_tokens: total,
    }
}

fn has_invalid_tokens(value: &Value) -> bool {
    [
        "input_tokens",
        "cached_input_tokens",
        "output_tokens",
        "total_tokens",
    ]
    .into_iter()
    .any(|field| {
        value
            .get(field)
            .is_some_and(|token| token.as_u64().is_none())
    }) || value
        .pointer("/input_tokens_details/cached_tokens")
        .is_some_and(|token| token.as_u64().is_none())
}

fn number(value: &Value, field: &str) -> Option<u64> {
    value.get(field).and_then(Value::as_u64)
}

fn valid_reasoning(
    reasoning: Option<u64>,
    output: Option<u64>,
    diagnostics: &mut ProviderDiagnostics,
) -> Option<u64> {
    if reasoning
        .zip(output)
        .is_some_and(|(reasoning, output)| reasoning > output)
    {
        diagnostics.invalid_thinking_rows += 1;
        None
    } else {
        reasoning
    }
}

fn total_mismatch(total: Option<u64>, input: Option<u64>, output: Option<u64>) -> bool {
    total
        .zip(input.zip(output))
        .is_some_and(|(total, (input, output))| input.checked_add(output) != Some(total))
}
