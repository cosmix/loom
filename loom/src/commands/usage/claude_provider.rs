use std::collections::HashSet;

use super::provider_normalization::{classify_by_id, project_basename};
use super::provider_types::{
    Attribution, NormalizedEvent, ProvenanceStatus, Provider, ProviderDiagnostics,
    ProviderTokenVector, SourceKind, StageAttributionState,
};
use super::transcript::{Entry, Request, Scope, Transcript};
use super::transcript_types::SYNTHETIC_MODEL;

pub(crate) struct ClaudeNormalization {
    pub(crate) rows: Vec<NormalizedEvent>,
    pub(crate) diagnostics: ProviderDiagnostics,
}

#[derive(Clone, Copy)]
struct Location {
    transcript: usize,
    entry: usize,
    row: usize,
}

pub(crate) fn normalize(transcripts: &mut [Transcript]) -> ClaudeNormalization {
    let mut rows = Vec::new();
    let mut diagnostics = diagnostics(transcripts);
    let mut locations = Vec::new();
    for (transcript_index, transcript) in transcripts.iter().enumerate() {
        for (entry_index, entry) in transcript.entries.iter().enumerate() {
            let Entry::Assistant(request) = entry else {
                continue;
            };
            let request = request.as_ref();
            record_stream_diagnostics(request, &mut diagnostics);
            let row_index = rows.len();
            rows.push(normalized_row(transcript, request));
            locations.push(Location {
                transcript: transcript_index,
                entry: entry_index,
                row: row_index,
            });
        }
    }
    classify_by_id(&mut rows, |row| {
        (row.model != SYNTHETIC_MODEL)
            .then(|| row.request_id.clone())
            .flatten()
    });
    let removed = removed_locations(&rows, &locations);
    remove_noncanonical(transcripts, &removed);
    ClaudeNormalization { rows, diagnostics }
}

fn diagnostics(transcripts: &[Transcript]) -> ProviderDiagnostics {
    let mut result = ProviderDiagnostics {
        files_seen: transcripts.len(),
        ..ProviderDiagnostics::default()
    };
    for transcript in transcripts {
        result.malformed_rows += transcript.diagnostics.malformed_rows;
        result.missing_timestamp_rows += transcript.diagnostics.missing_timestamp_rows;
    }
    result
}

fn record_stream_diagnostics(request: &Request, diagnostics: &mut ProviderDiagnostics) {
    if let Some(first) = request.normalization.first_usage {
        diagnostics.stream_first_observed.add(usage_values(first));
    }
    if request.normalization.usage_observed {
        diagnostics
            .stream_terminal_observed
            .add(usage_values(request.usage));
    }
    if request.normalization.usage_observations > 1 {
        diagnostics.streamed_messages += 1;
    }
    if request.normalization.changed_usage_fields > 0 {
        diagnostics.stream_first_last_changes += 1;
        diagnostics.stream_changed_fields += request.normalization.changed_usage_fields;
    }
    if request.normalization.invalid_thinking_output {
        diagnostics.invalid_thinking_rows += 1;
    }
    if request.normalization.invalid_usage {
        diagnostics.invalid_token_rows += 1;
    }
    if request.normalization.invalid_cache_relation {
        diagnostics.invalid_cache_relations += 1;
    }
}

fn usage_values(usage: super::transcript::TokenUsage) -> [u64; 6] {
    [
        usage.input,
        usage.cache_creation,
        usage.cache_read,
        usage.output,
        usage.ephemeral_5m,
        usage.ephemeral_1h,
    ]
}

fn normalized_row(transcript: &Transcript, request: &Request) -> NormalizedEvent {
    let tokens = claude_tokens(request);
    NormalizedEvent {
        provider: Provider::Claude,
        event_timestamp: request.timestamp,
        source_kind: SourceKind::ClaudeTranscript,
        request_id: request.message_id.clone(),
        source_ordinal: request.normalization.line_ordinal,
        model: if request.model.is_empty() {
            "unknown".to_owned()
        } else {
            request.model.clone()
        },
        project: project_basename(transcript.project_path.as_deref()),
        attribution: attribution(transcript),
        provenance: provenance(request, &tokens),
        observation_count: request.normalization.usage_observations,
        changed_usage_fields: request.normalization.changed_usage_fields,
        first_observed_in_range: request.normalization.first_observed_in_range,
        true_fresh_start: request.normalization.true_fresh_start,
        tool_names: request
            .tool_uses
            .iter()
            .map(|tool| tool.name.clone())
            .collect(),
        tokens,
    }
}

fn claude_tokens(request: &Request) -> ProviderTokenVector {
    if !request.normalization.usage_observed {
        return ProviderTokenVector::default();
    }
    let cache_writes = if request.normalization.invalid_cache_relation {
        (None, None)
    } else {
        (
            Some(request.usage.ephemeral_5m),
            Some(request.usage.ephemeral_1h),
        )
    };
    ProviderTokenVector {
        input_tokens: Some(request.usage.input),
        fresh_input_tokens: Some(request.usage.input),
        cache_creation_input_tokens: Some(request.usage.cache_creation),
        cache_read_input_tokens: Some(request.usage.cache_read),
        cache_write_5m_input_tokens: cache_writes.0,
        cache_write_1h_input_tokens: cache_writes.1,
        output_tokens: Some(request.usage.output),
        thinking_output_tokens: request.normalization.thinking_output_tokens,
        resident_input_tokens: Some(
            request
                .usage
                .input
                .saturating_add(request.usage.cache_creation)
                .saturating_add(request.usage.cache_read),
        ),
        total_tokens: None,
    }
}

fn provenance(request: &Request, tokens: &ProviderTokenVector) -> ProvenanceStatus {
    if request.model == SYNTHETIC_MODEL {
        ProvenanceStatus::Synthetic
    } else if !request.normalization.usage_observed || request.normalization.invalid_usage {
        ProvenanceStatus::UnknownUsage
    } else if tokens.is_zero() {
        ProvenanceStatus::ZeroUsage
    } else {
        ProvenanceStatus::MeasuredCanonical
    }
}

fn attribution(transcript: &Transcript) -> Attribution {
    let scope = match transcript.scope {
        Scope::Main => "main",
        Scope::Subagent => "subagent",
    };
    Attribution {
        scope: scope.to_owned(),
        role: transcript
            .agent_type
            .clone()
            .or_else(|| (transcript.scope == Scope::Main).then(|| "main".to_owned())),
        stage_id: transcript.stage_id.clone(),
        loom_session_id: transcript.loom_session_id.clone(),
        stage_state: if transcript.stage_id.is_some() {
            StageAttributionState::Known
        } else {
            StageAttributionState::Unknown
        },
    }
}

fn removed_locations(rows: &[NormalizedEvent], locations: &[Location]) -> HashSet<(usize, usize)> {
    locations
        .iter()
        .filter(|location| {
            matches!(
                rows[location.row].provenance,
                ProvenanceStatus::DuplicateExact | ProvenanceStatus::AmbiguousConflict
            )
        })
        .map(|location| (location.transcript, location.entry))
        .collect()
}

fn remove_noncanonical(transcripts: &mut [Transcript], removed: &HashSet<(usize, usize)>) {
    for (transcript_index, transcript) in transcripts.iter_mut().enumerate() {
        let mut entry_index = 0;
        transcript.entries.retain(|entry| {
            let keep = !matches!(entry, Entry::Assistant(_))
                || !removed.contains(&(transcript_index, entry_index));
            entry_index += 1;
            keep
        });
    }
}

#[cfg(test)]
#[path = "claude_provider_tests.rs"]
mod tests;
