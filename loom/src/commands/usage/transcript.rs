//! Reads Claude Code transcripts without treating their JSONL framing as a
//! transaction log. Assistant responses are flushed in several lines, and
//! real transcripts show that counting each line overstates usage by about
//! 2.3x. We therefore keep the last complete usage vector for each
//! `message.id` while merging every line's content blocks into one request.
//!
//! A transcript can also be read while Claude Code is appending its final
//! line. Parsing independently and ignoring an unparseable line makes that
//! ordinary torn write harmless instead of making a read-only report fail.

pub(super) use super::transcript_types::{
    Entry, Request, RequestNormalization, Scope, TokenUsage, ToolUse, Transcript,
    TranscriptDiagnostics, UserEntry,
};

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use super::transcript_merge::merge_request;

struct ScanResult {
    entries: Vec<Entry>,
    first_user_entry: Option<UserEntry>,
    project_path: Option<PathBuf>,
    diagnostics: TranscriptDiagnostics,
    had_prior_request: bool,
    chronology_uncertain: bool,
}

/// Parse one JSONL transcript, dropping entries older than `since`. Never
/// fails on a torn or unparseable line - such a line is skipped. Errors only
/// when the file itself cannot be read.
pub fn parse(
    file: &super::discovery::DiscoveredFile,
    range: &super::time_range::TimeRange,
) -> anyhow::Result<Transcript> {
    let handle = File::open(&file.path)
        .with_context(|| format!("Failed to read transcript {}", file.path.display()))?;
    let mut scan = scan_lines(BufReader::new(handle), range);
    mark_fresh_start(&mut scan);
    Ok(build_transcript(file, scan))
}

fn scan_lines(reader: BufReader<File>, range: &super::time_range::TimeRange) -> ScanResult {
    let mut entries = Vec::new();
    let mut request_indices = HashMap::new();
    let mut first_user_entry = None;
    let mut project_path = None;
    let mut diagnostics = TranscriptDiagnostics::default();
    let mut had_prior_request = false;
    let mut chronology_uncertain = false;
    for (ordinal, line) in reader.lines().enumerate() {
        let Some(value) = decoded_line(line, &mut diagnostics) else {
            chronology_uncertain = true;
            continue;
        };
        if first_user_entry.is_none() {
            first_user_entry = first_user_entry_from(&value);
        }
        capture_project_path(&mut project_path, &value);
        scan_value(
            &mut entries,
            &mut request_indices,
            &value,
            ordinal,
            range,
            &mut diagnostics,
            &mut had_prior_request,
            &mut chronology_uncertain,
        );
    }
    ScanResult {
        entries,
        first_user_entry,
        project_path,
        diagnostics,
        had_prior_request,
        chronology_uncertain,
    }
}

fn capture_project_path(project_path: &mut Option<PathBuf>, value: &Value) {
    if project_path.is_some() {
        return;
    }
    *project_path = value
        .get("cwd")
        .and_then(Value::as_str)
        .filter(|cwd| !cwd.trim().is_empty())
        .map(PathBuf::from);
}

fn decoded_line(
    line: std::io::Result<String>,
    diagnostics: &mut TranscriptDiagnostics,
) -> Option<Value> {
    let line = match line {
        Ok(line) => line,
        Err(_) => {
            diagnostics.malformed_rows += 1;
            return None;
        }
    };
    match serde_json::from_str(&line) {
        Ok(value) => Some(value),
        Err(_) => {
            diagnostics.malformed_rows += 1;
            None
        }
    }
}

fn build_transcript(file: &super::discovery::DiscoveredFile, scan: ScanResult) -> Transcript {
    Transcript {
        path: file.path.clone(),
        scope: file.scope,
        project_slug: file.project_slug.clone(),
        project_path: scan.project_path,
        session_id: file.session_id.clone(),
        agent_id: file.agent_id.clone(),
        agent_type: None,
        stage_id: None,
        loom_session_id: None,
        forward_receipt: None,
        forward_candidate: false,
        first_user_entry: scan.first_user_entry,
        entries: scan.entries,
        diagnostics: scan.diagnostics,
    }
}

