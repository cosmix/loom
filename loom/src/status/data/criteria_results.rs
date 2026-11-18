//! The acceptance criteria that failed in a blocked stage's last completion
//! attempt, for the attention card.
//!
//! The source is the stage session's completion checkpoint: its latest attempt
//! lists each criterion as `acceptance-<index>`, the position in the stage's
//! acceptance list that `loom stage amend --index` counts from 0. An id that is
//! not of that form (`goal-backward`) or points past the list is not a command
//! the operator can amend, so it is left out. No free-text reason is parsed.

use serde::{Deserialize, Serialize};

use crate::fs::work_dir::WorkDir;
use crate::handoff::load_session_checkpoint;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};

/// Most failing criteria one stage contributes to the shared payload.
const MAX_FAILING: usize = 20;

/// Prefix of a recorded criterion id; the rest is the acceptance index.
const ACCEPTANCE_ID_PREFIX: &str = "acceptance-";

/// One acceptance criterion that did not pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailingCriterion {
    /// Position in the acceptance list, counted from 0.
    pub index: usize,
    pub command: String,
    /// First line of the attempt's diagnostic, shown only when exactly one
    /// criterion failed: with several, the line cannot be pinned to one.
    pub diagnostic: Option<String>,
}

/// The failing criteria of a blocked stage; empty for every other stage, and
/// when no attempt recorded which criteria failed.
pub(super) fn failing_criteria(
    stage: &Stage,
    sessions: &[Session],
    work_dir: &WorkDir,
) -> Vec<FailingCriterion> {
    if stage.status != StageStatus::Blocked {
        return Vec::new();
    }
    let Some(session_id) = last_session_id(stage, sessions) else {
        return Vec::new();
    };
    let latest = load_session_checkpoint(&stage.id, &session_id, work_dir.root())
        .ok()
        .flatten()
        .and_then(|checkpoint| checkpoint.latest);
    let Some(latest) = latest else {
        return Vec::new();
    };
    let mut failing: Vec<FailingCriterion> = latest
        .verification
        .criteria
        .iter()
        .filter(|criterion| !criterion.passed)
        .filter_map(|criterion| {
            let index: usize = criterion
                .id
                .strip_prefix(ACCEPTANCE_ID_PREFIX)?
                .parse()
                .ok()?;
            let command = stage.acceptance.get(index)?.command().to_owned();
            Some(FailingCriterion {
                index,
                command,
                diagnostic: None,
            })
        })
        .take(MAX_FAILING)
        .collect();
    if let [only] = failing.as_mut_slice() {
        only.diagnostic = latest.diagnostic_first_line.clone();
    }
    failing
}

/// The session whose attempt the block followed: the one the stage names, else
/// the newest session that names the stage back.
fn last_session_id(stage: &Stage, sessions: &[Session]) -> Option<String> {
    stage.session.clone().or_else(|| {
        sessions
            .iter()
            .filter(|session| session.stage_id.as_deref() == Some(stage.id.as_str()))
            .max_by_key(|session| session.created_at)
            .map(|session| session.id.clone())
    })
}

#[cfg(test)]
#[path = "criteria_results_tests.rs"]
mod tests;
