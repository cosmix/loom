//! Parser for token-usage lines of a Claude transcript.

use crate::tokens::model::TokenUsage;
use chrono::DateTime;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeLine {
    Usage {
        message_id: Option<String>,
        timestamp: i64,
        model: Option<String>,
        effort: Option<String>,
        usage: TokenUsage,
    },
    Other,
    Unreadable,
}

/// Extract usage only from assistant messages with a usage object.
pub fn parse_line(line: &str) -> ClaudeLine {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) if line.contains("\"usage\"") => return ClaudeLine::Unreadable,
        Err(_) => return ClaudeLine::Other,
    };
    if value.get("type").and_then(Value::as_str) != Some("assistant") {
        return ClaudeLine::Other;
    }
    let message = &value["message"];
    let Some(fields) = message.get("usage").filter(|value| value.is_object()) else {
        return ClaudeLine::Other;
    };
    let Some(usage) = parse_usage(fields) else {
        return ClaudeLine::Unreadable;
    };
    let Some(timestamp) = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
    else {
        return ClaudeLine::Unreadable;
    };
    ClaudeLine::Usage {
        message_id: message.get("id").and_then(Value::as_str).map(str::to_owned),
        timestamp: timestamp.timestamp(),
        model: message
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned),
        effort: ["perTurnEffort", "effort"]
            .iter()
            .find_map(|key| value.get(*key).and_then(Value::as_str))
            .map(str::to_owned),
        usage,
    }
}

fn parse_usage(fields: &Value) -> Option<TokenUsage> {
    if crate::provider_data::claude::invalid_usage(fields) {
        return None;
    }
    let (usage, _) = crate::provider_data::claude::parse(fields);
    Some(TokenUsage {
        input: usage.input,
        cache_write: usage.cache_creation,
        cache_read: usage.cache_read,
        output: usage.output,
    })
}

#[cfg(test)]
#[path = "claude_line_tests.rs"]
mod tests;
