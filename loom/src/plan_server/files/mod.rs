//! File mode: the document tree, document and diff routes, saves and validation.

pub mod diff;
pub mod doc;
pub mod knowledge_index;
pub mod plan_view;
pub mod save;
pub mod texts;
pub mod tree;
pub mod validate;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use super::context::PlanContext;
use super::request::{ApiRequest, ApiResponse};
use super::routes::{BodyKind, Method, Role, RouteSpec};
use super::wire::files::{DocChanged, FileTree};
use super::wire::message::PlanMessage;
use crate::planning::discover::SUMMARIES;
use crate::planning::watch::Batch;
use texts::TextStore;
use validate::ValidationCache;

/// The file-mode routes: tree, document, save and the two diffs.
pub fn routes() -> Vec<RouteSpec> {
    let viewer = |path, handler: fn(&ApiRequest<'_>) -> ApiResponse| RouteSpec {
        method: Method::Get,
        path,
        body: BodyKind::None,
        role: Role::Viewer,
        handler,
    };
    vec![
        viewer("/api/tree", tree_route),
        viewer("/api/doc", doc::doc_route),
        RouteSpec {
            method: Method::Post,
            path: "/api/save",
            body: BodyKind::Text {
                max: doc::MAX_EDIT_BYTES,
            },
            role: Role::HolderOrTake,
            handler: save::save_route,
        },
        viewer("/api/diff/head", diff::head_diff),
        viewer("/api/diff/turn", diff::turn_diff),
    ]
}

/// Build the text store and an empty validation cache into `ctx.extensions()`, start the
/// validation pass, and register the batch hook that keeps all of it current and tells the
/// browsers what changed. Runs before `search::install`, which reads the text store.
pub fn install(ctx: &Arc<PlanContext>) {
    let loaded = TextStore::load(ctx.root(), &ctx.project());
    let texts = Arc::new(loaded.unwrap_or_else(|error| {
        tracing::warn!("file mode could not load the document texts: {error:#}");
        TextStore::default()
    }));
    ctx.extensions().insert(Arc::clone(&texts));
    let cache = Arc::new(ValidationCache::default());
    ctx.extensions().insert(Arc::clone(&cache));
    let state = Arc::new(FileMode::new(texts, cache));
    spawn_validation(ctx, &state);
    ctx.on_batch(Box::new(move |ctx, batch| state.follow(ctx, batch)));
}

/// Validate every plan on a thread of its own, so the server answers while the checks run:
/// until a plan is validated the tree shows it without an error count and `/api/doc`
/// validates it on request. Each plan's result is broadcast as `validation`, then the tree as
/// `tree_changed`. When the thread cannot start, the first batch validates every plan.
fn spawn_validation(ctx: &Arc<PlanContext>, state: &Arc<FileMode>) {
    let (worker_ctx, state) = (Arc::clone(ctx), Arc::clone(state));
    let spawned = thread::Builder::new()
        .name("loom-plan-validate".into())
        .spawn(move || state.validate_all(&worker_ctx));
    match spawned {
        Ok(worker) => ctx.own_worker("startup validation", worker, None),
        Err(error) => {
            tracing::warn!("loom plan validates its plans at the first doc/ change: {error}")
        }
    }
}

fn tree_route(req: &ApiRequest<'_>) -> ApiResponse {
    let texts = match doc::text_store(req.ctx) {
        Ok(texts) => texts,
        Err(refusal) => return refusal,
    };
    match req.ctx.extensions().get::<ValidationCache>() {
        Some(cache) => ApiResponse::json(200, &current_tree(req.ctx, &texts, &cache)),
        None => ApiResponse::error(500, "internal", "file mode is not installed"),
    }
}

/// The tree from the project view, the stored texts and the cached error counts; no file is
/// read while the project is locked.
fn current_tree(ctx: &PlanContext, texts: &TextStore, cache: &ValidationCache) -> FileTree {
    let errors = cache.error_counts();
    tree::build_tree(&ctx.project(), texts, &errors)
}

/// What file mode keeps current: the texts, the validation cache, and the tree last broadcast
/// (`None` until the first). The tree's lock also orders validation: the startup pass and
/// every batch revalidate and publish under it, one at a time.
struct FileMode {
    texts: Arc<TextStore>,
    cache: Arc<ValidationCache>,
    published: Mutex<Option<FileTree>>,
}

impl FileMode {
    fn new(texts: Arc<TextStore>, cache: Arc<ValidationCache>) -> FileMode {
        FileMode {
            texts,
            cache,
            published: Mutex::new(None),
        }
    }

    /// The startup pass: validate every plan, then publish the tree with its error counts.
    fn validate_all(&self, ctx: &PlanContext) {
        let mut published = self.publishing();
        // What `/api/tree` answered so far: the tree before any plan was validated.
        published.get_or_insert_with(|| current_tree(ctx, &self.texts, &self.cache));
        self.revalidate(ctx, &mut published, &BTreeSet::new());
    }

    /// Follow one watcher batch (the project is already refreshed): texts, then one
    /// `doc_changed` per changed document, then revalidation of the plans the batch can
    /// affect, then `tree_changed` when the tree differs from the one last sent.
    fn follow(&self, ctx: &PlanContext, batch: &Batch) {
        self.texts.apply(ctx.root(), batch);
        let touched = changed_documents(batch);
        for rel in &touched {
            let hash = self.texts.current(rel).map(|pinned| pinned.hash);
            let path = rel.to_string_lossy().into_owned();
            ctx.hub()
                .broadcast(&PlanMessage::DocChanged(DocChanged { path, hash }));
        }
        let mut published = self.publishing();
        self.revalidate(ctx, &mut published, &touched);
    }

    /// Revalidate the plans `touched` can affect and every plan the cache lacks, then
    /// broadcast the tree when it differs from `published`. Nothing is published once the
    /// server is stopping: the pass may have been cut short.
    fn revalidate(
        &self,
        ctx: &PlanContext,
        published: &mut Option<FileTree>,
        touched: &BTreeSet<PathBuf>,
    ) {
        self.cache.revalidate(ctx, &self.texts, touched);
        if ctx.is_stopping() {
            return;
        }
        let tree = current_tree(ctx, &self.texts, &self.cache);
        if published.as_ref() != Some(&tree) {
            ctx.hub().broadcast(&PlanMessage::TreeChanged(tree.clone()));
            *published = Some(tree);
        }
    }

    fn publishing(&self) -> MutexGuard<'_, Option<FileTree>> {
        self.published
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Every path the batch names, renames on both sides, sorted; the summary store is never a
/// document.
fn changed_documents(batch: &Batch) -> BTreeSet<PathBuf> {
    let renamed = batch.renamed.iter().flat_map(|(from, to)| [from, to]);
    let named = batch
        .added
        .iter()
        .chain(&batch.changed)
        .chain(&batch.removed);
    named
        .chain(renamed)
        .filter(|rel| !rel.starts_with(Path::new(SUMMARIES)))
        .cloned()
        .collect()
}
