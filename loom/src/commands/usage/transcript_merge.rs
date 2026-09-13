use super::transcript::Request;

pub(super) fn merge_request(first: &mut Request, duplicate: Request) {
    merge_usage_observation(first, &duplicate);
    first.tool_uses.extend(duplicate.tool_uses);
    first.thinking_chars += duplicate.thinking_chars;
    first.text_chars += duplicate.text_chars;
}

fn merge_usage_observation(first: &mut Request, duplicate: &Request) {
    if !duplicate.normalization.usage_observed {
        return;
    }
    first.normalization.usage_observations += duplicate.normalization.usage_observations;
    record_first_usage(first, duplicate);
    let current_key = (first.timestamp, first.normalization.line_ordinal);
    let duplicate_key = (duplicate.timestamp, duplicate.normalization.line_ordinal);
    if !first.normalization.usage_observed || duplicate_key >= current_key {
        first.usage = duplicate.usage;
        first.timestamp = duplicate.timestamp;
        first.model.clone_from(&duplicate.model);
        first.normalization.thinking_output_tokens = duplicate.normalization.thinking_output_tokens;
        first.normalization.invalid_thinking_output =
            duplicate.normalization.invalid_thinking_output;
        first.normalization.invalid_usage = duplicate.normalization.invalid_usage;
        first.normalization.invalid_cache_relation = duplicate.normalization.invalid_cache_relation;
        first.normalization.line_ordinal = duplicate.normalization.line_ordinal;
    }
    first.normalization.usage_observed = true;
    first.normalization.changed_usage_fields = changed_usage_fields(first);
}

fn record_first_usage(first: &mut Request, duplicate: &Request) {
    let current = first
        .normalization
        .first_usage_timestamp
        .map(|stamp| (stamp, first.normalization.first_usage_ordinal));
    let incoming = duplicate
        .normalization
        .first_usage_timestamp
        .map(|stamp| (stamp, duplicate.normalization.first_usage_ordinal));
    if incoming.is_some() && (current.is_none() || incoming < current) {
        first.normalization.first_usage = duplicate.normalization.first_usage;
        first.normalization.first_thinking_output_tokens =
            duplicate.normalization.first_thinking_output_tokens;
        first.normalization.first_usage_timestamp = duplicate.normalization.first_usage_timestamp;
        first.normalization.first_usage_ordinal = duplicate.normalization.first_usage_ordinal;
    }
}

fn changed_usage_fields(request: &Request) -> usize {
    let Some(first) = request.normalization.first_usage else {
        return 0;
    };
    let current = request.usage;
    [
        first.input != current.input,
        first.cache_creation != current.cache_creation,
        first.cache_read != current.cache_read,
        first.output != current.output,
        first.ephemeral_5m != current.ephemeral_5m,
        first.ephemeral_1h != current.ephemeral_1h,
        request.normalization.first_thinking_output_tokens
            != request.normalization.thinking_output_tokens,
    ]
    .into_iter()
    .filter(|changed| *changed)
    .count()
}
