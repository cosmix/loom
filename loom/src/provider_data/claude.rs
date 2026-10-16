use serde_json::Value;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ClaudeUsage {
    pub input: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
    pub output: u64,
    pub ephemeral_5m: u64,
    pub ephemeral_1h: u64,
}

pub(crate) fn parse(value: &Value) -> (ClaudeUsage, bool) {
    let creation = number(value, "cache_creation_input_tokens");
    let (ephemeral_5m, ephemeral_1h) = cache_split(value, creation);
    let (cache_creation, ephemeral_5m, ephemeral_1h, invalid_cache_relation) =
        reconcile_cache_creation(creation, ephemeral_5m, ephemeral_1h);
    let usage = ClaudeUsage {
        input: number(value, "input_tokens"),
        cache_creation,
        cache_read: number(value, "cache_read_input_tokens"),
        output: number(value, "output_tokens"),
        ephemeral_5m,
        ephemeral_1h,
    };
    (usage, invalid_cache_relation)
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

pub(crate) fn invalid_usage(value: &Value) -> bool {
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
    value.get(field).is_some_and(|token| {
        token.as_u64().is_none()
            && (!token.is_null() || matches!(field, "input_tokens" | "output_tokens"))
    })
}
