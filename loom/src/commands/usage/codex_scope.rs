//! Which Codex rows belong to the selected project. The Codex store is shared
//! by every project on the machine, so a row counts only when a forward
//! receipt of this project names its thread; every other row is foreign.

use std::collections::{BTreeSet, HashSet};

use chrono::Duration;

use super::forward_join::{ForwardJoin, ReceiptThread};
use super::provider_types::{NormalizedEvent, Provider, SourceKind};
use super::time_range::TimeRange;

/// Longest a forward runs: the wrapper's unit deadline is 540 s
/// (`codex-forward.sh`, `--timeout-ms 540000`), rounded up. A payload-formed
/// receipt carries the PostToolUse instant, after the forward ended, so a
/// receipt observed less than this after `--since` may belong to a forward
/// whose rows all precede the window.
const MAX_FORWARD_SECS: i64 = 600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodexScope {
    /// `--all`: every Codex row is reported.
    Unscoped,
    /// The receipts could not be read completely; no row can be claimed.
    Unavailable,
    Threads {
        /// Thread ids of every loaded receipt, whatever its `observed_at`:
        /// rollout rows are windowed by their own timestamps, and a forward
        /// that ends after `--until` still owns its in-window rows.
        threads: BTreeSet<String>,
        /// Members of `threads` whose receipt lies in
        /// `[since + MAX_FORWARD_SECS, until]`; the threads that must show a
        /// row for the report to be complete.
        in_window: BTreeSet<String>,
        /// In-window receipts that carry no thread id.
        unthreaded: usize,
    },
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ScopeOutcome {
    pub(crate) foreign_rows: usize,
    pub(crate) missing_threads: usize,
    pub(crate) unthreaded_receipts: usize,
    pub(crate) unavailable: bool,
}

impl CodexScope {
    pub(crate) fn for_selection(all: bool, join: &ForwardJoin, range: &TimeRange) -> Self {
        if all {
            return Self::Unscoped;
        }
        if !join.scope_available() {
            return Self::Unavailable;
        }
        Self::from_receipt_threads(join.receipt_threads(), range)
    }

    pub(crate) fn from_receipt_threads(threads: &[ReceiptThread], range: &TimeRange) -> Self {
        let checked_from = range.since + Duration::seconds(MAX_FORWARD_SECS);
        let observed_by = |receipt: &ReceiptThread, from| {
            receipt.observed_at >= from
                && range.until.is_none_or(|until| receipt.observed_at <= until)
        };
        let mut all = BTreeSet::new();
        let mut in_window = BTreeSet::new();
        let mut unthreaded = 0;
        for receipt in threads {
            match &receipt.thread_id {
                Some(thread_id) => {
                    all.insert(thread_id.clone());
                    if observed_by(receipt, checked_from) {
                        in_window.insert(thread_id.clone());
                    }
                }
                None if observed_by(receipt, range.since) => unthreaded += 1,
                None => {}
            }
        }
        Self::Threads {
            threads: all,
            in_window,
            unthreaded,
        }
    }
}

fn is_rollout_row(row: &NormalizedEvent) -> bool {
    row.provider == Provider::Codex
        && matches!(
            row.source_kind,
            SourceKind::CodexDirect | SourceKind::CodexFallback
        )
}

/// Removes the Codex rollout rows the scope does not claim. Execution-receipt
/// rows and Claude rows are never touched.
pub(crate) fn apply(scope: &CodexScope, rows: &mut Vec<NormalizedEvent>) -> ScopeOutcome {
    let mut outcome = ScopeOutcome::default();
    match scope {
        CodexScope::Unscoped => {}
        CodexScope::Unavailable => {
            outcome.unavailable = true;
            outcome.foreign_rows = remove_where(rows, |_| true);
        }
        CodexScope::Threads {
            threads,
            in_window,
            unthreaded,
        } => {
            outcome.foreign_rows = remove_where(rows, |row| {
                row.codex_thread_conflict
                    || row
                        .codex_thread_id
                        .as_deref()
                        .is_none_or(|thread| !threads.contains(thread))
            });
            outcome.unthreaded_receipts = *unthreaded;
            let seen: HashSet<&str> = rows
                .iter()
                .filter(|row| is_rollout_row(row))
                .filter_map(|row| row.codex_thread_id.as_deref())
                .collect();
            outcome.missing_threads = in_window
                .iter()
                .filter(|thread| !seen.contains(thread.as_str()))
                .count();
        }
    }
    outcome
}

/// Drops the rollout rows `foreign` selects; returns how many went.
fn remove_where(
    rows: &mut Vec<NormalizedEvent>,
    foreign: impl Fn(&NormalizedEvent) -> bool,
) -> usize {
    let before = rows.len();
    rows.retain(|row| !is_rollout_row(row) || !foreign(row));
    before - rows.len()
}

#[cfg(test)]
#[path = "codex_scope_tests.rs"]
mod tests;
