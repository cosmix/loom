//! The two halves of one retrieval: [`capture`] reads the catalog, both
//! freshness layers and the resolved graph once; [`retrieve_with`] ranks and
//! packs one query against that capture.
//!
//! [`super::retrieve_for_stage`] runs them back to back. A caller that scores
//! many queries over one corpus (`loom knowledge eval`) captures once, so every
//! query reads the same revision and pays the catalog refresh and the graph
//! load once instead of per query.

use super::graph::{self, GraphLoad};
use super::{channels, request, resolve_catalog, resolve_roots_optional, windows};
use super::{ResolvedRoots, StageQuery};
use crate::context::config::RetrievalConfig;
use crate::context::fuse::fuse;
use crate::context::pack::pack;
use crate::context::schema::ContextPack;
use crate::context::store::StoreState;
use crate::fs::knowledge::catalog::Catalog;
use anyhow::Result;

/// The corpus one or more queries are answered from.
pub(crate) struct Captured {
    roots: ResolvedRoots,
    config: RetrievalConfig,
    catalog: Catalog,
    state: StoreState,
    graph_load: GraphLoad,
}

impl Captured {
    /// Revision of the knowledge layer this capture was built from.
    pub(crate) fn structural_revision(&self) -> &str {
        &self.state.structural.revision
    }

    /// Revision of the source-graph layer this capture was built from.
    pub(crate) fn semantic_revision(&self) -> &str {
        &self.state.semantic.revision
    }
}

/// Resolve roots, catalog, freshness and graph for `query`'s work directory and
/// overlay. Later queries may differ in text, required ids and budget only.
pub(crate) fn capture(query: &StageQuery) -> Result<Captured> {
    let roots = resolve_roots_optional(&query.work_dir_hint)?;
    let config = RetrievalConfig::load(&roots.main_project_root);
    let (catalog, mut state) = resolve_catalog(&roots.store, roots.knowledge_root.as_deref())?;
    let graph_load = graph::load_resolved_graph(
        &query.work_dir_hint,
        &roots.store,
        &state.semantic.revision,
        &query.overlay,
    );
    graph_load.apply_freshness(&mut state);
    Ok(Captured {
        roots,
        config,
        catalog,
        state,
        graph_load,
    })
}

/// Rank and pack `query` against `captured`.
pub(crate) fn retrieve_with(
    captured: &Captured,
    query: &StageQuery,
    budget_tokens: usize,
) -> Result<ContextPack> {
    let graph = captured.graph_load.graph.as_ref();
    request::check_require_ids(query, &captured.catalog, graph)?;

    let ranked = channels::rank_stage_channels(
        query,
        &captured.catalog,
        graph,
        &captured.config,
        captured.roots.store.root(),
    );
    let fused = fuse(&ranked.lists);

    let request = request::build_pack_request(
        query,
        budget_tokens,
        captured.state.clone(),
        ranked.dropped_terms,
        ranked.surviving_terms,
        captured.graph_load.degraded.clone(),
    );
    let mut retrieved = pack(&request, &fused, &captured.catalog.chunks, graph);
    if let (Some(graph), Some(project_root)) = (graph, captured.graph_load.project_root.as_deref())
    {
        windows::attach_windows(&mut retrieved, graph, project_root, query.surface);
    }
    Ok(retrieved)
}
