//! What the facade reads from an evaluation: the out-of-date set, seals, claims.

use std::collections::BTreeMap;
use std::time::Instant;

use super::{Rebuilds, State};
use crate::plan_server::lifecycle::{line, safe};
use crate::plan_server::summaries::scheduler::Scheduler;
use crate::planning::summary::claims::list_claims;
use crate::planning::summary::evaluate::{evaluate, Evaluation, SummaryEval, SummaryState};
use crate::planning::summary::hashes::input_hashes;

impl Rebuilds {
    /// When `needed` holds for the state, the ids to rebuild ([`to_rebuild`]) in the store as
    /// it is now.
    ///
    /// A chat the user asks to rebuild a summary seals it inside the turn, and the watcher's
    /// batch showing the seal can arrive up to a second after the turn ends: resuming on the
    /// cached evaluation would queue what the turn just sealed, and a summary with no change on
    /// record (out of date before the turn) would start at once. Only the out-of-date set
    /// is taken: storing this evaluation could replace a newer batch's that was applied while
    /// this one was made. It is made outside the state lock, because concept mode takes the state
    /// lock while it holds the project's. `None` without a server context or readable claims:
    /// the caller keeps the cached evaluation.
    pub(super) fn to_rebuild_now(&self, needed: impl Fn(&State) -> bool) -> Option<Vec<String>> {
        let needed = needed(&self.lock());
        if !needed {
            return None;
        }
        let env = self.env.as_ref()?;
        let ctx = env.ctx.upgrade()?;
        let claims = match list_claims(&env.runtime) {
            Ok(claims) => claims,
            Err(error) => {
                let why = safe(&format!("{error:#}"));
                line(&format!("summaries: cannot list the claims: {why}"));
                return None;
            }
        };
        let evaluation = evaluate(&ctx.project(), &claims);
        Some(to_rebuild(&evaluation))
    }

    /// What each summary of `evaluation` is built from, read from the server's project; `None`
    /// without a server. Never called under the state lock (it takes the project's).
    pub(super) fn source_inputs(
        &self,
        evaluation: &Evaluation,
    ) -> Option<BTreeMap<String, String>> {
        let ctx = self.env.as_ref()?.ctx.upgrade()?;
        let project = ctx.project();
        Some(input_hashes(&project, evaluation))
    }
}

impl State {
    /// Record `inputs` and start the quiet period of every summary whose inputs changed since
    /// the last record.
    pub(super) fn note_inputs(&mut self, inputs: BTreeMap<String, String>, now: Instant) {
        let changed = changed_inputs(self.inputs.as_ref(), &inputs);
        self.scheduler.sources_changed(&changed, now);
        self.inputs = Some(inputs);
    }
}

/// The ids whose inputs in `now` differ from `before`, or that `before` lacks (a new document).
/// None without an earlier record: what the first evaluation finds out of date waits for nothing.
fn changed_inputs(
    before: Option<&BTreeMap<String, String>>,
    now: &BTreeMap<String, String>,
) -> Vec<String> {
    let Some(before) = before else {
        return Vec::new();
    };
    let changed = now
        .iter()
        .filter(|(id, hash)| before.get(*id) != Some(*hash));
    changed.map(|(id, _)| id.clone()).collect()
}

/// Every id the evaluation finds stale or missing: what the scheduler keeps queued and rebuilds
/// without a press (`SummaryState::rebuilds_in_background`). An incomplete summary is out of
/// date too, but waits for a press; the panel lists it apart.
pub(super) fn to_rebuild(evaluation: &Evaluation) -> Vec<String> {
    let summaries = evaluation.summaries.values();
    let due = summaries.filter(|eval| eval.state.rebuilds_in_background());
    due.map(|eval| eval.id.clone()).collect()
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
mod tests_quiet;
#[cfg(test)]
mod tests_terminal;
