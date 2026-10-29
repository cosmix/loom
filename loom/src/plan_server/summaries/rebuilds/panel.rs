//! The summary panel, the question's estimate and concept mode's never-built estimate.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::Local;

use super::{lock, Rebuilds, State};
use crate::plan_server::summaries::attribution::level_of;
use crate::plan_server::summaries::scheduler::QuestionView;
use crate::plan_server::wire::summaries::{
    FailedSummary, RunningRebuild, SummaryPanel, SummaryQuestion,
};
use crate::planning::summary::cost::estimate::{estimate, totals};
use crate::planning::summary::cost::{CostLine, Level};
use crate::planning::summary::evaluate::{Evaluation, SummaryState};
use crate::planning::summary::store::is_overview;

/// `(id, level, source bytes)` of summaries waiting for an estimate.
pub(super) type Pending = Vec<(String, Level, u64)>;

impl Rebuilds {
    /// The summary panel: the scheduler's state, the cost log's last line and totals, quota.
    pub fn panel(&self) -> SummaryPanel {
        let (view, started_at, pending) = {
            let state = self.lock();
            let view = state.scheduler.panel_state();
            let pending = view.question.as_ref().map(|open| state.pending(&open.ids));
            (view, state.started_at.clone(), pending)
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
        let scope = &evaluation.scope;
        for (overview, members) in [
            ("product", &scope.product),
            ("engineering", &scope.engineering),
        ] {
            let total = members.iter().filter_map(|id| sizes.get(id)).sum();
            sizes.insert(overview.to_owned(), total);
        }
        Some(sizes)
    }
}

impl State {
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
