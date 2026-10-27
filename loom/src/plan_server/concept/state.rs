//! Out-of-date reasons for summaries, from their evaluated state and the page context.

use crate::plan_server::concept::page::PageContext;
use crate::plan_server::wire::concept::{OutOfDate, OutOfDateReason};
use crate::planning::summary::evaluate::SummaryState;

/// Plan 10 reads the summary ID when determining the reason.
pub(crate) fn out_of_date_reason(
    _id: &str,
    state: SummaryState,
    ctx: &PageContext,
) -> Option<OutOfDate> {
    let reason = match state {
        SummaryState::Current | SummaryState::Missing => return None,
        SummaryState::Incomplete | SummaryState::Stale => OutOfDateReason::SourceChanged,
        SummaryState::Rebuilding => OutOfDateReason::Rebuilding,
    };
    Some(match &ctx.read_only {
        Some(detail) => OutOfDate {
            reason: OutOfDateReason::CannotRebuild,
            detail: Some(detail.clone()),
        },
        None => OutOfDate {
            reason,
            detail: None,
        },
    })
}
