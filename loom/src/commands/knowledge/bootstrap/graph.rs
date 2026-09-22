//! Source graph loading for knowledge bootstrap.
//!
//! `loom map` degrades on a failed snapshot: it prints the outcome and
//! resolves whatever layers exist, so a failed overlay yields the stale base
//! and a missing base an empty graph. Bootstrap must not certify that: its
//! clusters, digests and receipt would describe a graph that is not the
//! working tree. [`load_current_graph`] therefore refuses an unavailable
//! snapshot before anything is briefed, spawned or written.

use std::path::Path;

use anyhow::{bail, Result};

use crate::context::graph_store::{GraphStore, ResolvedGraph};
use crate::context::refresh::{ensure_snapshot, SnapshotAction, SnapshotOutcome, SnapshotPolicy};
use crate::context::resolve_graph;
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;

/// Ensure the local snapshot, refuse it when unavailable, then load its
/// layers and resolve inferred edges. Mirrors `commands/map.rs::load_graph`
/// apart from the [`require_snapshot`] gate.
pub(super) fn load_current_graph(repo_root: &Path) -> Result<ResolvedGraph> {
    let work_dir = WorkDir::new(repo_root)?;
    let store = ContextStore::open(&work_dir)?;
    store.ensure()?;
    let graph_store = GraphStore::new(store.root(), work_dir.root());
    let snapshot = ensure_snapshot(
        &store,
        &graph_store,
        repo_root,
        SnapshotPolicy::LocalCurrent,
    );
    require_snapshot(&snapshot)?;
    if snapshot.action != SnapshotAction::Reused {
        eprintln!("{}", snapshot.describe());
    }
    let overlay = snapshot
        .overlay
        .as_ref()
        .map(|(plan, stage)| (plan.as_str(), stage.as_str()));
    let mut graph = graph_store.resolved(&snapshot.revision, overlay)?;
    resolve_graph(&mut graph);
    Ok(graph)
}

/// Refuse a snapshot `ensure_snapshot` could not produce.
pub(super) fn require_snapshot(outcome: &SnapshotOutcome) -> Result<()> {
    if outcome.action == SnapshotAction::Unavailable {
        bail!(
            "source graph unavailable: {}; nothing was spawned and no receipt was written",
            outcome.reason
        );
    }
    Ok(())
}
