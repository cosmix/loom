//! Engine driver: runs the summary engine on every watcher batch and broadcasts the delta.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use anyhow::Result;

use crate::plan_server::concept::page::{page_key_of, summary_file_hash};
use crate::plan_server::context::PlanContext;
use crate::plan_server::lifecycle;
use crate::plan_server::wire::concept::ConceptChanged;
use crate::plan_server::wire::message::PlanMessage;
use crate::planning::project::Project;
use crate::planning::summary::claims::list_claims;
use crate::planning::summary::engine::{EngineUpdate, SummaryEngine};
use crate::planning::summary::evaluate::{evaluate, Evaluation, SummaryState};
use crate::planning::summary::store::{self, STORE_DIR};
use crate::planning::watch::Batch;

/// A callback run with every successful engine update (plan 10 registers its rebuild hooks).
pub type Listener = Box<dyn Fn(&PlanContext, &EngineUpdate) + Send + Sync>;

/// Per summary id: its state and the content hash of its summary file.
type Snapshot = BTreeMap<String, (SummaryState, Option<String>)>;

/// The summary engine and its latest evaluation, shared through `ctx.extensions()`.
pub struct ConceptState {
    engine: Mutex<SummaryEngine>,
    latest: RwLock<Arc<Evaluation>>,
    previous: Mutex<Snapshot>,
    listeners: RwLock<Vec<Arc<Listener>>>,
}

impl ConceptState {
    fn new(engine: SummaryEngine, first: Evaluation, previous: Snapshot) -> Self {
        ConceptState {
            engine: Mutex::new(engine),
            latest: RwLock::new(Arc::new(first)),
            previous: Mutex::new(previous),
            listeners: RwLock::new(Vec::new()),
        }
    }

    /// The evaluation after the last successful batch.
    pub fn latest(&self) -> Arc<Evaluation> {
        let guard = self.latest.read().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(&guard)
    }

    /// Run `f` after every successful engine update, in registration order.
    pub fn subscribe(&self, f: Listener) {
        let mut listeners = self
            .listeners
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        listeners.push(Arc::new(f));
    }

    /// Process one batch (the project is already refreshed with it) and broadcast the delta.
    /// Returns what was broadcast.
    pub(crate) fn handle(&self, ctx: &PlanContext, batch: &Batch) -> Option<ConceptChanged> {
        let result = {
            let project = ctx.project();
            let mut engine = self.engine.lock().unwrap_or_else(PoisonError::into_inner);
            engine.on_batch(&project, batch)
        };
        self.apply(ctx, batch, result)
    }

    /// Store a successful update, notify listeners and broadcast the delta. An error keeps the
    /// previous evaluation and prints one terminal line.
    pub(crate) fn apply(
        &self,
        ctx: &PlanContext,
        batch: &Batch,
        result: Result<EngineUpdate>,
    ) -> Option<ConceptChanged> {
        let update = match result {
            Ok(update) => update,
            Err(error) => {
                lifecycle::line(&format!("summary engine failed: {error:#}"));
                return None;
            }
        };
        *self.latest.write().unwrap_or_else(PoisonError::into_inner) =
            Arc::new(update.evaluation.clone());
        let listeners: Vec<_> = self
            .listeners
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for listener in listeners {
            listener(ctx, &update);
        }
        let changed = self.delta(&ctx.project(), &update.evaluation, batch);
        if let Some(changed) = &changed {
            ctx.hub()
                .broadcast(&PlanMessage::ConceptChanged(changed.clone()));
        }
        changed
    }

    fn delta(&self, project: &Project, eval: &Evaluation, batch: &Batch) -> Option<ConceptChanged> {
        let after = snapshot(project, eval);
        let mut previous = self.previous.lock().unwrap_or_else(PoisonError::into_inner);
        let changed = delta(project, &previous, &after, batch);
        *previous = after;
        let empty = changed.pages.is_empty()
            && changed.out_of_date.is_empty()
            && changed.sources.is_empty();
        (!empty).then_some(changed)
    }
}

fn snapshot(project: &Project, eval: &Evaluation) -> Snapshot {
    eval.summaries
        .iter()
        .map(|(id, summary)| {
            let hash = summary_file_hash(project, id);
            (id.clone(), (summary.state, hash))
        })
        .collect()
}

fn delta(project: &Project, before: &Snapshot, after: &Snapshot, batch: &Batch) -> ConceptChanged {
    let scope = store::scope(project);
    let ids: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    let mut pages = BTreeSet::new();
    let mut out_of_date = BTreeSet::new();
    for id in ids {
        let (old, new) = (before.get(id), after.get(id));
        if old == new {
            continue;
        }
        pages.extend(page_keys(id, &scope));
        let became = new.is_some_and(|(state, _)| is_pending(*state));
        if became && old.map(|(state, _)| *state) != new.map(|(state, _)| *state) {
            out_of_date.insert(id.clone());
        }
    }
    ConceptChanged {
        pages: pages.into_iter().collect(),
        out_of_date: out_of_date.into_iter().collect(),
        sources: sources(batch),
    }
}

fn is_pending(state: SummaryState) -> bool {
    matches!(
        state,
        SummaryState::Stale | SummaryState::Incomplete | SummaryState::Rebuilding
    )
}

/// Every batch path under `doc/` outside the summary store, sorted.
fn sources(batch: &Batch) -> Vec<String> {
    let moved = batch.renamed.iter().flat_map(|(from, to)| [from, to]);
    let paths = batch
        .added
        .iter()
        .chain(&batch.changed)
        .chain(&batch.removed)
        .chain(moved);
    paths
        .filter(|path| path.starts_with("doc") && !path.starts_with(STORE_DIR))
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The page keys whose content depends on summary `id`: its own page and, when it is a member
/// of an overview's scope, that overview.
pub fn page_keys_for_summary(id: &str, project: &Project) -> Vec<String> {
    page_keys(id, &store::scope(project))
}

fn page_keys(id: &str, scope: &store::Scope) -> Vec<String> {
    let (page, overview, members) = if id == "product" || id == "engineering" {
        return vec![id.to_owned()];
    } else if id.starts_with("product/") {
        (id.to_owned(), "product", &scope.product)
    } else if id.starts_with("plans/") {
        (page_key_of(id), "engineering", &scope.engineering)
    } else {
        return Vec::new();
    };
    let mut keys = vec![page];
    if members.iter().any(|member| member == id) {
        keys.push(overview.to_owned());
    }
    keys
}

/// Build the engine, seed the latest evaluation, and follow every watcher batch.
pub fn install(ctx: &Arc<PlanContext>) {
    let engine = SummaryEngine::new(
        ctx.root().to_path_buf(),
        ctx.runtime().clone(),
        ctx.own_writes().clone(),
    );
    let claims = list_claims(ctx.runtime()).unwrap_or_else(|error| {
        lifecycle::line(&format!("summary claims could not be read: {error:#}"));
        Vec::new()
    });
    let state = {
        let project = ctx.project();
        let first = evaluate(&project, &claims);
        let previous = snapshot(&project, &first);
        Arc::new(ConceptState::new(engine, first, previous))
    };
    ctx.extensions().insert(Arc::clone(&state));
    ctx.on_batch(Box::new(move |ctx, batch| {
        state.handle(ctx, batch);
    }));
}
