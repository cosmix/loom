//! JSONL-entry parsing and structural liveness classification for `classify`.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::SubagentState;

/// Read bytes so a torn UTF-8 tail becomes a single skipped JSONL row rather
/// than making the whole transcript unreadable.
pub(super) fn read_entries(path: &Path) -> Result<Vec<Value>> {
    let bytes = fs::read(path)
        .with_context(|| format!("reading subagent transcript {}", path.display()))?;
    let content = String::from_utf8_lossy(&bytes);
    Ok(parse_lines(&content))
}

/// Parse every non-blank JSONL row independently, discarding malformed rows.
fn parse_lines(content: &str) -> Vec<Value> {
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

pub(super) fn timestamp(entry: &Value) -> Option<DateTime<Utc>> {
    entry
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

pub(super) fn is_assistant(entry: &Value) -> bool {
    entry_type(entry) == Some("assistant")
}

pub(super) fn text_blocks(entry: &Value) -> Vec<&str> {
    message_content_blocks(entry)
        .into_iter()
        .filter(|block| block_type(block) == Some("text"))
        .filter_map(block_text)
        .collect()
}

/// Classify one entry using the frozen table in the parent module. This never
/// applies the done debounce; that depends on transcript idle time.
pub(super) fn classify_last(entry: &Value) -> SubagentState {
    match entry_type(entry) {
        Some("assistant") => classify_assistant(message_content_blocks(entry)),
        Some("user") => SubagentState::Generating,
        _ => SubagentState::Unknown,
    }
}

fn classify_assistant(blocks: Vec<&Value>) -> SubagentState {
    if blocks
        .iter()
        .any(|block| block_type(block) == Some("tool_use"))
    {
        SubagentState::ToolWait
    } else if blocks.iter().any(|block| block_type(block) == Some("text")) {
        SubagentState::Done
    } else if blocks
        .iter()
        .any(|block| block_type(block) == Some("thinking"))
    {
        SubagentState::Generating
    } else {
        SubagentState::Unknown
    }
}

/// Return the most recent tool call even when a later user result exists.
pub(super) fn last_tool_used(entries: &[Value]) -> Option<String> {
    entries.iter().rev().find_map(|entry| {
        (entry_type(entry) == Some("assistant"))
            .then(|| message_content_blocks(entry))?
            .into_iter()
            .rev()
            .find(|block| block_type(block) == Some("tool_use"))
            .and_then(block_tool_name)
            .map(str::to_string)
    })
}

fn entry_type(entry: &Value) -> Option<&str> {
    entry.get("type").and_then(Value::as_str)
}

fn message_content_blocks(entry: &Value) -> Vec<&Value> {
    entry
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .map(|blocks| blocks.iter().collect())
        .unwrap_or_default()
}

fn block_type(block: &Value) -> Option<&str> {
    block.get("type").and_then(Value::as_str)
}

fn block_text(block: &Value) -> Option<&str> {
    block.get("text").and_then(Value::as_str)
}

fn block_tool_name(block: &Value) -> Option<&str> {
    block.get("name").and_then(Value::as_str)
}
