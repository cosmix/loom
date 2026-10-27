//! A stage's history for the status payload: what `Stage::issue_history`
//! archived plus every adjudication round that ended, newest first.

use crate::context::untrusted::whole_safe;
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{IssueKind, Stage, StageIssue};

use super::rounds::{adjudication_rounds, Round};

/// Every issue `stage` moved past, newest first by `ended_at`. The sort is
/// stable, so entries with one `ended_at` keep the order they were recorded in.
///
/// Nothing is cut: a stage's history grows with every issue and is shipped
/// whole in each status frame. Revisit with paging if a stage's history makes
/// the frame too large for the dashboard's refresh.
pub(super) fn stage_history(stage: &Stage, work_dir: &WorkDir) -> Vec<StageIssue> {
    let rounds = adjudication_rounds(stage, work_dir);
    let mut history: Vec<StageIssue> = stage
        .issue_history
        .iter()
        .filter(|issue| !restates_a_round(issue, &rounds))
        .cloned()
        .collect();
    history.extend(rounds.into_iter().map(|round| round.issue));
    history.iter_mut().for_each(neutralize);
    history.sort_by_key(|issue| std::cmp::Reverse(issue.ended_at));
    history
}

/// Filing a dispute records its reason as the stage's review reason, so the
/// archived reason of a dispute whose round ended repeats that round: the
/// round, which also carries the ruling, stands for both. A review reason
/// no ended round was filed with (an escalation, a rejection, an open
/// dispute) stays.
fn restates_a_round(issue: &StageIssue, rounds: &[Round]) -> bool {
    issue.kind == IssueKind::Review
        && rounds
            .iter()
            .any(|round| !round.reason_lines.is_empty() && round.reason_lines == issue.evidence)
}

/// Evidence is a crashed session's stderr, an agent's dispute or an
/// adjudicator's reasoning, and `ended_by` can carry a verdict's own word, so
/// each goes through [`whole_safe`]: neutralized, never cut.
fn neutralize(issue: &mut StageIssue) {
    for line in &mut issue.evidence {
        *line = whole_safe(line);
    }
    issue.ended_by = whole_safe(&issue.ended_by);
}
