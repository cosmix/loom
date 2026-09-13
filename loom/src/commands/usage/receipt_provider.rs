use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::models::execution_receipt::{
    decode, ExecutionReceipt, ReceiptDecodeError, ReceiptProvider, ReceiptUsage,
};

use super::provider_normalization::classify_by_id;
use super::provider_types::{
    Attribution, NormalizedEvent, ProvenanceStatus, Provider, ProviderDiagnostics,
    ProviderTokenVector, SourceKind, StageAttributionState,
};
use super::time_range::TimeRange;

pub(crate) struct ReceiptNormalization {
    pub(crate) rows: Vec<NormalizedEvent>,
    pub(crate) diagnostics: BTreeMap<Provider, ProviderDiagnostics>,
}

pub(crate) fn normalize(root: Option<&Path>, range: &TimeRange) -> ReceiptNormalization {
    let mut result = ReceiptNormalization {
        rows: Vec::new(),
        diagnostics: BTreeMap::new(),
    };
    let Some(root) = root else {
        return result;
    };
    if !root.is_dir() {
        increment_diagnostic(&mut result, None, |value| value.missing_roots += 1);
        return result;
    }
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => {
            increment_diagnostic(&mut result, None, |value| value.unreadable_files += 1);
            return result;
        }
    };
    let mut paths = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        read_file(&path, range, &mut result);
    }
    classify_request_ids(&mut result.rows);
    result
}

fn read_file(path: &Path, range: &TimeRange, result: &mut ReceiptNormalization) {
    for provider in [Provider::Claude, Provider::Codex] {
        result
            .diagnostics
            .entry(provider)
            .or_default()
            .receipt_files_seen += 1;
    }
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => {
            increment_diagnostic(result, None, |value| value.unreadable_files += 1);
            return;
        }
    };
    let documents: Vec<&str> = if path.extension().and_then(|value| value.to_str()) == Some("jsonl")
    {
        content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect()
    } else {
        vec![content.as_str()]
    };
    for (ordinal, document) in documents.into_iter().enumerate() {
        match decode(document) {
            Ok(receipt) => add_receipt(result, receipt, ordinal, range),
            Err(error) => record_decode_error(result, error),
        }
    }
}

fn add_receipt(
    result: &mut ReceiptNormalization,
    receipt: ExecutionReceipt,
    ordinal: usize,
    range: &TimeRange,
) {
    let provider = provider(receipt.provider);
    let request_id = receipt
        .request_id
        .as_deref()
        .filter(|request_id| !request_id.trim().is_empty());
    let request_id = request_id.map(str::to_owned);
    let diagnostics = result.diagnostics.entry(provider).or_default();
    let invalid_cache_split = cache_split_invalid(&receipt.usage);
    if request_id.is_none() {
        diagnostics.unattributable_receipts += 1;
    }
    if invalid_cache_split {
        diagnostics.invalid_cache_relations += 1;
    }
    if provider == Provider::Codex
        && receipt
            .usage
            .cache_read_input_tokens
            .is_some_and(|cached| cached > receipt.usage.input_tokens)
    {
        diagnostics.invalid_cache_relations += 1;
    }
    if !range.includes(receipt.observed_at) {
        return;
    }
    result.rows.push(receipt_row(
        &receipt,
        provider,
        request_id,
        ordinal,
        invalid_cache_split,
    ));
}

fn receipt_row(
    receipt: &ExecutionReceipt,
    provider: Provider,
    request_id: Option<String>,
    ordinal: usize,
    invalid_cache_split: bool,
) -> NormalizedEvent {
    let tokens = receipt_tokens(receipt, invalid_cache_split);
    let provenance = if tokens.is_zero() {
        ProvenanceStatus::ZeroUsage
    } else {
        ProvenanceStatus::MeasuredCanonical
    };
    NormalizedEvent {
        provider,
        event_timestamp: receipt.observed_at,
        source_kind: SourceKind::ExecutionReceipt,
        request_id,
        source_ordinal: ordinal,
        model: "unknown".to_owned(),
        project: "unknown".to_owned(),
        tokens,
        attribution: Attribution {
            scope: "unknown".to_owned(),
            role: None,
            stage_id: None,
            loom_session_id: None,
            stage_state: StageAttributionState::NotApplicable,
        },
        provenance,
        observation_count: 1,
        changed_usage_fields: 0,
        first_observed_in_range: false,
        true_fresh_start: None,
        tool_names: Vec::new(),
    }
}

fn receipt_tokens(receipt: &ExecutionReceipt, invalid_cache_split: bool) -> ProviderTokenVector {
    let usage = &receipt.usage;
    let cache_read = usage.cache_read_input_tokens;
    let cache_writes = if invalid_cache_split {
        (None, None)
    } else {
        (
            usage.cache_write_5m_input_tokens,
            usage.cache_write_1h_input_tokens,
        )
    };
    let (fresh, resident) = match receipt.provider {
        ReceiptProvider::Claude => (
            Some(usage.input_tokens),
            cache_read
                .zip(usage.cache_creation_input_tokens)
                .map(|(read, creation)| {
                    usage
                        .input_tokens
                        .saturating_add(read)
                        .saturating_add(creation)
                }),
        ),
        ReceiptProvider::Codex => (
            cache_read.and_then(|read| usage.input_tokens.checked_sub(read)),
            Some(usage.input_tokens),
        ),
    };
    ProviderTokenVector {
        input_tokens: Some(usage.input_tokens),
        fresh_input_tokens: fresh,
        cache_creation_input_tokens: usage.cache_creation_input_tokens,
        cache_read_input_tokens: cache_read,
        cache_write_5m_input_tokens: cache_writes.0,
        cache_write_1h_input_tokens: cache_writes.1,
        output_tokens: Some(usage.output_tokens),
        thinking_output_tokens: usage.thinking_output_tokens,
        resident_input_tokens: resident,
        total_tokens: None,
    }
}

fn cache_split_invalid(usage: &ReceiptUsage) -> bool {
    usage
        .cache_creation_input_tokens
        .zip(usage.cache_write_5m_input_tokens)
        .zip(usage.cache_write_1h_input_tokens)
        .is_some_and(|((creation, five_minute), one_hour)| {
            five_minute.checked_add(one_hour) != Some(creation)
        })
}

fn provider(provider: ReceiptProvider) -> Provider {
    match provider {
        ReceiptProvider::Claude => Provider::Claude,
        ReceiptProvider::Codex => Provider::Codex,
    }
}

fn record_decode_error(result: &mut ReceiptNormalization, error: ReceiptDecodeError) {
    increment_diagnostic(result, None, |diagnostics| {
        if error.is_unsupported_version() {
            diagnostics.unsupported_receipt_versions += 1;
        } else {
            diagnostics.malformed_receipts += 1;
        }
    });
}

fn increment_diagnostic(
    result: &mut ReceiptNormalization,
    provider: Option<Provider>,
    update: impl Fn(&mut ProviderDiagnostics),
) {
    if let Some(provider) = provider {
        update(result.diagnostics.entry(provider).or_default());
        return;
    }
    for provider in [Provider::Claude, Provider::Codex] {
        update(result.diagnostics.entry(provider).or_default());
    }
}

fn classify_request_ids(rows: &mut [NormalizedEvent]) {
    classify_by_id(rows, |row| {
        row.request_id.as_ref().map(|id| (row.provider, id.clone()))
    });
}

#[cfg(test)]
#[path = "receipt_provider_tests.rs"]
mod tests;
