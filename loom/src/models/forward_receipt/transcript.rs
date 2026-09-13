use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::is_safe_id;
use super::locator::{read_bounded_prefix, validate_task_output_path};
use super::marker::{decode_marker_channel, MarkerChannel, MAX_PREFIX_BYTES};

mod identity;
pub use identity::TranscriptIdentity;

const MAX_TRANSCRIPT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardingResult {
    pub id: String,
    pub text: String,
    pub background_task_id: Option<String>,
    pub task_output_path_text: Vec<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardingInvocation {
    pub id: String,
    pub model: String,
    pub effort: String,
    pub timestamp: DateTime<Utc>,
    pub result: Option<ForwardingResult>,
}

pub struct Transcript {
    pub identity: TranscriptIdentity,
    pub invocations: Vec<ForwardingInvocation>,
    entries: Vec<Value>,
}

impl Transcript {
    pub fn entries(&self) -> &[Value] {
        &self.entries
    }
}

pub fn read(path: &Path) -> Result<Transcript> {
    let identity = TranscriptIdentity::from_path(path)?;
    let metadata = fs::symlink_metadata(path).context("inspecting forwarding transcript")?;
    ensure!(
        metadata.file_type().is_file(),
        "forwarding transcript is not a regular file"
    );
    ensure!(
        metadata.len() <= MAX_TRANSCRIPT_BYTES as u64,
        "forwarding transcript is truncated"
    );
    let input = read_bounded_prefix(path, MAX_TRANSCRIPT_BYTES + 1)?;
    ensure!(
        input.len() <= MAX_TRANSCRIPT_BYTES,
        "forwarding transcript is truncated"
    );
    decode(&input, identity)
}

pub fn decode(input: &str, identity: TranscriptIdentity) -> Result<Transcript> {
    let mut calls = BTreeMap::new();
    let mut results = BTreeMap::new();
    let mut entries = Vec::new();
    for (line_number, line) in input.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("invalid transcript JSON at line {}", line_number + 1))?;
        ensure_row_identity(&value, &identity)?;
        scan_entry(&value, &mut calls, &mut results);
        entries.push(value);
    }
    let invocations = calls
        .into_values()
        .flatten()
        .map(|mut invocation| {
            invocation.result = results.remove(&invocation.id).flatten();
            invocation
        })
        .collect();
    Ok(Transcript {
        identity,
        invocations,
        entries,
    })
}

fn ensure_row_identity(value: &Value, identity: &TranscriptIdentity) -> Result<()> {
    for (fields, expected) in [
        (
            ["sessionId", "session_id"],
            identity.parent_session_id.as_str(),
        ),
        (["agentId", "agent_id"], identity.agent_id.as_str()),
    ] {
        for field in fields {
            if let Some(actual) = value.get(field) {
                ensure!(
                    actual.as_str() == Some(expected),
                    "transcript row identity mismatch"
                );
            }
        }
    }
    Ok(())
}

fn scan_entry(
    row: &Value,
    calls: &mut BTreeMap<String, Option<ForwardingInvocation>>,
    results: &mut BTreeMap<String, Option<ForwardingResult>>,
) {
    let Some(timestamp) = timestamp(row) else {
        return;
    };
    let Some(blocks) = content(row) else {
        return;
    };
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("tool_use") => add_invocation(calls, block, timestamp),
            Some("tool_result") => add_result(results, row, block, timestamp),
            _ => {}
        }
    }
}

fn timestamp(value: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.get("timestamp")?.as_str()?)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn content(row: &Value) -> Option<&[Value]> {
    row.pointer("/message/content")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
}

fn add_invocation(
    calls: &mut BTreeMap<String, Option<ForwardingInvocation>>,
    block: &Value,
    timestamp: DateTime<Utc>,
) {
    let Some(id) = block.get("id").and_then(Value::as_str) else {
        return;
    };
    if block.get("name").and_then(Value::as_str) != Some("Bash") || !is_safe_id(id) {
        return;
    }
    let Some(command) = block.pointer("/input/command").and_then(Value::as_str) else {
        return;
    };
    let Some((model, effort)) = exact_forward_argv(command) else {
        return;
    };
    insert_unique(
        calls,
        id,
        ForwardingInvocation {
            id: id.to_owned(),
            model,
            effort,
            timestamp,
            result: None,
        },
    );
}

