use chrono::{DateTime, Utc};
use serde_json::Value;

use super::transcript::{ToolUse, UserEntry};

pub(super) fn content_counts(blocks: &[Value]) -> (Vec<ToolUse>, usize, usize) {
    let mut tools = Vec::new();
    let mut thinking_chars = 0;
    let mut text_chars = 0;
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => text_chars += string_len(block, "text"),
            Some("thinking") => thinking_chars += string_len(block, "thinking"),
            Some("tool_use") => tools.extend(tool_use(block)),
            _ => {}
        }
    }
    (tools, thinking_chars, text_chars)
}

fn tool_use(block: &Value) -> Option<ToolUse> {
    Some(ToolUse {
        id: block.get("id")?.as_str()?.to_owned(),
        name: block.get("name")?.as_str()?.to_owned(),
        input: block.get("input").cloned().unwrap_or(Value::Null),
    })
}

fn string_len(value: &Value, field: &str) -> usize {
    value
        .get(field)
        .and_then(Value::as_str)
        .map_or(0, |text| text.chars().count())
}

pub(super) fn user_entries(value: &Value, timestamp: DateTime<Utc>) -> Vec<UserEntry> {
    match value.pointer("/message/content") {
        Some(Value::String(text)) => vec![user_entry(timestamp, None, text.clone())],
        Some(Value::Array(blocks)) => tool_results(blocks, timestamp),
        _ => Vec::new(),
    }
}

fn tool_results(blocks: &[Value], timestamp: DateTime<Utc>) -> Vec<UserEntry> {
    let results: Vec<_> = blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
        .map(|block| {
            user_entry(
                timestamp,
                block
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                tool_text(block),
            )
        })
        .collect();
    if results.is_empty() {
        plain_array_entry(blocks, timestamp).into_iter().collect()
    } else {
        results
    }
}

fn plain_array_entry(blocks: &[Value], timestamp: DateTime<Utc>) -> Option<UserEntry> {
    let text: String = blocks
        .iter()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect();
    (!text.is_empty()).then(|| user_entry(timestamp, None, text))
}

fn tool_text(block: &Value) -> String {
    match block.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect(),
        _ => String::new(),
    }
}

fn user_entry(timestamp: DateTime<Utc>, tool_use_id: Option<String>, text: String) -> UserEntry {
    UserEntry {
        timestamp,
        tool_use_id,
        text,
    }
}
