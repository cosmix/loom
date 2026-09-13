use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::codex_tokens::token_vector;
use super::provider_normalization::{classify_by_id, project_basename};
use super::provider_types::{
    Attribution, NormalizedEvent, ProvenanceStatus, Provider, ProviderDiagnostics,
    ProviderTokenVector, SourceKind, StageAttributionState,
};
use super::time_range::TimeRange;

pub(crate) struct CodexNormalization {
    pub(crate) rows: Vec<NormalizedEvent>,
    pub(crate) diagnostics: ProviderDiagnostics,
}

#[derive(Default)]
struct FileState {
    model: String,
    project: String,
    previous_fallback: Option<(Value, Value)>,
    had_prior_usage: bool,
    chronology_uncertain: bool,
    observed_in_range: bool,
    saw_direct: bool,
    saw_fallback: bool,
    thread_id: Option<String>,
    thread_id_conflict: bool,
}

pub(crate) fn normalize(files: &[PathBuf], range: &TimeRange) -> CodexNormalization {
    let mut rows = Vec::new();
    let mut diagnostics = ProviderDiagnostics {
        files_seen: files.len(),
        ..ProviderDiagnostics::default()
    };
    for path in files {
        match parse_file(path, range, &mut diagnostics) {
            Ok(file_rows) => rows.extend(file_rows),
            Err(_) => diagnostics.unreadable_files += 1,
        }
    }
    classify_response_ids(&mut rows);
    CodexNormalization { rows, diagnostics }
}

fn parse_file(
    path: &Path,
    range: &TimeRange,
    diagnostics: &mut ProviderDiagnostics,
) -> std::io::Result<Vec<NormalizedEvent>> {
    let reader = BufReader::new(File::open(path)?);
    let mut state = FileState::default();
    let mut rows = Vec::new();
    for (ordinal, line) in reader.lines().enumerate() {
        let value = match line.and_then(json_line) {
            Ok(value) => value,
            Err(_) => {
                diagnostics.malformed_rows += 1;
                state.chronology_uncertain = true;
                continue;
            }
        };
        update_context(&value, &mut state);
        if let Some(row) = direct_row(&value, ordinal, range, &mut state, diagnostics) {
            rows.push(row);
        }
        if let Some(row) = fallback_row(&value, ordinal, range, &mut state, diagnostics) {
            rows.push(row);
        }
    }
    apply_thread_identity(&mut rows, &state);
    classify_mixed_file(&mut rows, state.saw_direct, state.saw_fallback, diagnostics);
    Ok(rows)
}

fn apply_thread_identity(rows: &mut [NormalizedEvent], state: &FileState) {
    for row in rows {
        if state.thread_id_conflict {
            row.codex_thread_id = None;
            row.codex_thread_conflict = true;
        } else {
            row.codex_thread_id.clone_from(&state.thread_id);
        }
    }
}

fn json_line(line: String) -> std::io::Result<Value> {
    serde_json::from_str(&line).map_err(std::io::Error::other)
}

fn update_context(value: &Value, state: &mut FileState) {
    update_thread_id(value, state);
    if let Some(model) = value
        .pointer("/payload/model")
        .or_else(|| value.pointer("/payload/model_name"))
        .and_then(Value::as_str)
    {
        state.model = model.to_owned();
    }
    if let Some(cwd) = value.pointer("/payload/cwd").and_then(Value::as_str) {
        state.project = project_basename(Some(Path::new(cwd)));
    }
}

fn update_thread_id(value: &Value, state: &mut FileState) {
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return;
    }
    let Some(thread_id) = value.pointer("/payload/id").and_then(Value::as_str) else {
        return;
    };
    if !crate::models::forward_receipt::is_safe_id(thread_id) {
        state.thread_id_conflict = true;
        return;
    }
    match state.thread_id.as_deref() {
        Some(known) if known != thread_id => state.thread_id_conflict = true,
        None => state.thread_id = Some(thread_id.to_owned()),
        _ => {}
    }
}

fn direct_row(
    value: &Value,
    ordinal: usize,
    range: &TimeRange,
    state: &mut FileState,
    diagnostics: &mut ProviderDiagnostics,
) -> Option<NormalizedEvent> {
    if value.get("type").and_then(Value::as_str) != Some("token_usage_record") {
        return None;
    }
    state.saw_direct = true;
    let Some(usage) = value
        .pointer("/payload/usage")
        .filter(|usage| usage.is_object())
    else {
        diagnostics.malformed_rows += 1;
        return None;
    };
    let timestamp = usage_timestamp(value, range, state, diagnostics)?;
    let tokens = token_vector(usage, diagnostics);
    Some(row(
        timestamp,
        SourceKind::CodexDirect,
        value
            .pointer("/payload/response_id")
            .or_else(|| usage.get("response_id"))
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .map(str::to_owned),
        ordinal,
        tokens,
        state,
    ))
}

