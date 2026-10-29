//! Engine driver: runs the summary engine at install and on every watcher batch, and broadcasts
//! the delta.

use std::sync::{Arc, Mutex, PoisonError, RwLock};

use anyhow::Result;

use crate::plan_server::concept::delta::{delta, snapshot, source_deltas, Snapshot};
use crate::plan_server::concept::routes::page_context;
use crate::plan_server::context::PlanContext;
use crate::plan_server::lifecycle;
use crate::plan_server::wire::concept::ConceptChanged;
use crate::plan_server::wire::message::PlanMessage;
use crate::planning::project::Project;
use crate::planning::runtime::RuntimePaths;
use crate::planning::summary::claims::list_claims;
use crate::planning::summary::engine::{EngineUpdate, SummaryEngine};
use crate::planning::summary::evaluate::{evaluate, Evaluation};
use crate::planning::watch::Batch;

/// A callback run with every successful engine update (plan 10 registers its rebuild hooks).
pub type Listener = Box<dyn Fn(&PlanContext, &EngineUpdate) + Send + Sync>;

/// The summary engine and its latest evaluation, shared through `ctx.extensions()`.
pub struct ConceptState {
    engine: Mutex<SummaryEngine>,
    /// The runtime directory the engine reads session claims from.
    runtime: RuntimePaths,
    latest: RwLock<Arc<Evaluation>>,
    previous: Mutex<Snapshot>,
    listeners: RwLock<Vec<Arc<Listener>>>,
}

impl ConceptState {
    /// The evaluation after the last batch.
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
    pub(crate) fn handle(&self, ctx: &PlanContext, batch: &Batch) -> Vec<ConceptChanged> {
        let result = {
            let project = ctx.project();
            let mut engine = self.engine.lock().unwrap_or_else(PoisonError::into_inner);
            engine.on_batch(&project, batch)
        };
        self.apply(ctx, batch, result)
    }

    /// Store a successful update, notify listeners and broadcast the delta. A failure prints one
    /// terminal line and still broadcasts the batch's sources so open passages refetch. When the
    /// session claims are unreadable (the engine's only failing step) the project is evaluated
    /// without them, as `install` does; otherwise the previous evaluation stays.
    pub(crate) fn apply(
        &self,
        ctx: &PlanContext,
        batch: &Batch,
        result: Result<EngineUpdate>,
    ) -> Vec<ConceptChanged> {
        let error = match result {
            Ok(update) => {
                let evaluation = self.store(update.evaluation.clone());
                self.notify(ctx, &update);
                return self.publish(ctx, batch, Some(&evaluation));
            }
            Err(error) => error,
        };
        if list_claims(&self.runtime).is_ok() {
            lifecycle::line(&format!("summary engine failed: {}", safe_error(&error)));
            return self.publish(ctx, batch, None);
        }
        let evaluation = self.store(without_claims(&ctx.project(), &error));
        self.publish(ctx, batch, Some(&evaluation))
    }

    fn store(&self, evaluation: Evaluation) -> Arc<Evaluation> {
        let evaluation = Arc::new(evaluation);
        *self.latest.write().unwrap_or_else(PoisonError::into_inner) = Arc::clone(&evaluation);
        evaluation
    }

    fn notify(&self, ctx: &PlanContext, update: &EngineUpdate) {
        let listeners: Vec<_> = self
            .listeners
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for listener in listeners {
            listener(ctx, update);
        }
    }

    /// Broadcast what changed since the previous evaluation. With no evaluation the pages are
    /// unknown and the delta carries the batch's sources alone.
    fn publish(
        &self,
        ctx: &PlanContext,
        batch: &Batch,
        evaluation: Option<&Evaluation>,
    ) -> Vec<ConceptChanged> {
        let changed = match evaluation {
            Some(eval) => {
                let project = ctx.project();
                let after = snapshot(&project, eval, &page_context(ctx));
                let mut previous = self.previous.lock().unwrap_or_else(PoisonError::into_inner);
                let changed = delta(&project, &previous, &after, batch);
                *previous = after;
                changed
            }
            None => source_deltas(batch),
        };
        for payload in &changed {
            ctx.hub()
                .broadcast(&PlanMessage::ConceptChanged(payload.clone()));
        }
        changed
    }
}

/// `error` as one terminal-safe line.
fn safe_error(error: &anyhow::Error) -> String {
    lifecycle::safe(&format!("{error:#}"))
}

/// Evaluate `project` with no session claims after the engine failed to read them.
fn without_claims(project: &Project, error: &anyhow::Error) -> Evaluation {
    lifecycle::line(&format!(
        "summary claims could not be read, evaluating without them: {}",
        safe_error(error)
    ));
    evaluate(project, &[])
}

/// Build the engine, run it once, and follow every watcher batch.
pub fn install(ctx: &Arc<PlanContext>) {
    install_with(ctx, ctx.runtime().clone());
}

/// [`install`] with the runtime directory the engine locks and reads claims from.
pub(crate) fn install_with(ctx: &Arc<PlanContext>, runtime: RuntimePaths) {
    let own = ctx.own_writes().clone();
    let mut engine = SummaryEngine::new(ctx.root().to_path_buf(), runtime.clone(), own);
    // The watcher drops its initial batch because the project loads after that walk. This run
    // is the engine's share of it (section 04 "The watcher", Start): renames and deletions made
    // while no server ran are applied before the first request.
    let initial = Batch {
        initial: true,
        ..Batch::default()
    };
    let state = {
        let project = ctx.project();
        let first = match engine.on_batch(&project, &initial) {
            Ok(update) => update.evaluation,
            Err(error) => without_claims(&project, &error),
        };
        let previous = snapshot(&project, &first, &page_context(ctx));
        Arc::new(ConceptState {
            engine: Mutex::new(engine),
            runtime,
            latest: RwLock::new(Arc::new(first)),
            previous: Mutex::new(previous),
            listeners: RwLock::new(Vec::new()),
        })
    };
    ctx.extensions().insert(Arc::clone(&state));
    ctx.on_batch(Box::new(move |ctx, batch| {
        state.handle(ctx, batch);
    }));
}
