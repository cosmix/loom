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
use std::sync::{Arc, Mutex, PoisonError};

use super::context::PlanContext;
use super::request::{ApiRequest, ApiResponse};
use super::routes::{BodyKind, Method, Role, RouteSpec};
use super::wire::files::{DocChanged, FileTree};
use super::wire::message::PlanMessage;
use crate::planning::discover::SUMMARIES;
use crate::planning::watch::Batch;
use texts::TextStore;
use validate::{revalidate, validate_all, ValidationCache};

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

/// Build the text store and the validation cache into `ctx.extensions()`, and register the
/// batch hook that keeps both current and tells the browsers what changed. Runs before
/// `search::install`, which reads the text store.
pub fn install(ctx: &Arc<PlanContext>) {
    let loaded = TextStore::load(ctx.root(), &ctx.project());
    let texts = Arc::new(loaded.unwrap_or_else(|error| {
        tracing::warn!("file mode could not load the document texts: {error:#}");
        TextStore::default()
    }));
    ctx.extensions().insert(Arc::clone(&texts));
    let cache: Arc<ValidationCache> = Arc::new(Mutex::new(validate_all(ctx, &texts)));
    ctx.extensions().insert(Arc::clone(&cache));
    let last_tree = Mutex::new(current_tree(ctx, &cache));
    ctx.on_batch(Box::new(move |ctx, batch| {
        on_batch(ctx, batch, &texts, &cache, &last_tree);
    }));
}

fn tree_route(req: &ApiRequest<'_>) -> ApiResponse {
    match req.ctx.extensions().get::<ValidationCache>() {
        Some(cache) => ApiResponse::json(200, &current_tree(req.ctx, &cache)),
        None => ApiResponse::error(500, "internal", "file mode is not installed"),
    }
}

fn current_tree(ctx: &PlanContext, cache: &ValidationCache) -> FileTree {
    let validation = cache.lock().unwrap_or_else(PoisonError::into_inner).clone();
    tree::build_tree(&ctx.project(), &validation)
}

/// Follow one watcher batch (the project is already refreshed): texts, then one `doc_changed`
/// per changed document, then revalidation, then `tree_changed` when the tree differs from
/// the one last sent.
fn on_batch(
    ctx: &PlanContext,
    batch: &Batch,
    texts: &TextStore,
    cache: &ValidationCache,
    last_tree: &Mutex<FileTree>,
) {
    texts.apply(ctx.root(), batch);
    for rel in changed_documents(batch) {
        let hash = texts.current(&rel).map(|pinned| pinned.hash);
        let path = rel.to_string_lossy().into_owned();
        ctx.hub()
            .broadcast(&PlanMessage::DocChanged(DocChanged { path, hash }));
    }
    revalidate(ctx, cache, texts);
    let tree = current_tree(ctx, cache);
    let changed = {
        let mut last = last_tree.lock().unwrap_or_else(PoisonError::into_inner);
        let changed = *last != tree;
        if changed {
            last.clone_from(&tree);
        }
        changed
    };
    if changed {
        ctx.hub().broadcast(&PlanMessage::TreeChanged(tree));
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
