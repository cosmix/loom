use chrono::{DateTime, TimeZone, Utc};

use super::*;
use crate::commands::usage::provider_types::{
    Attribution, ProvenanceStatus, ProviderTokenVector, StageAttributionState,
};

fn at(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 14, hour, minute, second)
        .single()
        .expect("valid fixture timestamp")
}

fn window(until: Option<DateTime<Utc>>) -> TimeRange {
    TimeRange {
        since: at(10, 0, 0),
        until,
    }
}

fn row(source_kind: SourceKind, thread: Option<&str>, timestamp: DateTime<Utc>) -> NormalizedEvent {
    NormalizedEvent {
        provider: Provider::Codex,
        event_timestamp: timestamp,
        source_kind,
        request_id: None,
        source_ordinal: 0,
        model: "gpt".to_owned(),
        project: "project".to_owned(),
        tokens: ProviderTokenVector::default(),
        attribution: Attribution {
            scope: "main".to_owned(),
            role: None,
            stage_id: None,
            loom_session_id: None,
            stage_state: StageAttributionState::NotApplicable,
        },
        provenance: ProvenanceStatus::MeasuredCanonical,
        observation_count: 1,
        changed_usage_fields: 0,
        first_observed_in_range: true,
        true_fresh_start: None,
        tool_names: Vec::new(),
        codex_thread_id: thread.map(str::to_owned),
        codex_thread_conflict: false,
        forward_candidate: false,
        forward_receipt: None,
    }
}

fn rollout(thread: &str) -> NormalizedEvent {
    row(SourceKind::CodexDirect, Some(thread), at(10, 5, 0))
}

fn receipt(thread: Option<&str>, observed_at: DateTime<Utc>) -> ReceiptThread {
    ReceiptThread {
        thread_id: thread.map(str::to_owned),
        observed_at,
    }
}

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

fn threads(ids: &[&str]) -> CodexScope {
    CodexScope::Threads {
        threads: set(ids),
        in_window: set(ids),
        unthreaded: 0,
    }
}

fn thread_ids(rows: &[NormalizedEvent]) -> Vec<&str> {
    rows.iter()
        .filter_map(|row| row.codex_thread_id.as_deref())
        .collect()
}

#[test]
fn thread_scope_keeps_named_threads_and_counts_foreign_and_missing() {
    let mut rows = vec![rollout("t-a"), rollout("t-b"), rollout("t-x")];

    let outcome = apply(&threads(&["t-a", "t-missing"]), &mut rows);

    assert_eq!(thread_ids(&rows), ["t-a"]);
    assert_eq!(
        outcome,
        ScopeOutcome {
            foreign_rows: 2,
            missing_threads: 1,
            unthreaded_receipts: 0,
            unavailable: false,
        }
    );
}

#[test]
fn unavailable_scope_removes_every_rollout_row() {
    let mut rows = vec![rollout("t-a"), rollout("t-b"), rollout("t-x")];

    let outcome = apply(&CodexScope::Unavailable, &mut rows);

    assert!(rows.is_empty());
    assert_eq!(outcome.foreign_rows, 3);
    assert!(outcome.unavailable);
}

#[test]
fn unscoped_keeps_every_row() {
    let mut rows = vec![rollout("t-a"), rollout("t-b"), rollout("t-x")];

    let outcome = apply(&CodexScope::Unscoped, &mut rows);

    assert_eq!(rows.len(), 3);
    assert_eq!(outcome, ScopeOutcome::default());
}

#[test]
fn execution_receipt_rows_and_conflicting_threads() {
    let mut conflicted = rollout("t-a");
    conflicted.codex_thread_conflict = true;
    for scope in [
        CodexScope::Unscoped,
        CodexScope::Unavailable,
        threads(&["t-a"]),
    ] {
        let mut rows = vec![row(SourceKind::ExecutionReceipt, None, at(10, 5, 0))];
        apply(&scope, &mut rows);
        assert_eq!(
            rows.len(),
            1,
            "execution receipt row must survive {scope:?}"
        );
    }

    let mut rows = vec![conflicted, rollout("t-a")];
    let outcome = apply(&threads(&["t-a"]), &mut rows);
    assert_eq!(rows.len(), 1);
    assert_eq!(outcome.foreign_rows, 1);
}

#[test]
fn receipt_after_until_still_claims_in_window_rows() {
    let range = window(Some(at(10, 30, 0)));
    let scope = CodexScope::from_receipt_threads(&[receipt(Some("t-a"), at(10, 40, 0))], &range);
    let mut rows = vec![rollout("t-a")];

    let outcome = apply(&scope, &mut rows);

    assert_eq!(thread_ids(&rows), ["t-a"]);
    assert_eq!(outcome.foreign_rows, 0);
    assert_eq!(outcome.missing_threads, 0);
}

#[test]
fn terminal_receipt_past_the_cutoff_never_removes_usage() {
    let range = window(Some(at(10, 30, 0)));
    let running = CodexScope::from_receipt_threads(&[receipt(Some("t-a"), at(10, 0, 0))], &range);
    let folded = CodexScope::from_receipt_threads(&[receipt(Some("t-a"), at(10, 40, 0))], &range);

    for scope in [running, folded] {
        let mut rows = vec![rollout("t-a")];
        let outcome = apply(&scope, &mut rows);
        assert_eq!(thread_ids(&rows), ["t-a"]);
        assert_eq!(outcome.foreign_rows, 0);
    }
}

#[test]
fn receipt_just_after_since_without_rows_is_not_missing() {
    let range = window(None);
    let scope = CodexScope::from_receipt_threads(&[receipt(Some("t-a"), at(10, 1, 0))], &range);
    let mut rows = Vec::new();

    assert_eq!(apply(&scope, &mut rows).missing_threads, 0);
}

#[test]
fn receipt_well_after_since_without_rows_is_missing() {
    let range = window(None);
    let observed = range.since + Duration::seconds(700);
    let scope = CodexScope::from_receipt_threads(&[receipt(Some("t-a"), observed)], &range);
    let mut rows = Vec::new();

    assert_eq!(apply(&scope, &mut rows).missing_threads, 1);
}

#[test]
fn unthreaded_receipts_count_only_inside_the_window() {
    let range = window(Some(at(11, 0, 0)));
    let scope = CodexScope::from_receipt_threads(
        &[
            receipt(None, at(10, 1, 0)),
            receipt(None, at(9, 0, 0)),
            receipt(None, at(12, 0, 0)),
        ],
        &range,
    );
    let mut rows = Vec::new();

    assert_eq!(apply(&scope, &mut rows).unthreaded_receipts, 1);
}
