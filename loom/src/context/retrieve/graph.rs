//! Resolve a query's source graph against the overlay's base or checkout HEAD,
//! and report missing bases and overlays whose base revision is unknown.
//!
//! Split out of `retrieve.rs` so the top-level pipeline in
//! [`super::retrieve_for_stage`] stays a readable sequence of steps rather
//! than growing this reasoning inline. See
//! `doc/PROPOSAL-retrieval-precision.md` §A.11 for the design this
//! implements.

use crate::context::graph_store::{GraphRead, GraphStore, ResolvedGraph};
use crate::context::local_overlay::OverlayScope;
use crate::context::refresh::{clean_generation, short_revision, working_tree};
use crate::context::store::{ContextStore, StoreState};
use crate::fs::work_dir::WorkDir;
use std::path::{Path, PathBuf};

pub(super) struct GraphLoad {
    pub graph: Option<ResolvedGraph>,
    /// The checkout the graph's paths are relative to, for reading source back.
    pub project_root: Option<PathBuf>,
    pub degraded: Option<String>,
    /// Why the served graph does not describe this checkout's current tree.
    pub stale_detail: Option<String>,
    pub served_revision: Option<String>,
}

/// A loaded graph, and the note that its cross-file edges could not be resolved.
type LoadedGraph = (ResolvedGraph, Option<String>);

impl GraphLoad {
    fn empty() -> Self {
        Self {
            graph: None,
            project_root: None,
            degraded: None,
            stale_detail: None,
            served_revision: None,
        }
    }

    pub(super) fn apply_freshness(&self, state: &mut StoreState) {
        if let Some(revision) = &self.served_revision {
            if state.semantic.revision != *revision {
                state.semantic.revision = revision.clone();
                state.semantic.computed_at = None;
                state.semantic.stale = self
                    .graph
                    .as_ref()
                    .is_none_or(|graph| revision.is_empty() || graph.base_revision != *revision);
                state.semantic.unavailable = false;
                state.semantic.detail = state
                    .semantic
                    .stale
                    .then(|| "source graph base for the served snapshot is unavailable".into());
            }
            if !revision.is_empty()
                && self
                    .graph
                    .as_ref()
                    .is_some_and(|graph| graph.base_revision.is_empty())
            {
                state.semantic.stale = true;
                state.semantic.detail =
                    Some("source graph base for the served snapshot is unavailable".into());
            }
        }
        if let Some(detail) = &self.stale_detail {
            state.semantic.stale = true;
            state.semantic.detail = Some(detail.clone());
        }
    }
}

/// Load the checkout source layers and resolve a derived view.
/// Retrieval may cache the view, but does not rebuild source layers. An
/// overlay chooses its recorded base; without one, the checkout HEAD does, and
/// a HEAD with no base serves the newest base there is, labelled stale.
pub(super) fn load_resolved_graph(
    work_dir_hint: &Path,
    store: &ContextStore,
    semantic_revision: &str,
    overlay: &OverlayScope,
) -> GraphLoad {
    let Some(work_dir) = WorkDir::new(work_dir_hint).ok() else {
        return GraphLoad::empty();
    };
    let Some(project_root) = work_dir.project_root() else {
        return GraphLoad::empty();
    };
    let (plan, stage) = overlay.resolve(project_root);
    let graph_store = GraphStore::new(store.root(), work_dir.root());
    let tree = working_tree(project_root).ok();
    let requested = tree.as_ref().map_or(semantic_revision, |tree| &tree.head);
    let Ok(read) = graph_store.graph_read(requested, Some((&plan, &stage))) else {
        return GraphLoad::empty();
    };
    let Some(loaded) = load_graph(&graph_store, &read) else {
        return GraphLoad::empty();
    };
    let (read, (graph, view_note), older_base) = with_older_base(&graph_store, read, loaded);

    let degraded = graph_degradation(&read, &graph, view_note);
    let stale_detail = older_base.or_else(|| {
        tree.and_then(|tree| match &read.overlay {
            Some(layer) if layer.revision != tree.head || layer.generation != tree.generation => {
                Some("working tree changed since the source graph overlay was built".to_string())
            }
            None if tree.generation != clean_generation(&tree.head) => {
                Some("working tree has changes but no source graph overlay exists".to_string())
            }
            _ => None,
        })
    });
    GraphLoad {
        graph: Some(graph),
        project_root: Some(project_root.to_path_buf()),
        degraded,
        stale_detail,
        served_revision: Some(read.revision),
    }
}

/// The read's view, or its unresolved graph with a note when no view can be
/// built.
fn load_graph(graph_store: &GraphStore, read: &GraphRead<'_>) -> Option<LoadedGraph> {
    match graph_store.view_read(read) {
        Ok(view) => Some((view.graph, None)),
        Err(error) => {
            let (graph, _) = graph_store.resolve_read(read).ok()?;
            let note =
                format!("resolved view unavailable ({error}); cross-file edges are not resolved");
            Some((graph, Some(note)))
        }
    }
}

/// A read with neither an overlay nor a base has nothing to answer from, so it
/// serves the newest base there is, with the label saying so. An overlay never
/// falls back: its files are a delta against one revision only.
fn with_older_base<'a>(
    graph_store: &GraphStore,
    read: GraphRead<'a>,
    loaded: LoadedGraph,
) -> (GraphRead<'a>, LoadedGraph, Option<String>) {
    if read.overlay.is_some() || !loaded.0.base_revision.is_empty() {
        return (read, loaded, None);
    }
    let Ok(Some(newest)) = graph_store.load_newest_base() else {
        return (read, loaded, None);
    };
    let older = GraphRead {
        revision: newest.revision,
        address: None,
        overlay: None,
    };
    let Some(older_loaded) = load_graph(graph_store, &older) else {
        return (read, loaded, None);
    };
    let detail = format!(
        "serving source graph base {}: no base or overlay for revision {} yet",
        short_revision(&older.revision),
        short_revision(&read.revision)
    );
    (older, older_loaded, Some(detail))
}

fn graph_degradation(
    read: &GraphRead<'_>,
    graph: &ResolvedGraph,
    view_note: Option<String>,
) -> Option<String> {
    if read.overlay.is_some() && read.revision.is_empty() {
        Some("source graph overlay has no recorded base revision".to_string())
    } else if read.overlay.is_some() && !read.revision.is_empty() && graph.base_revision.is_empty()
    {
        Some(format!(
            "source graph base {} missing; serving overlay only",
            short_revision(&read.revision)
        ))
    } else {
        degraded_reason(&read.revision, graph).or(view_note)
    }
}

/// The degraded reason for a graph that was never built and holds no content.
const NEVER_BUILT_REASON: &str = "source graph never built; run loom map to build it";

/// Report an absent base and overlay when no graph content is available.
fn degraded_reason(semantic_revision: &str, graph: &ResolvedGraph) -> Option<String> {
    if semantic_revision.is_empty() {
        return graph
            .files
            .is_empty()
            .then(|| NEVER_BUILT_REASON.to_string());
    }
    if !graph.base_revision.is_empty() || !graph.files.is_empty() {
        return None;
    }
    Some(format!(
        "source graph base {} missing and no overlay covers this checkout — no source graph content available",
        short_revision(semantic_revision)
    ))
}
