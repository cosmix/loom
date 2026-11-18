//! Reads Claude Code transcripts without treating their JSONL framing as a
//! transaction log. Assistant responses are flushed in several lines, and
//! each line can repeat usage. Select the whole usage vector with the latest timestamp
//! for each `message.id`, keeping the first on ties, while merging every
//! line's content blocks into one request. Absent counters in a partial vector
//! are zero, as in the live tracker.
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
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use super::transcript_attachment::attachment_entry;
use super::transcript_merge::merge_request;
use super::transcript_types::{AttachmentEntry, SYNTHETIC_MODEL};

#[derive(Default)]
struct Scan {
    entries: Vec<Entry>,
    attachments: Vec<AttachmentEntry>,
    first_user_entry: Option<UserEntry>,
    project_path: Option<PathBuf>,
    diagnostics: TranscriptDiagnostics,
    had_prior_request: bool,
    chronology_uncertain: bool,
    /// Where each message id's request sits in `entries`.
    request_indices: HashMap<String, usize>,
    /// Uuids of the `user` and `attachment` rows already seen: a later row with
    /// the same uuid is a copy of one the model saw once.
    row_uuids: HashSet<String>,
    /// Non-synthetic requests retained so far.
    retained_requests: usize,
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

fn scan_lines(reader: BufReader<File>, range: &super::time_range::TimeRange) -> Scan {
    let mut scan = Scan::default();
    for (ordinal, line) in reader.lines().enumerate() {
        let Some(value) = decoded_line(line, &mut scan.diagnostics) else {
            scan.note_unplaced_row();
            continue;
        };
        if scan.first_user_entry.is_none() {
            scan.first_user_entry = first_user_entry_from(&value);
        }
        capture_project_path(&mut scan.project_path, &value);
        scan.add_row(&value, ordinal, range);
    }
    scan
}

impl Scan {
    /// A row with no place in time may be context or a request that came before
    /// the first retained request, which then cannot be called the start. After
    /// that request it changes nothing about the start.
    fn note_unplaced_row(&mut self) {
        self.chronology_uncertain |= self.retained_requests == 0;
    }

    fn add_row(&mut self, value: &Value, ordinal: usize, range: &super::time_range::TimeRange) {
        let kind = value.get("type").and_then(Value::as_str);
        let Some(timestamp) = timestamp(value) else {
            self.diagnostics.missing_timestamp_rows += 1;
            if matches!(kind, Some("user" | "assistant" | "attachment")) {
                self.note_unplaced_row();
            }
            return;
        };
        if matches!(kind, Some("user" | "attachment")) && self.is_repeated_row(value) {
            return;
        }
        let synthetic =
            value.pointer("/message/model").and_then(Value::as_str) == Some(SYNTHETIC_MODEL);
        if kind == Some("assistant") && !synthetic && timestamp < range.since {
            self.had_prior_request = true;
        }
        if !range.includes(timestamp) {
            return;
        }
        match kind {
            Some("assistant") => self.add_request(value, timestamp, ordinal),
            Some("user") => self.entries.extend(
                super::transcript_content::user_entries(value, timestamp)
                    .into_iter()
                    .map(Entry::User),
            ),
            Some("attachment") => self
                .attachments
                .extend(attachment_entry(value, self.retained_requests)),
            _ => {}
        }
    }

    /// Whether this row's uuid was seen before. A row with no uuid never repeats.
    fn is_repeated_row(&mut self, value: &Value) -> bool {
        value
            .get("uuid")
            .and_then(Value::as_str)
            .filter(|uuid| !uuid.is_empty())
            .is_some_and(|uuid| !self.row_uuids.insert(uuid.to_owned()))
    }

    fn add_request(&mut self, value: &Value, timestamp: DateTime<Utc>, ordinal: usize) {
        let Some(request) = request(value, timestamp, ordinal) else {
            return;
        };
        if let Some(id) = request.message_id.as_ref() {
            if let Some(index) = self.request_indices.get(id) {
                if let Some(Entry::Assistant(first)) = self.entries.get_mut(*index) {
                    // A merge can take the later line's model.
                    self.retained_requests -= usize::from(first.model != SYNTHETIC_MODEL);
                    merge_request(first.as_mut(), request);
                    self.retained_requests += usize::from(first.model != SYNTHETIC_MODEL);
                }
                return;
            }
            self.request_indices.insert(id.clone(), self.entries.len());
        }
        self.retained_requests += usize::from(request.model != SYNTHETIC_MODEL);
        self.entries.push(Entry::Assistant(Box::new(request)));
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

fn build_transcript(file: &super::discovery::DiscoveredFile, scan: Scan) -> Transcript {
    Transcript {
        path: file.path.clone(),
        scope: file.scope,
        project_slug: file.project_slug.clone(),
        project_path: scan.project_path,
        session_id: file.session_id.clone(),
        agent_id: file.agent_id.clone(),
        agent_type: None,
        agent_type_from_sidecar: false,
        requested_model: None,
        stage_id: None,
        loom_session_id: None,
        forward_receipt: None,
        forward_candidate: false,
        first_user_entry: scan.first_user_entry,
        entries: scan.entries,
        attachments: scan.attachments,
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

fn mark_fresh_start(scan: &mut Scan) {
    let mut first = true;
    for entry in &mut scan.entries {
        if let Entry::Assistant(request) = entry {
            let request = request.as_mut();
            if request.model == SYNTHETIC_MODEL {
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
#[path = "transcript_row_tests.rs"]
mod row_tests;
#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
