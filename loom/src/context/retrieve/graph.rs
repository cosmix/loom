//! Resolving the source graph for a query's overlay, and detecting the A.11
//! degraded mode: a non-empty semantic revision whose base layer went
//! missing.
//!
//! Split out of `retrieve.rs` so the top-level pipeline in
//! [`super::retrieve_for_stage`] stays a readable sequence of steps rather
//! than growing this reasoning inline. See
//! `doc/PROPOSAL-retrieval-precision.md` §A.11 for the design this
//! implements.

use crate::context::graph_store::{GraphStore, ResolvedGraph};
use crate::context::local_overlay::OverlayScope;
use crate::context::refresh::short_revision;
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;
use std::path::Path;

/// Load the resolved source graph for `query`'s overlay, degrading to `None`
/// on any error, alongside an A.11 degradation message when the base layer
/// for a non-empty semantic revision could not be found.
///
/// `overlay.resolve` always yields a `(plan, stage)` pair, so this always asks
/// [`GraphStore::resolved`] for the overlay-applied view — never `None` for
/// the stage. That distinction matters on its own: with `None`, `resolved`
/// reads only the base layer, and a base miss there becomes an *empty* graph
/// rather than a missing one, silently dropping an overlay the query should
/// have read. [`OverlayScope::Local`] resolves to the `(plan, stage)` address
/// `local_overlay_key` computes and `loom map` writes (`commands/map.rs`) —
/// the only production writer of that overlay today — so a `Local`-scoped
/// query is what lets a caller see a working tree that no merge has
/// published a base for.
///
/// Retrieval itself never builds or refreshes this graph: `resolve_catalog`
/// calls `refresh` with `structural_only = true`, which skips the semantic
/// reconcile on every call this pipeline makes. So this function only ever
/// reads what a prior `loom map` run, or a merge, already wrote for
/// `semantic_revision`.
///
/// **Real, currently-reachable degraded mode:** a non-empty
/// `semantic_revision` names a layer `state.json` claims was built, but
/// `graph_store::GraphStore::resolved`'s base half is `unwrap_or_default()`
/// over `load_base`, so a missing base file resolves to an *empty* base
/// rather than an error — `resolved.base_revision` comes back empty exactly
/// when that happened. [`degraded_reason`] turns that into the message
/// `super::build_pack_request` carries out to `ContextPack::degraded`, so a
/// checkout whose entire source channel is served from one overlay against a
/// silently empty base says so instead of reading as `Semantic: current`.
pub(super) fn load_resolved_graph(
    work_dir_hint: &Path,
    store: &ContextStore,
    semantic_revision: &str,
    overlay: &OverlayScope,
) -> (Option<ResolvedGraph>, Option<String>) {
    let Some(work_dir) = WorkDir::new(work_dir_hint).ok() else {
        return (None, None);
    };
    let Some(project_root) = work_dir.project_root() else {
        return (None, None);
    };
    let (plan, stage) = overlay.resolve(project_root);
    let graph_store = GraphStore::new(store.root(), work_dir.root());
    let Ok(graph) = graph_store.resolved(semantic_revision, Some((&plan, &stage))) else {
        return (None, None);
    };

    let degraded = degraded_reason(semantic_revision, &graph);
    (Some(graph), degraded)
}

/// The A.11 degradation message, or `None` when this read is honestly
/// healthy.
///
/// An EMPTY `semantic_revision` means "never built" — `evaluate_state`
/// already reports that honestly as a `never_built` [`crate::context::schema::Freshness`]
/// elsewhere in the pack, so flagging it here too would print a
/// `DEGRADED:` banner on every unmapped checkout forever, for a condition
/// that is not a degradation at all; skip it. Otherwise, `graph.base_revision`
/// empty means [`GraphStore::load_base`] missed for a revision the store
/// claims exists — the real degraded mode this function exists to surface.
fn degraded_reason(semantic_revision: &str, graph: &ResolvedGraph) -> Option<String> {
    if semantic_revision.is_empty() || !graph.base_revision.is_empty() {
        return None;
    }
    Some(format!(
        "source graph base {} missing — serving overlay only",
        short_revision(semantic_revision)
    ))
}
