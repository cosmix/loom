//! Whether a provider's totals can be trusted to cover everything the
//! selection names. A report is `partial` when any evidence was missing,
//! unreadable or unattributable; the reasons say which.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::provider_types::{CoverageCounts, Provider, ProviderDiagnostics};
use super::time_range::TimeRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CompletenessState {
    Measured,
    Partial,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Completeness {
    pub(crate) state: CompletenessState,
    pub(crate) reasons: Vec<&'static str>,
}

/// What the session records say about Claude transcripts the report did not
/// read: transcripts missed, and records that could not be inspected at all.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionInventory {
    pub(crate) missed: usize,
    pub(crate) unreadable: usize,
}

/// Reason codes in the order they are reported. `fallback_coverage_rows` and
/// `missing_timestamp_rows` stay diagnostics: the first is a coverage count,
/// the second counts every row type without a timestamp, so it would make
/// every Claude report partial.
pub(crate) fn assess(
    provider: Provider,
    coverage: &CoverageCounts,
    diagnostics: &ProviderDiagnostics,
) -> Completeness {
    let damaged_receipts = damaged_receipts(provider, diagnostics);
    let checks = [
        ("missing-roots", diagnostics.missing_roots),
        ("unreadable-files", diagnostics.unreadable_files),
        ("malformed-rows", diagnostics.malformed_rows),
        (
            "unreadable-session-records",
            diagnostics.unreadable_session_records,
        ),
        (
            "missed-session-transcripts",
            diagnostics.missed_session_transcripts,
        ),
        ("unknown-usage-rows", coverage.unknown_usage_rows),
        ("ambiguous-rows", coverage.ambiguous_rows),
        (
            "codex-scope-unavailable",
            diagnostics.codex_scope_unavailable,
        ),
        ("malformed-forward-receipts", damaged_receipts),
        (
            "forward-thread-missing",
            diagnostics.missing_forward_threads,
        ),
        (
            "forward-receipt-unthreaded",
            diagnostics.unthreaded_forward_receipts,
        ),
    ];
    let reasons: Vec<&'static str> = checks
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(reason, _)| reason)
        .collect();
    let state = if reasons.is_empty() {
        CompletenessState::Measured
    } else {
        CompletenessState::Partial
    };
    Completeness { state, reasons }
}

/// A damaged receipt line can hide a project thread, whose rows then read
/// foreign. Claude totals do not depend on receipts.
fn damaged_receipts(provider: Provider, diagnostics: &ProviderDiagnostics) -> usize {
    if provider == Provider::Codex {
        diagnostics.malformed_forward_receipts + diagnostics.truncated_forward_receipt_files
    } else {
        0
    }
}

/// Counts session records of the selected work whose transcript lies in the
/// window but was not among the files read. Reads the records directly; the
/// session-file lock would write a lock file from a read-only report.
pub(crate) fn missed_claude_transcripts(
    work_dir: Option<&Path>,
    read: &[PathBuf],
    range: &TimeRange,
    stages: Option<&HashSet<String>>,
) -> SessionInventory {
    let mut inventory = SessionInventory::default();
    let Some(work_dir) = work_dir else {
        return inventory;
    };
    let enumeration = match crate::fs::session_files::enumerate_sessions(work_dir) {
        Ok(enumeration) => enumeration,
        Err(error) => {
            eprintln!("loom usage: cannot enumerate session records: {error:#}");
            inventory.unreadable = 1;
            return inventory;
        }
    };
    // A record that failed enumeration cannot be attributed to a stage.
    inventory.unreadable += enumeration.diagnostics.len();
    for record in &enumeration.records {
        let selected = stages.is_none_or(|stages| {
            record
                .stage_id
                .as_deref()
                .is_some_and(|stage| stages.contains(stage))
        });
        if !selected {
            continue;
        }
        match record_transcript(&record.path, range) {
            Ok(Some(path)) if !was_read(&path, read) => inventory.missed += 1,
            Ok(_) => {}
            Err(error) => {
                eprintln!(
                    "loom usage: cannot read session record {}: {error:#}",
                    record.path.display()
                );
                inventory.unreadable += 1;
            }
        }
    }
    inventory
}

/// The record's transcript path when the session was active in the window.
fn record_transcript(path: &Path, range: &TimeRange) -> anyhow::Result<Option<PathBuf>> {
    use anyhow::Context;

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let session = crate::parser::frontmatter::parse_from_markdown::<crate::models::session::Session>(
        &content, "Session",
    )?;
    let in_window = session.last_active >= range.since
        && range.until.is_none_or(|until| session.created_at <= until);
    Ok(session
        .transcript_path
        .filter(|_| in_window)
        .map(PathBuf::from))
}

fn was_read(path: &Path, read: &[PathBuf]) -> bool {
    if read.iter().any(|candidate| candidate == path) {
        return true;
    }
    let Ok(canonical) = path.canonicalize() else {
        return false;
    };
    read.iter().any(|candidate| {
        candidate
            .canonicalize()
            .is_ok_and(|other| other == canonical)
    })
}

#[cfg(test)]
#[path = "completeness_tests.rs"]
mod tests;
