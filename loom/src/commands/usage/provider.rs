use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::codex_scope::{self, CodexScope};
use super::completeness::SessionInventory;
use super::provider_types::{
    NormalizedEvent, Provider, ProviderDiagnostics, ProviderLedger, TimeRangeView,
    PROVIDER_LEDGER_SCHEMA_VERSION,
};
use super::time_range::TimeRange;
use super::transcript::Transcript;
use super::ProviderSelection;

pub(crate) struct ProviderEventInput<'a> {
    pub(crate) selection: ProviderSelection,
    pub(crate) range: TimeRange,
    pub(crate) claude_transcripts: Vec<Transcript>,
    pub(crate) claude_discovered_files: usize,
    pub(crate) claude_missing_roots: usize,
    pub(crate) claude_sessions: SessionInventory,
    pub(crate) codex_files: &'a [PathBuf],
    pub(crate) codex_missing_roots: usize,
    pub(crate) codex_unreadable_directories: usize,
    pub(crate) codex_scope: &'a CodexScope,
    pub(crate) receipts_root: Option<&'a Path>,
    pub(crate) selected_stages: Option<&'a HashSet<String>>,
    pub(crate) selection_unavailable: bool,
}

pub(crate) struct NormalizedProviderEvents {
    pub(crate) claude_transcripts: Vec<Transcript>,
    pub(crate) ledger: ProviderLedger,
}

pub(crate) fn normalize_provider_events(
    mut input: ProviderEventInput<'_>,
) -> NormalizedProviderEvents {
    let mut rows = Vec::new();
    let mut claude_diagnostics = ProviderDiagnostics::default();
    let mut codex_diagnostics = ProviderDiagnostics::default();
    if input.selection.includes(Provider::Claude) {
        let normalized = super::claude_provider::normalize(&mut input.claude_transcripts);
        claude_diagnostics = normalized.diagnostics;
        claude_diagnostics.files_seen = input.claude_discovered_files;
        claude_diagnostics.unreadable_files += input
            .claude_discovered_files
            .saturating_sub(input.claude_transcripts.len());
        claude_diagnostics.missing_roots += input.claude_missing_roots;
        claude_diagnostics.missed_session_transcripts = input.claude_sessions.missed;
        claude_diagnostics.unreadable_session_records = input.claude_sessions.unreadable;
        claude_diagnostics.selection_unavailable = usize::from(input.selection_unavailable);
        rows.extend(normalized.rows);
    }
    if input.selection.includes(Provider::Codex) {
        let (codex_rows, diagnostics) = normalize_codex(&input);
        codex_diagnostics = diagnostics;
        rows.extend(codex_rows);
    }
    merge_receipts(
        &input,
        &mut rows,
        &mut claude_diagnostics,
        &mut codex_diagnostics,
    );
    rows.sort_by_key(|row| {
        (
            row.event_timestamp,
            row.provider,
            row.source_kind,
            row.source_ordinal,
        )
    });
    let providers = selected_reports(
        input.selection,
        &rows,
        claude_diagnostics,
        codex_diagnostics,
    );
    NormalizedProviderEvents {
        claude_transcripts: input.claude_transcripts,
        ledger: build_ledger(input.range, providers, rows),
    }
}

/// Codex rollout rows of the selection, reduced to the threads the project's
/// forward receipts name.
fn normalize_codex(input: &ProviderEventInput<'_>) -> (Vec<NormalizedEvent>, ProviderDiagnostics) {
    let normalized = super::codex_provider::normalize_unclassified(
        input.codex_files,
        &input.range,
        input.codex_scope,
    );
    let mut diagnostics = normalized.diagnostics;
    let mut rows = normalized.rows;
    diagnostics.missing_roots += input.codex_missing_roots;
    diagnostics.unreadable_files += input.codex_unreadable_directories;
    let outcome = codex_scope::apply(input.codex_scope, &mut rows);
    super::codex_provider::classify_response_ids(&mut rows);
    diagnostics.foreign_codex_rows = outcome.foreign_rows;
    diagnostics.missing_forward_threads = outcome.missing_threads;
    diagnostics.unthreaded_forward_receipts = outcome.unthreaded_receipts;
    diagnostics.codex_scope_unavailable = usize::from(outcome.unavailable);
    diagnostics.ambiguous_forward_threads = outcome.ambiguous_threads;
    (rows, diagnostics)
}

