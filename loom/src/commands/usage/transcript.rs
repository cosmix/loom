//! Reads Claude Code transcripts without treating their JSONL framing as a
//! transaction log. Assistant responses are flushed in several lines, and
//! real transcripts show that counting each line overstates usage by about
//! 2.3x. We therefore keep the first usage for each `message.id` while
//! merging every line's content blocks into that one request.
//!
//! A transcript can also be read while Claude Code is appending its final
//! line. Parsing independently and ignoring an unparseable line makes that
//! ordinary torn write harmless instead of making a read-only report fail.

pub(super) use super::transcript_types::{
    Entry, Request, Scope, TokenUsage, ToolUse, Transcript, UserEntry,
};

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

/// Parse one JSONL transcript, dropping entries older than `since`. Never
/// fails on a torn or unparseable line - such a line is skipped. Errors only
/// when the file itself cannot be read.
pub fn parse(
    file: &super::discovery::DiscoveredFile,
    since: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<Transcript> {
    let handle = File::open(&file.path)
        .with_context(|| format!("Failed to read transcript {}", file.path.display()))?;
    let mut entries = Vec::new();
    let mut request_indices = HashMap::new();
    let mut first_user_entry = None;
    // An explicit loop, not `map_while`/`filter_map(Result::ok)`: `lines()`
    // also yields `Err` for invalid UTF-8, and a combinator that stops at the
    // first `Err` would silently truncate the rest of the file instead of
    // just skipping the one bad line.
    for line in BufReader::new(handle).lines() {
        let Ok(line) = line else { continue };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if first_user_entry.is_none() {
            first_user_entry = first_user_entry_from(&value);
        }
        add_value(&mut entries, &mut request_indices, &value, since);
    }
    Ok(Transcript {
        path: file.path.clone(),
        scope: file.scope,
        project_slug: file.project_slug.clone(),
        session_id: file.session_id.clone(),
        agent_id: file.agent_id.clone(),
        first_user_entry,
        entries,
    })
}

/// The transcript's first `user` record, read independent of the `since`
/// cutoff so spawn-prompt classification always sees the real preamble.
fn first_user_entry_from(value: &Value) -> Option<UserEntry> {
    if value.get("type").and_then(Value::as_str) != Some("user") {
        return None;
    }
    let stamp = timestamp(value)?;
    user_entries(value, stamp).into_iter().next()
}

fn add_value(
    entries: &mut Vec<Entry>,
    seen: &mut HashMap<String, usize>,
    value: &Value,
    since: DateTime<Utc>,
) {
    let Some(timestamp) = timestamp(value) else {
        return;
    };
    if timestamp < since {
        return;
    }
    match value.get("type").and_then(Value::as_str) {
        Some("assistant") => add_request(entries, seen, value, timestamp),
        Some("user") => entries.extend(user_entries(value, timestamp).into_iter().map(Entry::User)),
        _ => {}
    }
}

fn add_request(
    entries: &mut Vec<Entry>,
    seen: &mut HashMap<String, usize>,
    value: &Value,
    timestamp: DateTime<Utc>,
) {
    let request = request(value, timestamp);
    let Some(request) = request else { return };
    if let Some(id) = request.message_id.as_ref() {
        if let Some(index) = seen.get(id) {
            if let Some(Entry::Assistant(first)) = entries.get_mut(*index) {
                merge_request(first, request);
            }
            return;
        }
        seen.insert(id.clone(), entries.len());
    }
    entries.push(Entry::Assistant(request));
}

fn request(value: &Value, timestamp: DateTime<Utc>) -> Option<Request> {
    let message = value.get("message")?;
    let content = message.get("content").and_then(Value::as_array)?;
    let (tool_uses, thinking_chars, text_chars) = content_counts(content);
    Some(Request {
        message_id: message.get("id").and_then(Value::as_str).map(str::to_owned),
        timestamp,
        model: message
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        usage: usage(message.get("usage")),
        tool_uses,
        thinking_chars,
        text_chars,
    })
}

fn content_counts(blocks: &[Value]) -> (Vec<ToolUse>, usize, usize) {
    let mut tools = Vec::new();
    let mut thinking_chars = 0;
    let mut text_chars = 0;
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => text_chars += string_len(block, "text"),
            Some("thinking") => thinking_chars += string_len(block, "thinking"),
            Some("tool_use") => {
                if let Some(tool) = tool_use(block) {
                    tools.push(tool);
                }
            }
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

fn usage(value: Option<&Value>) -> TokenUsage {
    let Some(value) = value else {
        return TokenUsage::default();
    };
    let creation = number(value, "cache_creation_input_tokens");
    let split_value = value.get("cache_creation");
    let (ephemeral_5m, ephemeral_1h) = match split_value {
        Some(split) => (
            number(split, "ephemeral_5m_input_tokens"),
            number(split, "ephemeral_1h_input_tokens"),
        ),
        // Claude defaults unsplit cache creation to the five-minute TTL.
        None => (creation, 0),
    };
    let (cache_creation, ephemeral_5m, ephemeral_1h) =
        reconcile_cache_creation(creation, ephemeral_5m, ephemeral_1h);
    TokenUsage {
        input: number(value, "input_tokens"),
        cache_creation,
        cache_read: number(value, "cache_read_input_tokens"),
        output: number(value, "output_tokens"),
        ephemeral_5m,
        ephemeral_1h,
    }
}

/// The flat `cache_creation_input_tokens` field and the nested
/// `cache_creation` split are read independently from the same JSON, so
/// they can silently disagree. S2's cache-write term is priced entirely off
/// the ephemeral split, so keeping `ephemeral_5m + ephemeral_1h ==
/// cache_creation` true here is what stops a missing or unrecognised field
/// from losing that term (or the flat total) outright.
fn reconcile_cache_creation(
    creation: u64,
    ephemeral_5m: u64,
    ephemeral_1h: u64,
) -> (u64, u64, u64) {
    let split_total = ephemeral_5m + ephemeral_1h;
    if creation == 0 && split_total != 0 {
        // Nested split present, flat field missing: trust the split.
        return (split_total, ephemeral_5m, ephemeral_1h);
    }
    if split_total != creation {
        // Nested split missing or carrying unrecognised keys: fall back to
        // the flat total on the five-minute default TTL bucket.
        return (creation, creation, 0);
    }
    (creation, ephemeral_5m, ephemeral_1h)
}

fn number(value: &Value, field: &str) -> u64 {
    value.get(field).and_then(Value::as_u64).unwrap_or(0)
}

fn merge_request(first: &mut Request, duplicate: Request) {
    first.tool_uses.extend(duplicate.tool_uses);
    first.thinking_chars += duplicate.thinking_chars;
    first.text_chars += duplicate.text_chars;
    // Duplicate lines for the same message.id are supposed to carry
    // identical usage. If the first line's usage object was missing or
    // all-zero, adopt a later duplicate's real counts instead of silently
    // recording the request as free.
    if first.usage == TokenUsage::default() && duplicate.usage != TokenUsage::default() {
        first.usage = duplicate.usage;
    }
}

fn user_entries(value: &Value, timestamp: DateTime<Utc>) -> Vec<UserEntry> {
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

fn timestamp(value: &Value) -> Option<DateTime<Utc>> {
    value
        .get("timestamp")?
        .as_str()?
        .parse::<DateTime<Utc>>()
        .ok()
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
