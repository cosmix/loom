//! Out-of-date reasons for summaries, from their evaluated state and the page context.

use crate::plan_server::concept::page::PageContext;
use crate::plan_server::summaries::scheduler::RebuildReason;
use crate::plan_server::wire::concept::{OutOfDate, OutOfDateReason};
use crate::planning::summary::evaluate::SummaryState;

/// Combine evaluated claim state with read-only and scheduler reasons.
pub(crate) fn out_of_date_reason(
    id: &str,
    state: SummaryState,
    ctx: &PageContext,
) -> Option<OutOfDate> {
    let rebuild = ctx.rebuilds.as_ref().and_then(|r| r.out_of_date_reason(id));
    let (reason, detail) = match (state, &ctx.read_only, rebuild) {
        (SummaryState::Current | SummaryState::Missing, _, _) => return None,
        (_, Some(detail), _) => (OutOfDateReason::CannotRebuild, Some(detail.clone())),
        (SummaryState::Rebuilding, _, _) => (OutOfDateReason::Rebuilding, None),
        (_, _, Some(RebuildReason::AwaitingApproval)) => (OutOfDateReason::AwaitingApproval, None),
        (_, _, Some(RebuildReason::Failed(error))) => (OutOfDateReason::Failed, Some(error)),
        (SummaryState::Incomplete | SummaryState::Stale, _, None) => {
            (OutOfDateReason::SourceChanged, None)
        }
    };
    Some(OutOfDate { reason, detail })
}