fn build_ledger(
    range: TimeRange,
    providers: Vec<super::provider_types::ProviderReport>,
    rows: Vec<NormalizedEvent>,
) -> ProviderLedger {
    ProviderLedger {
        schema_version: PROVIDER_LEDGER_SCHEMA_VERSION,
        range: TimeRangeView {
            since: range.since,
            until: range.until,
        },
        providers,
        rows,
        quota_history: None,
    }
}

fn merge_receipts(
    input: &ProviderEventInput<'_>,
    rows: &mut Vec<NormalizedEvent>,
    claude: &mut ProviderDiagnostics,
    codex: &mut ProviderDiagnostics,
) {
    let receipts = super::receipt_provider::normalize(input.receipts_root, &input.range);
    for row in receipts.rows {
        if input.selection.includes(row.provider) {
            if input.selected_stages.is_some() {
                match row.provider {
                    Provider::Claude => claude.unscoped_execution_receipts += 1,
                    Provider::Codex => codex.unscoped_execution_receipts += 1,
                }
            } else {
                rows.push(row);
            }
        }
    }
    for (provider, diagnostics) in receipts.diagnostics {
        if !input.selection.includes(provider) {
            continue;
        }
        match provider {
            Provider::Claude => claude.merge(diagnostics),
            Provider::Codex => codex.merge(diagnostics),
        }
    }
}

fn selected_reports(
    selection: ProviderSelection,
    rows: &[NormalizedEvent],
    claude: ProviderDiagnostics,
    codex: ProviderDiagnostics,
) -> Vec<super::provider_types::ProviderReport> {
    let mut reports = Vec::new();
    if selection.includes(Provider::Claude) {
        reports.push(super::provider_report::build(
            Provider::Claude,
            rows,
            claude,
        ));
    }
    if selection.includes(Provider::Codex) {
        reports.push(super::provider_report::build(Provider::Codex, rows, codex));
    }
    reports
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    #[test]
    fn stage_selection_excludes_execution_receipt_without_stage_evidence() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("one.json"),
            json!({
                "schema_version": 1,
                "provider": "claude",
                "observed_at": "2026-09-12T20:00:00Z",
                "request_id": "receipt-1",
                "usage": {"input_tokens": 10, "output_tokens": 2}
            })
            .to_string(),
        )
        .unwrap();
        let stages = HashSet::from(["alpha".to_owned()]);
        let range = TimeRange {
            since: Utc.with_ymd_and_hms(2026, 9, 12, 0, 0, 0).single().unwrap(),
            until: None,
        };
        let normalized = normalize_provider_events(ProviderEventInput {
            selection: ProviderSelection::Claude,
            range,
            claude_transcripts: Vec::new(),
            claude_discovered_files: 0,
            claude_missing_roots: 0,
            claude_sessions: SessionInventory::default(),
            codex_files: &[],
            codex_missing_roots: 0,
            codex_unreadable_directories: 0,
            codex_scope: &CodexScope::Unscoped,
            receipts_root: Some(root.path()),
            selected_stages: Some(&stages),
            selection_unavailable: false,
        });
        assert!(normalized.ledger.rows.is_empty());
        let report = &normalized.ledger.providers[0];
        assert_eq!(report.diagnostics.unscoped_execution_receipts, 1);
        assert!(report
            .completeness
            .reasons
            .contains(&"execution-receipt-unscoped"));
    }
}