fn add_result(
    results: &mut BTreeMap<String, Option<ForwardingResult>>,
    row: &Value,
    block: &Value,
    timestamp: DateTime<Utc>,
) {
    let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
        return;
    };
    if !is_safe_id(id) {
        return;
    }
    let fallback = tool_result_text(block);
    let metadata = tool_result_metadata(row, block);
    let text = metadata
        .iter()
        .find_map(|value| value.get("stdout").and_then(Value::as_str))
        .map_or_else(|| fallback.clone(), str::to_owned);
    let background_task_id = metadata
        .iter()
        .find_map(|value| {
            value
                .get("backgroundTaskId")
                .or_else(|| value.get("background_task_id"))
                .and_then(Value::as_str)
        })
        .filter(|value| is_safe_id(value))
        .map(str::to_owned);
    let mut task_output_path_text = vec![fallback];
    for value in metadata {
        collect_strings(value, &mut task_output_path_text);
    }
    insert_unique(
        results,
        id,
        ForwardingResult {
            id: id.to_owned(),
            text,
            background_task_id,
            task_output_path_text,
            timestamp,
        },
    );
}

fn tool_result_metadata<'a>(row: &'a Value, block: &'a Value) -> Vec<&'a Value> {
    [row, block]
        .into_iter()
        .flat_map(|value| {
            ["toolUseResult", "tool_use_result"]
                .into_iter()
                .filter_map(move |field| value.get(field))
        })
        .collect()
}

fn tool_result_text(block: &Value) -> String {
    match block.get("content") {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.get("text")?.as_str())
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn collect_strings(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(value) => output.push(value.clone()),
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_strings(value, output)),
        Value::Object(values) => values
            .values()
            .for_each(|value| collect_strings(value, output)),
        _ => {}
    }
}

fn insert_unique<T>(items: &mut BTreeMap<String, Option<T>>, id: &str, value: T) {
    items
        .entry(id.to_owned())
        .and_modify(|existing| *existing = None)
        .or_insert(Some(value));
}

pub fn exact_forward_argv(command: &str) -> Option<(String, String)> {
    if !safe_shell_syntax(command) {
        return None;
    }
    let words = shell_words::split(command).ok()?;
    if words.len() != 8
        || !words[0].contains('/')
        || Path::new(&words[0]).file_name()?.to_str()? != "codex-forward.sh"
        || words[1] != "task"
        || words[2].is_empty()
        || words[3] != "--model"
        || words[5] != "--effort"
        || words[7] != "--write"
        || !bounded_diagnostic(&words[4])
        || !bounded_diagnostic(&words[6])
    {
        return None;
    }
    Some((words[4].clone(), words[6].clone()))
}

fn safe_shell_syntax(input: &str) -> bool {
    #[derive(Clone, Copy)]
    enum State {
        Plain,
        Single,
        Double,
        Escape,
        DoubleEscape,
    }
    let mut state = State::Plain;
    for character in input.chars() {
        state = match (state, character) {
            (State::Plain, '\'') => State::Single,
            (State::Plain, '"') => State::Double,
            (State::Plain, '\\') => State::Escape,
            (State::Plain, c) if "\n\r\t\u{b}\u{c};|&<>`$()#*?[]{}".contains(c) => return false,
            (State::Plain, _) => State::Plain,
            (State::Single, '\'') => State::Plain,
            (State::Single, _) => State::Single,
            (State::Double, '"') => State::Plain,
            (State::Double, '\\') => State::DoubleEscape,
            (State::Double, c) if "\n\r\t\u{b}\u{c}$`".contains(c) => return false,
            (State::Double, _) => State::Double,
            (State::Escape, c) if !c.is_control() => State::Plain,
            (State::DoubleEscape, '"' | '\\') => State::Double,
            _ => return false,
        };
    }
    matches!(state, State::Plain)
}

fn bounded_diagnostic(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub fn evidence_channel(
    result: &ForwardingResult,
    roots: &[PathBuf],
    parent_session_id: &str,
) -> MarkerChannel {
    let inline = decode_marker_channel(&result.text);
    if !matches!(&inline, MarkerChannel::Absent) {
        return inline;
    }
    let Some(task_id) = result.background_task_id.as_deref() else {
        return inline;
    };
    let valid = validated_task_output_paths(result, roots, parent_session_id, task_id);
    if valid.len() != 1 {
        return inline;
    }
    let Some(path) = valid.into_iter().next() else {
        return inline;
    };
    read_bounded_prefix(&path, MAX_PREFIX_BYTES).map_or(inline, |text| decode_marker_channel(&text))
}

fn validated_task_output_paths(
    result: &ForwardingResult,
    roots: &[PathBuf],
    parent_session_id: &str,
    task_id: &str,
) -> HashSet<PathBuf> {
    let mut valid = HashSet::new();
    for text in &result.task_output_path_text {
        for token in text.split_whitespace() {
            let candidate = token.trim_matches(|c: char| "'\"`()[]{}<>,.:;".contains(c));
            if let Ok(path) =
                validate_task_output_path(Path::new(candidate), roots, parent_session_id, task_id)
            {
                valid.insert(path);
            }
        }
    }
    valid
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