/// The transcript's first `user` record, read independent of the `since`
/// cutoff so spawn-prompt classification always sees the real preamble.
fn first_user_entry_from(value: &Value) -> Option<UserEntry> {
    if value.get("type").and_then(Value::as_str) != Some("user") {
        return None;
    }
    let stamp = timestamp(value)?;
    super::transcript_content::user_entries(value, stamp)
        .into_iter()
        .next()
}

fn add_value(
    entries: &mut Vec<Entry>,
    seen: &mut HashMap<String, usize>,
    value: &Value,
    timestamp: DateTime<Utc>,
    ordinal: usize,
) {
    match value.get("type").and_then(Value::as_str) {
        Some("assistant") => add_request(entries, seen, value, timestamp, ordinal),
        Some("user") => entries.extend(
            super::transcript_content::user_entries(value, timestamp)
                .into_iter()
                .map(Entry::User),
        ),
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn scan_value(
    entries: &mut Vec<Entry>,
    seen: &mut HashMap<String, usize>,
    value: &Value,
    ordinal: usize,
    range: &super::time_range::TimeRange,
    diagnostics: &mut TranscriptDiagnostics,
    had_prior_request: &mut bool,
    chronology_uncertain: &mut bool,
) {
    let Some(timestamp) = timestamp(value) else {
        diagnostics.missing_timestamp_rows += 1;
        *chronology_uncertain = true;
        return;
    };
    let synthetic = value.pointer("/message/model").and_then(Value::as_str)
        == Some(super::transcript_types::SYNTHETIC_MODEL);
    if value.get("type").and_then(Value::as_str) == Some("assistant")
        && !synthetic
        && timestamp < range.since
    {
        *had_prior_request = true;
    }
    if range.includes(timestamp) {
        add_value(entries, seen, value, timestamp, ordinal);
    }
}

fn add_request(
    entries: &mut Vec<Entry>,
    seen: &mut HashMap<String, usize>,
    value: &Value,
    timestamp: DateTime<Utc>,
    ordinal: usize,
) {
    let request = request(value, timestamp, ordinal);
    let Some(request) = request else { return };
    if let Some(id) = request.message_id.as_ref() {
        if let Some(index) = seen.get(id) {
            if let Some(Entry::Assistant(first)) = entries.get_mut(*index) {
                merge_request(first.as_mut(), request);
            }
            return;
        }
        seen.insert(id.clone(), entries.len());
    }
    entries.push(Entry::Assistant(Box::new(request)));
}

fn request(value: &Value, timestamp: DateTime<Utc>, ordinal: usize) -> Option<Request> {
    let message = value.get("message")?;
    let content = message.get("content").and_then(Value::as_array)?;
    let (tool_uses, thinking_chars, text_chars) =
        super::transcript_content::content_counts(content);
    let (usage, normalization) =
        super::claude_usage::parse(message.get("usage"), timestamp, ordinal);
    Some(Request {
        message_id: message
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .map(str::to_owned),
        timestamp,
        model: message
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        usage,
        tool_uses,
        thinking_chars,
        text_chars,
        normalization,
    })
}

fn mark_fresh_start(scan: &mut ScanResult) {
    let mut first = true;
    for entry in &mut scan.entries {
        if let Entry::Assistant(request) = entry {
            let request = request.as_mut();
            if request.model == super::transcript_types::SYNTHETIC_MODEL {
                request.normalization.first_observed_in_range = false;
                request.normalization.true_fresh_start = None;
                continue;
            }
            request.normalization.first_observed_in_range = first;
            request.normalization.true_fresh_start = if first && scan.chronology_uncertain {
                None
            } else if first {
                Some(!scan.had_prior_request)
            } else {
                Some(false)
            };
            first = false;
        }
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