fn fallback_row(
    value: &Value,
    ordinal: usize,
    range: &TimeRange,
    state: &mut FileState,
    diagnostics: &mut ProviderDiagnostics,
) -> Option<NormalizedEvent> {
    if value.get("type").and_then(Value::as_str) != Some("event_msg")
        || value.pointer("/payload/type").and_then(Value::as_str) != Some("token_count")
    {
        return None;
    }
    state.saw_fallback = true;
    let Some(last) = value
        .pointer("/payload/info/last_token_usage")
        .filter(|usage| usage.is_object())
        .cloned()
    else {
        diagnostics.malformed_rows += 1;
        state.previous_fallback = None;
        return None;
    };
    let total = value
        .pointer("/payload/info/total_token_usage")
        .cloned()
        .unwrap_or(Value::Null);
    let duplicate = state
        .previous_fallback
        .as_ref()
        .is_some_and(|previous| previous == &(last.clone(), total.clone()));
    state.previous_fallback = Some((last.clone(), total));
    if duplicate {
        diagnostics.fallback_duplicate_snapshots += 1;
        return None;
    }
    let timestamp = usage_timestamp(value, range, state, diagnostics)?;
    let tokens = token_vector(&last, diagnostics);
    Some(row(
        timestamp,
        SourceKind::CodexFallback,
        None,
        ordinal,
        tokens,
        state,
    ))
}

fn usage_timestamp(
    value: &Value,
    range: &TimeRange,
    state: &mut FileState,
    diagnostics: &mut ProviderDiagnostics,
) -> Option<DateTime<Utc>> {
    let timestamp = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|stamp| stamp.parse().ok());
    let Some(timestamp) = timestamp else {
        diagnostics.missing_timestamp_rows += 1;
        state.chronology_uncertain = true;
        return None;
    };
    if timestamp < range.since {
        state.had_prior_usage = true;
    }
    range.includes(timestamp).then_some(timestamp)
}

fn provenance(tokens: &ProviderTokenVector) -> ProvenanceStatus {
    if tokens.input_tokens.is_none() || tokens.output_tokens.is_none() {
        ProvenanceStatus::UnknownUsage
    } else if tokens.is_zero() {
        ProvenanceStatus::ZeroUsage
    } else {
        ProvenanceStatus::MeasuredCanonical
    }
}

fn row(
    timestamp: DateTime<Utc>,
    source_kind: SourceKind,
    request_id: Option<String>,
    ordinal: usize,
    tokens: ProviderTokenVector,
    state: &mut FileState,
) -> NormalizedEvent {
    let first = !state.observed_in_range;
    state.observed_in_range = true;
    let provenance = provenance(&tokens);
    NormalizedEvent {
        provider: Provider::Codex,
        event_timestamp: timestamp,
        source_kind,
        request_id,
        source_ordinal: ordinal,
        model: value_or_unknown(&state.model),
        project: value_or_unknown(&state.project),
        tokens,
        attribution: Attribution {
            scope: "main".to_owned(),
            role: Some("codex".to_owned()),
            stage_id: None,
            loom_session_id: None,
            stage_state: StageAttributionState::Unknown,
        },
        provenance,
        observation_count: 1,
        changed_usage_fields: 0,
        first_observed_in_range: first,
        true_fresh_start: if first && state.chronology_uncertain {
            None
        } else if first {
            Some(!state.had_prior_usage)
        } else {
            Some(false)
        },
        tool_names: Vec::new(),
        codex_thread_id: None,
        codex_thread_conflict: false,
        forward_candidate: false,
        forward_receipt: None,
    }
}

fn value_or_unknown(value: &str) -> String {
    if value.is_empty() {
        "unknown".to_owned()
    } else {
        value.to_owned()
    }
}

fn classify_mixed_file(
    rows: &mut [NormalizedEvent],
    direct: bool,
    fallback_schema: bool,
    diagnostics: &mut ProviderDiagnostics,
) {
    let fallback = rows
        .iter()
        .filter(|row| row.source_kind == SourceKind::CodexFallback)
        .count();
    if direct && fallback_schema {
        diagnostics.mixed_schema_files += 1;
        diagnostics.unknown_fallback_relations += fallback;
        for row in rows
            .iter_mut()
            .filter(|row| row.source_kind == SourceKind::CodexFallback)
        {
            row.provenance = ProvenanceStatus::FallbackCoverage;
        }
    }
}

fn classify_response_ids(rows: &mut [NormalizedEvent]) {
    classify_by_id(rows, |row| {
        (row.source_kind == SourceKind::CodexDirect)
            .then(|| row.request_id.clone())
            .flatten()
    });
}

#[cfg(test)]
#[path = "codex_provider_tests.rs"]
mod tests;
