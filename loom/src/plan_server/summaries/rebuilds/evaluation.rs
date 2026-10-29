//! What the facade reads from an evaluation: the out-of-date set, seals, claims.

use std::time::Instant;

use super::State;
use crate::plan_server::summaries::scheduler::Scheduler;
use crate::planning::summary::evaluate::{Evaluation, SummaryEval, SummaryState};

/// Every id the evaluation finds stale, incomplete or missing.
pub(super) fn out_of_date(evaluation: &Evaluation) -> Vec<String> {
    let summaries = evaluation.summaries.values();
    let stale = summaries.filter(|eval| eval.state.is_out_of_date());
    stale.map(|eval| eval.id.clone()).collect()
}

fn claimant(eval: &SummaryEval) -> Option<&str> {
    eval.claim.as_ref()?.session.as_deref()
}

/// The ids `session` holds claims on.
pub(super) fn held_by(evaluation: &Evaluation, session: &str) -> Vec<String> {
    let summaries = evaluation.summaries.values();
    let held = summaries.filter(|eval| claimant(eval) == Some(session));
    held.map(|eval| eval.id.clone()).collect()
}

/// The ids claimed by a session outside `alive`: the start-up sweep deletes those claims,
/// though the evaluation, made before it, still shows the summaries rebuilding.
pub(super) fn swept(evaluation: &Evaluation, alive: &[String]) -> Vec<String> {
    let dead = |eval: &&SummaryEval| claimant(eval).is_some_and(|s| !alive.iter().any(|a| a == s));
    let summaries = evaluation.summaries.values();
    summaries.filter(dead).map(|eval| eval.id.clone()).collect()
}

/// Bring the scheduler's terminal-claim list in line with the evaluation: every summary it shows
/// claimed without a session is listed (a claim made from a terminal while the server runs
/// included), a listed id it shows with a session claim or none is dropped, and a listed id it
/// does not evaluate (its document was removed, the claim file remains) stays until released.
pub(super) fn keep_terminal_claims(scheduler: &mut Scheduler, evaluation: &Evaluation) {
    let listed = scheduler.panel_state().terminal_claims;
    let is_terminal = |eval: &SummaryEval| {
        eval.claim
            .as_ref()
            .is_some_and(|claim| claim.session.is_none())
    };
    let mut terminal = listed.clone();
    terminal.retain(|id| evaluation.summaries.get(id).is_none_or(is_terminal));
    let claimed = evaluation
        .summaries
        .values()
        .filter(|eval| is_terminal(eval));
    for eval in claimed {
        if !terminal.contains(&eval.id) {
            terminal.push(eval.id.clone());
        }
    }
    if terminal != listed {
        scheduler.set_terminal_claims(terminal);
    }
}

/// A seal is seen when a running id's summary file differs from its baseline and does not
/// evaluate missing; the new file becomes the baseline. An id without a baseline gets one.
pub(super) fn detect_seals(state: &mut State, evaluation: &Evaluation, now: Instant) {
    let Some((_, ids)) = state.scheduler.panel_state().running else {
        return;
    };
    for id in ids {
        let Some(eval) = evaluation.summaries.get(&id) else {
            continue;
        };
        let sealed = match state.baselines.get(&id) {
            Some(base) => *base != eval.file && eval.state != SummaryState::Missing,
            None => false,
        };
        if sealed {
            state.scheduler.seal_seen(&id, now);
        }
        if sealed || !state.baselines.contains_key(&id) {
            state.baselines.insert(id, eval.file.clone());
        }
    }
}

#[cfg(test)]
mod tests_terminal;
