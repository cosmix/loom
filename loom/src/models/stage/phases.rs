//! Work phases of a stage, recorded as spans on `Stage::phases`.
//!
//! `verify::transitions::persistence` calls [`record`] on every persisted status
//! or session change, so each span edge is a real transition time.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::types::StageStatus;
use crate::models::session::SessionType;

/// Most spans a stage keeps. The last slot is the overflow [`Phase::Waiting`] span.
pub const MAX_PHASE_SPANS: usize = 512;

/// What a stage spends a stretch of its life doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// A contract session is writing the behavioural tests.
    Contract,
    /// The first attempt of a stage or knowledge session.
    Executing,
    /// An adjudicator is judging a disputed criterion.
    Adjudication,
    /// A stage or knowledge session working after a retry.
    Retry,
    /// Inactive: queued again, blocked, merging or recovering.
    Waiting,
}

/// One stretch of a [`Phase`]; `ended_at` is `None` while it is the current one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhaseSpan {
    pub phase: Phase,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

/// The phase a stage is in for its persisted state, or `None` when no span is open.
///
/// `retry_count` splits `Executing` from `Retry` for stage and knowledge sessions
/// (the sessions `orchestrator/core/inbox_drain/complete.rs` admits). `has_history`
/// says whether a span was recorded already: before the first work phase the
/// pending and inactive states add no span, and afterwards they are `Waiting`.
/// `Executing` with a merge session, or with no readable session, is inactive too.
pub fn phase_for(
    status: StageStatus,
    session_type: Option<SessionType>,
    retry_count: u32,
    has_history: bool,
) -> Option<Phase> {
    let inactive = has_history.then_some(Phase::Waiting);
    match status {
        StageStatus::Completed | StageStatus::Skipped => None,
        StageStatus::NeedsAdjudication => Some(Phase::Adjudication),
        StageStatus::Executing => match session_type {
            Some(SessionType::Contract) => Some(Phase::Contract),
            Some(SessionType::Adjudication) => Some(Phase::Adjudication),
            Some(SessionType::Stage | SessionType::Knowledge) if retry_count == 0 => {
                Some(Phase::Executing)
            }
            Some(SessionType::Stage | SessionType::Knowledge) => Some(Phase::Retry),
            Some(SessionType::Merge | SessionType::BaseConflict) | None => inactive,
        },
        _ => inactive,
    }
}

/// Move `spans` to `phase` at `at`: close the open span when the phase differs,
/// then open the new one. `None` closes without opening.
///
/// `at` is raised to the last recorded edge, so a clock that ran backwards never
/// yields a span that ends before it starts. At [`MAX_PHASE_SPANS`] the first spans
/// stay and the last slot becomes one `Waiting` span standing for the remainder; it
/// is closed by the terminal `None` and nothing is added after it.
pub fn record(spans: &mut Vec<PhaseSpan>, phase: Option<Phase>, at: DateTime<Utc>) {
    if phase.is_some() && spans.len() >= MAX_PHASE_SPANS {
        return;
    }
    let at = match spans.last() {
        Some(last) => at.max(last.ended_at.unwrap_or(last.started_at)),
        None => at,
    };
    if let Some(last) = spans.last_mut().filter(|last| last.ended_at.is_none()) {
        if Some(last.phase) == phase {
            return;
        }
        last.ended_at = Some(at);
    }
    let Some(phase) = phase else { return };
    let phase = if spans.len() + 1 == MAX_PHASE_SPANS {
        Phase::Waiting
    } else {
        phase
    };
    spans.push(PhaseSpan {
        phase,
        started_at: at,
        ended_at: None,
    });
}
