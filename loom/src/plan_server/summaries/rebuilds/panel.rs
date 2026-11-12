//! The summary panel, the question's estimate and concept mode's never-built estimate.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use chrono::Local;

use super::{lock, Rebuilds, State};
use crate::plan_server::summaries::attribution::level_of;
use crate::plan_server::summaries::scheduler::{QuestionView, SchedulerView};
use crate::plan_server::wire::summaries::{
    FailedSummary, RunningRebuild, SummaryPanel, SummaryQuestion,
};
use crate::planning::summary::cost::estimate::{estimate, totals};
use crate::planning::summary::cost::{CostLine, Level};
use crate::planning::summary::evaluate::{Evaluation, SummaryState};
use crate::planning::summary::store::is_overview;

#[cfg(test)]
mod tests;

/// `(id, level, source bytes)` of summaries waiting for an estimate.
pub(super) type Pending = Vec<(String, Level, u64)>;

impl Rebuilds {
    /// The summary panel: the scheduler's state, the cost log's last line and totals, quota.
    pub fn panel(&self) -> SummaryPanel {
        let (view, started_at, pending, waiting, incomplete) = {
            let state = self.lock();
            let view = state.scheduler.panel_state();
            let pending = view.question.as_ref().map(|open| state.pending(&open.ids));
            let waiting = state.scheduler.waiting(Instant::now());
            let incomplete = state.incomplete(&view);
            (view, state.started_at.clone(), pending, waiting, incomplete)
        };
        let (last, totals, question) = {
            let log = lock(&self.log);
            let asked = view.question.as_ref().zip(pending);
            let question = asked.map(|(open, pending)| question_message(open, &log, &pending));
            (log.last().cloned(), totals(&log, Local::now()), question)
        };
        let runtime = self.env.as_ref().map(|env| env.runtime.dir.as_path());
        let quota = runtime.map(crate::quota::read_snapshot).unwrap_or_default();
        let running = view.running.map(|(session, ids)| RunningRebuild {
            log_path: self
                .spawner
                .log_path(&session)
                .map(|path| path.display().to_string()),
            session,
            done: view.sealed.len(),
            total: ids.len(),
            ids,
            started_at: started_at.unwrap_or_default(),
        });
        let failed = view.failed.into_iter();
        SummaryPanel {
            running,
            queue: view.queue,
            waiting,
            incomplete,
            failed: failed
                .map(|(id, error)| FailedSummary { id, error })
                .collect(),
            question,
            last,
            today: totals.today,
            week: totals.week,
            quota,
            read_only: view.read_only,
            terminal_claims: view.terminal_claims,
            start_error: view.start_error,
        }
    }

    /// `about <n> tokens to build, an estimate` while summaries are missing (concept mode's
    /// never-built state); `None` when none is.
    pub fn estimate_text(&self) -> Option<String> {
        let pending = {
            let state = self.lock();
            let latest = state.latest.clone()?;
            let missing = latest.summaries.values();
            let missing = missing.filter(|eval| eval.state == SummaryState::Missing);
            let missing: Vec<String> = missing.map(|eval| eval.id.clone()).collect();
            if missing.is_empty() {
                return None;
            }
            state.pending(&missing)
        };
        let tokens = estimate(&lock(&self.log), &pending).tokens;
        Some(format!("about {tokens} tokens to build, an estimate"))
    }

    /// The question as browsers get it, with its estimate.
    pub(super) fn summary_question(
        &self,
        open: &QuestionView,
        pending: &Pending,
    ) -> SummaryQuestion {
        question_message(open, &lock(&self.log), pending)
    }

    /// Source bytes of every summary from the project's tree; an overview sums its members'.
    /// `None` without a server. Never called under the state lock (it takes the project's).
    pub(super) fn source_sizes(&self, evaluation: &Evaluation) -> Option<BTreeMap<String, u64>> {
        let ctx = self.env.as_ref()?.ctx.upgrade()?;
        let project = ctx.project();
        let size = |path: &Path| project.tree.get(path).map_or(0, |doc| doc.size);
        let documents = evaluation
            .summaries
            .values()
            .filter(|eval| !is_overview(&eval.id));
        let sized = documents.map(|eval| (eval.id.clone(), eval.source.as_deref().map_or(0, size)));
        let mut sizes: BTreeMap<String, u64> = sized.collect();
        for scope in evaluation.scopes.values() {
            let id = format!("overview/{}/product", scope.feature);
            if evaluation.summaries.contains_key(&id) {
                let total = scope.product.iter().filter_map(|id| sizes.get(id)).sum();
                sizes.insert(id, total);
            }
        }
        Some(sizes)
    }
}

impl State {
    /// The incomplete summaries of the latest evaluation that nothing queues, runs or failed:
    /// out of date, but rebuilt on a press only (`SummaryState::rebuilds_in_background`).
    fn incomplete(&self, view: &SchedulerView) -> Vec<String> {
        let Some(latest) = &self.latest else {
            return Vec::new();
        };
        let running = view.running.as_ref().map(|(_, ids)| ids.as_slice());
        let listed = |id: &String| {
            view.queue.contains(id)
                || running.unwrap_or_default().contains(id)
                || view.failed.iter().any(|(failed, _)| failed == id)
        };
        let summaries = latest.summaries.values();
        let incomplete = summaries.filter(|eval| eval.state == SummaryState::Incomplete);
        incomplete
            .map(|eval| eval.id.clone())
            .filter(|id| !listed(id))
            .collect()
    }

    /// `ids` with their level and source bytes, for an estimate.
    pub(super) fn pending(&self, ids: &[String]) -> Pending {
        let bytes = |id: &String| self.sizes.get(id).copied().unwrap_or(0);
        ids.iter()
            .map(|id| (id.clone(), level_of(id), bytes(id)))
            .collect()
    }
}

fn question_message(open: &QuestionView, log: &[CostLine], pending: &Pending) -> SummaryQuestion {
    let estimate = estimate(log, pending);
    SummaryQuestion {
        id: open.id.clone(),
        count: open.ids.len(),
        ids: open.ids.clone(),
        estimate_tokens: estimate.tokens,
        basis: estimate.basis,
    }
}
