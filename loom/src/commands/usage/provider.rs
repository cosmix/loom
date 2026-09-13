use std::path::{Path, PathBuf};

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
    pub(crate) codex_files: &'a [PathBuf],
    pub(crate) codex_missing_roots: usize,
    pub(crate) codex_unreadable_directories: usize,
    pub(crate) receipts_root: Option<&'a Path>,
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
        rows.extend(normalized.rows);
    }
    if input.selection.includes(Provider::Codex) {
        let normalized = super::codex_provider::normalize(input.codex_files, &input.range);
        codex_diagnostics = normalized.diagnostics;
        codex_diagnostics.missing_roots += input.codex_missing_roots;
        codex_diagnostics.unreadable_files += input.codex_unreadable_directories;
        rows.extend(normalized.rows);
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
            rows.push(row);
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
