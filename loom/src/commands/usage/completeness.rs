//! Whether a provider's totals can be trusted to cover everything the
//! selection names. A report is `partial` when any evidence was missing,
//! unreadable or unattributable; the reasons say which.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::provider_types::{CoverageCounts, Provider, ProviderDiagnostics};
use super::time_range::TimeRange;
use crate::context::untrusted::inline_safe;

/// The bound `enumerate_sessions` puts on a session record read.
const MAX_RECORD_BYTES: usize = 1024 * 1024;

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
    // From a stage worktree `.loom/work` is a symlink to the main checkout's
    // work dir, and the bounded record reads open their root without following
    // links: resolve it once, or every record reads as invalid.
    let root = std::fs::canonicalize(work_dir).unwrap_or_else(|_| work_dir.to_path_buf());
    let enumeration = match crate::fs::session_files::enumerate_sessions(&root) {
        Ok(enumeration) => enumeration,
        Err(error) => {
            eprintln!("loom usage: cannot enumerate session records: {error:#}");
            inventory.unreadable = 1;
            return inventory;
        }
    };
    // A record that failed enumeration cannot be attributed to a stage. Its
    // file name and parse error are session-controlled text bound for a terminal.
    for diagnostic in &enumeration.diagnostics {
        eprintln!("loom usage: {}", inline_safe(diagnostic));
    }
    inventory.unreadable += enumeration.diagnostics.len();
    let read = ReadSet::new(read);
    for record in &enumeration.records {
        if !is_selected(record.stage_id.as_deref(), stages) {
            continue;
        }
        match record_transcript(&root, &record.path, range) {
            Ok(Some(path)) if !read.contains(&path) => inventory.missed += 1,
            Ok(_) => {}
            Err(error) => {
                eprintln!(
                    "loom usage: cannot read session record {}: {}",
                    inline_safe(&record.path.display().to_string()),
                    inline_safe(&format!("{error:#}"))
                );
                inventory.unreadable += 1;
            }
        }
    }
    inventory
}

fn is_selected(stage_id: Option<&str>, stages: Option<&HashSet<String>>) -> bool {
    stages.is_none_or(|stages| stage_id.is_some_and(|stage| stages.contains(stage)))
}

/// The record's transcript path when the session was active in the window.
/// `path` lies under `root`; the read is bounded and follows no symlink, as
/// `enumerate_sessions` reads the same record.
fn record_transcript(
    root: &Path,
    path: &Path,
    range: &TimeRange,
) -> anyhow::Result<Option<PathBuf>> {
    use anyhow::Context;

    let relative = path.strip_prefix(root)?;
    let content = crate::fs::safe_read::read_to_string_bounded(root, relative, MAX_RECORD_BYTES)
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

/// The transcripts already read, as given and canonicalized once, so a record
/// costs one canonicalization rather than one per read file.
struct ReadSet<'a> {
    literal: HashSet<&'a Path>,
    canonical: HashSet<PathBuf>,
}

impl<'a> ReadSet<'a> {
    fn new(read: &'a [PathBuf]) -> Self {
        Self {
            literal: read.iter().map(PathBuf::as_path).collect(),
            canonical: read
                .iter()
                .filter_map(|path| path.canonicalize().ok())
                .collect(),
        }
    }

    fn contains(&self, path: &Path) -> bool {
        self.literal.contains(path)
            || path
                .canonicalize()
                .is_ok_and(|canonical| self.canonical.contains(&canonical))
    }
}

#[cfg(test)]
#[path = "completeness_tests.rs"]
mod tests;
