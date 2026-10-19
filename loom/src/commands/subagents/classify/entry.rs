//! JSONL-entry parsing and structural liveness classification for `classify`.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
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

/// Most bytes read from the end of a transcript by [`read_tail_entries`].
///
/// Deliberate limit: a final entry larger than this (a `Write` of a huge file)
/// is dropped with the partial line, so the tail then ends on the entry before
/// it. Revisit if open-tool detection misses calls that carry large inputs.
const TAIL_BYTES: u64 = 512 * 1024;

/// Parse the rows in the last [`TAIL_BYTES`] of an open transcript. The first
/// row of a tail that starts mid-file is cut, so it is discarded.
pub(super) fn read_tail_entries(file: &mut File) -> Result<Vec<Value>> {
    let length = file.metadata().context("inspecting transcript")?.len();
    let start = length.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))
        .context("seeking transcript")?;
    let mut bytes = Vec::new();
    file.take(TAIL_BYTES)
        .read_to_end(&mut bytes)
        .context("reading transcript tail")?;
    let whole_rows = match (start, bytes.iter().position(|byte| *byte == b'\n')) {
        (0, _) => &bytes[..],
        (_, Some(newline)) => &bytes[newline + 1..],
        (_, None) => &[],
    };
    Ok(parse_lines(&String::from_utf8_lossy(whole_rows)))
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

pub(crate) fn is_assistant(entry: &Value) -> bool {
    entry_type(entry) == Some("assistant")
}

/// A conversation turn (`assistant` or `user`), as opposed to the bookkeeping
/// rows (`attachment`, `system`, `last-prompt`, ...) Claude Code interleaves.
pub(super) fn is_message(entry: &Value) -> bool {
    matches!(entry_type(entry), Some("assistant" | "user"))
}

pub(crate) fn text_blocks(entry: &Value) -> Vec<&str> {
    message_content_blocks(entry)
        .into_iter()
        .filter(|block| block_type(block) == Some("text"))
        .filter_map(block_text)
        .collect()
}

/// Classify one entry using the frozen table in the parent module. This emits
/// only transcript states: lifecycle-only `Failed` and `Cancelled` are
/// applied after exact identity lookup, and the done debounce is applied by
/// the parent classifier.
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
