use chrono::{DateTime, TimeZone, Utc};

use super::phases::{phase_for, record, Phase, PhaseSpan, MAX_PHASE_SPANS};
use super::StageStatus;
use crate::models::session::SessionType;

fn at(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + secs, 0).unwrap()
}

fn open(phase: Phase, start: i64) -> PhaseSpan {
    PhaseSpan {
        phase,
        started_at: at(start),
        ended_at: None,
    }
}

#[test]
fn record_opens_the_first_span() {
    let mut spans = Vec::new();
    record(&mut spans, Some(Phase::Executing), at(1));
    assert_eq!(spans, vec![open(Phase::Executing, 1)]);
}

#[test]
fn record_closes_the_open_span_and_opens_the_next() {
    let mut spans = vec![open(Phase::Executing, 1)];
    record(&mut spans, Some(Phase::Adjudication), at(5));
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].ended_at, Some(at(5)));
    assert_eq!(spans[1], open(Phase::Adjudication, 5));
}

#[test]
fn record_ignores_an_unchanged_phase() {
    let mut spans = vec![open(Phase::Retry, 1)];
    record(&mut spans, Some(Phase::Retry), at(9));
    assert_eq!(spans, vec![open(Phase::Retry, 1)]);
}

#[test]
fn record_none_closes_without_reopening_and_is_idempotent() {
    let mut spans = vec![open(Phase::Executing, 1)];
    record(&mut spans, None, at(4));
    record(&mut spans, None, at(8));
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].ended_at, Some(at(4)));
    record(&mut Vec::new(), None, at(1));
}

#[test]
fn record_after_a_close_opens_a_new_span() {
    let mut spans = vec![open(Phase::Executing, 1)];
    record(&mut spans, None, at(4));
    record(&mut spans, Some(Phase::Waiting), at(6));
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[1], open(Phase::Waiting, 6));
}

#[test]
fn a_clock_that_runs_backwards_never_ends_a_span_before_it_starts() {
    let mut spans = vec![open(Phase::Executing, 100)];
    record(&mut spans, Some(Phase::Waiting), at(40));
    assert_eq!(spans[0].ended_at, Some(at(100)));
    assert_eq!(spans[1].started_at, at(100));
    record(&mut spans, None, at(10));
    assert_eq!(spans[1].ended_at, Some(at(100)));
}

/// Alternate Executing and Waiting until `count` spans exist, the last one open.
fn alternating(count: usize) -> Vec<PhaseSpan> {
    let mut spans = Vec::new();
    for index in 0..count {
        let phase = if index % 2 == 0 {
            Phase::Executing
        } else {
            Phase::Waiting
        };
        record(&mut spans, Some(phase), at(index as i64));
    }
    spans
}

#[test]
fn the_last_slot_becomes_one_overflow_waiting_span_closed_at_the_end() {
    let mut spans = alternating(MAX_PHASE_SPANS - 1);
    assert_eq!(spans.len(), MAX_PHASE_SPANS - 1);
    let first_start = spans[0].started_at;

    // The 512th change lands in the overflow slot, even for a work phase.
    let edge = (MAX_PHASE_SPANS) as i64;
    record(&mut spans, Some(Phase::Retry), at(edge));
    assert_eq!(spans.len(), MAX_PHASE_SPANS);
    assert_eq!(spans.last().unwrap().phase, Phase::Waiting);
    assert_eq!(spans.last().unwrap().started_at, at(edge));

    // Later changes add nothing.
    record(&mut spans, Some(Phase::Executing), at(edge + 5));
    record(&mut spans, Some(Phase::Adjudication), at(edge + 6));
    assert_eq!(spans.len(), MAX_PHASE_SPANS);
    assert_eq!(spans.last().unwrap().ended_at, None);

    // The terminal transition closes the overflow span; the first start is intact.
    record(&mut spans, None, at(edge + 20));
    assert_eq!(spans.last().unwrap().ended_at, Some(at(edge + 20)));
    assert_eq!(spans[0].started_at, first_start);
    assert!(spans.iter().all(|span| span.ended_at.is_some()));
}

