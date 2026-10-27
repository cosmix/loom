//! Tests for out-of-date reasons.

use super::state::*;
use crate::plan_server::concept::page::PageContext;
use crate::plan_server::wire::concept::{OutOfDate, OutOfDateReason};
use crate::planning::summary::evaluate::SummaryState;

#[test]
fn current_gives_none() {
    assert_eq!(
        out_of_date_reason("id", SummaryState::Current, &PageContext::default()),
        None
    );
}

#[test]
fn stale_states_give_source_changed() {
    for state in [SummaryState::Stale, SummaryState::Incomplete] {
        assert_eq!(
            out_of_date_reason("id", state, &PageContext::default()),
            Some(OutOfDate {
                reason: OutOfDateReason::SourceChanged,
                detail: None
            })
        );
    }
}

#[test]
fn rebuilding_gives_rebuilding() {
    assert_eq!(
        out_of_date_reason("id", SummaryState::Rebuilding, &PageContext::default()),
        Some(OutOfDate {
            reason: OutOfDateReason::Rebuilding,
            detail: None
        })
    );
}

#[test]
fn missing_gives_none() {
    assert_eq!(
        out_of_date_reason("id", SummaryState::Missing, &PageContext::default()),
        None
    );
}

// Plan 10 adds fields; the struct update keeps this literal compiling.
#[allow(clippy::needless_update)]
#[test]
fn read_only_turns_reasons_into_cannot_rebuild() {
    let ctx = PageContext {
        read_only: Some("locked".into()),
        ..Default::default()
    };
    for state in [
        SummaryState::Stale,
        SummaryState::Incomplete,
        SummaryState::Rebuilding,
    ] {
        assert_eq!(
            out_of_date_reason("id", state, &ctx),
            Some(OutOfDate {
                reason: OutOfDateReason::CannotRebuild,
                detail: Some("locked".into()),
            })
        );
    }
    for state in [SummaryState::Current, SummaryState::Missing] {
        assert_eq!(out_of_date_reason("id", state, &ctx), None);
    }
}