#[test]
fn a_terminal_close_below_the_cap_adds_no_overflow_span() {
    let mut spans = alternating(MAX_PHASE_SPANS - 1);
    record(&mut spans, None, at(900));
    assert_eq!(spans.len(), MAX_PHASE_SPANS - 1);
    assert_eq!(spans.last().unwrap().ended_at, Some(at(900)));
}

#[test]
fn phase_for_work_phases_follow_the_session_and_retry_count() {
    let executing = |kind, retries| phase_for(StageStatus::Executing, Some(kind), retries, true);
    assert_eq!(executing(SessionType::Contract, 0), Some(Phase::Contract));
    assert_eq!(executing(SessionType::Contract, 2), Some(Phase::Contract));
    assert_eq!(executing(SessionType::Stage, 0), Some(Phase::Executing));
    assert_eq!(executing(SessionType::Stage, 1), Some(Phase::Retry));
    assert_eq!(executing(SessionType::Knowledge, 0), Some(Phase::Executing));
    assert_eq!(executing(SessionType::Knowledge, 3), Some(Phase::Retry));
    assert_eq!(
        executing(SessionType::Adjudication, 0),
        Some(Phase::Adjudication)
    );
    assert_eq!(executing(SessionType::Merge, 0), Some(Phase::Waiting));
    assert_eq!(
        executing(SessionType::BaseConflict, 0),
        Some(Phase::Waiting)
    );
}

#[test]
fn phase_for_merge_sessions_and_missing_sessions_add_no_first_span() {
    for kind in [
        None,
        Some(SessionType::Merge),
        Some(SessionType::BaseConflict),
    ] {
        assert_eq!(phase_for(StageStatus::Executing, kind, 0, false), None);
        assert_eq!(
            phase_for(StageStatus::Executing, kind, 0, true),
            Some(Phase::Waiting)
        );
    }
}

#[test]
fn phase_for_adjudication_status_needs_no_session() {
    for history in [false, true] {
        assert_eq!(
            phase_for(StageStatus::NeedsAdjudication, None, 0, history),
            Some(Phase::Adjudication)
        );
    }
}

#[test]
fn phase_for_terminal_statuses_close() {
    for status in [StageStatus::Completed, StageStatus::Skipped] {
        assert_eq!(phase_for(status.clone(), None, 0, true), None);
        assert_eq!(phase_for(status, None, 0, false), None);
    }
}

#[test]
fn phase_for_inactive_statuses_wait_only_after_the_first_work_phase() {
    let inactive = [
        StageStatus::WaitingForDeps,
        StageStatus::Queued,
        StageStatus::WaitingForInput,
        StageStatus::Blocked,
        StageStatus::NeedsHandoff,
        StageStatus::MergeConflict,
        StageStatus::CompletedWithFailures,
        StageStatus::MergeBlocked,
        StageStatus::NeedsHumanReview,
    ];
    for status in inactive {
        assert_eq!(phase_for(status.clone(), None, 1, false), None, "{status}");
        assert_eq!(
            phase_for(status.clone(), None, 1, true),
            Some(Phase::Waiting),
            "{status}"
        );
    }
}

#[test]
fn phases_serialize_with_snake_case_names() {
    let names: Vec<String> = [
        Phase::Contract,
        Phase::Executing,
        Phase::Adjudication,
        Phase::Retry,
        Phase::Waiting,
    ]
    .iter()
    .map(|phase| {
        serde_json::to_value(phase)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string()
    })
    .collect();
    assert_eq!(
        names,
        ["contract", "executing", "adjudication", "retry", "waiting"]
    );
}

#[test]
fn an_empty_history_is_not_serialized_and_reads_back_empty() {
    let stage = super::Stage::default();
    let json = serde_json::to_value(&stage).unwrap();
    assert!(json.get("phases").is_none());
    let mut with_history = stage;
    with_history.phases = vec![open(Phase::Executing, 0)];
    let round: super::Stage =
        serde_json::from_value(serde_json::to_value(&with_history).unwrap()).unwrap();
    assert_eq!(round.phases, with_history.phases);
}
