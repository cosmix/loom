//! One policy-driven decision path for ensuring source-graph snapshots.

use anyhow::Result;
use std::{
    path::Path,
    time::{Duration, Instant},
};

use super::source_graph::{
    clean_generation, reconcile_with_working_tree, replace_base_with_working_tree,
    try_mark_semantic_stale, working_tree, WorkingTree,
};
use super::{SourceGraphCounters, SourceGraphOutcome, SourceGraphScope};
use crate::context::extract;
use crate::context::graph_store::{GraphLayer, GraphStore};
use crate::context::local_overlay::local_overlay_key;
use crate::context::store::ContextStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotPolicy {
    /// Base for HEAD present, plus the `_local` overlay current when the tree is dirty.
    LocalCurrent,
    /// The named stage overlay current (a stage worktree; no base publish).
    StageOverlay { plan: String, stage: String },
    /// Base for HEAD present; the working tree is not consulted beyond HEAD.
    BaseOnly,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotAction {
    Reused,
    Updated,
    Rebuilt,
    Unavailable,
}
impl SnapshotAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reused => "reused",
            Self::Updated => "updated",
            Self::Rebuilt => "rebuilt",
            Self::Unavailable => "unavailable",
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotOutcome {
    pub action: SnapshotAction,
    pub reason: String,
    pub revision: String,
    pub generation: String,
    pub overlay: Option<(String, String)>,
    pub counters: SourceGraphCounters,
    pub elapsed: Duration,
    /// False when a read-only cache kept part of the refresh off the disk: the
    /// `state.json` write was refused, or `GraphStore` serves a layer or a
    /// resolved view from memory for this process only.
    pub persisted: bool,
    /// The older base a failed build fell back to; `revision` then names it.
    pub serving: Option<String>,
}

mod describe;
mod materialize;
mod state;

/// Whether a layer entry, or every entry of a layer, was stamped by the
/// extractor now registered for its path; `worktree_graph` and the resolved
/// view reach the checks through here.
pub(crate) use super::source_graph::{entries_are_current, parser_version_matches};

/// Ensure the policy-selected graph layer without making callers repeat its decision tree.
///
/// Every failure - inspecting the working tree, building a layer, or
/// persisting it - is reported as `SnapshotAction::Unavailable` with the
/// full cause chain in `reason`. A failed build over a known HEAD serves the
/// newest older base instead (`SnapshotOutcome::serving`). A write the cache
/// denies is no failure: the layer serves this process from memory, and the
/// outcome keeps its action with `persisted` false.
pub fn ensure_snapshot(
    store: &ContextStore,
    graph_store: &GraphStore,
    project_root: &Path,
    policy: SnapshotPolicy,
) -> SnapshotOutcome {
    let started = Instant::now();
    let tree = match working_tree(project_root) {
        Ok(tree) => tree,
        Err(error) => {
            let reason = format!("failed to inspect the working tree: {error:#}");
            try_mark_semantic_stale(store, &reason);
            return unavailable(reason, started.elapsed());
        }
    };

    let dispatched = match policy {
        SnapshotPolicy::BaseOnly => ensure_base_only(store, graph_store, project_root, &tree),
        SnapshotPolicy::LocalCurrent => {
            ensure_local_current(store, graph_store, project_root, &tree)
        }
        SnapshotPolicy::StageOverlay { plan, stage } => {
            ensure_stage_overlay(store, graph_store, project_root, &tree, plan, stage)
        }
    };

    let outcome = match dispatched {
        Ok(outcome) => outcome,
        Err(error) => {
            let reason = format!("{error:#}");
            try_mark_semantic_stale(store, &reason);
            unavailable_with_tree(&tree, reason)
        }
    };
    let mut outcome = state::serve_stale_base(graph_store, outcome);
    materialize::materialize_views(graph_store, &mut outcome);
    outcome.persisted &= !graph_store.fell_back();
    outcome.elapsed = started.elapsed();
    outcome
}

fn ensure_base_only(
    store: &ContextStore,
    graph_store: &GraphStore,
    project_root: &Path,
    tree: &WorkingTree,
) -> Result<SnapshotOutcome> {
    match ensure_base(store, graph_store, project_root, tree)? {
        Some(outcome) => from_reconcile(tree, None, outcome, "base refreshed"),
        None => Ok(reused(
            tree,
            None,
            format!(
                "base for {} present; {}",
                super::short_revision(&tree.head),
                tree_state(tree)
            ),
        )),
    }
}

fn ensure_local_current(
    store: &ContextStore,
    graph_store: &GraphStore,
    project_root: &Path,
    tree: &WorkingTree,
) -> Result<SnapshotOutcome> {
    let base = ensure_base(store, graph_store, project_root, tree)?;
    if tree.generation == clean_generation(&tree.head) {
        return match base {
            Some(outcome) => from_reconcile(tree, None, outcome, "base refreshed; tree clean"),
            None => Ok(reused(
                tree,
                None,
                format!(
                    "base for {} present; tree clean",
                    super::short_revision(&tree.head)
                ),
            )),
        };
    }

    let (plan, stage) = local_overlay_key(project_root);
    ensure_local_overlay(store, graph_store, project_root, tree, plan, stage, base)
}

fn ensure_base(
    store: &ContextStore,
    graph_store: &GraphStore,
    project_root: &Path,
    tree: &WorkingTree,
) -> Result<Option<SourceGraphOutcome>> {
    let replace = match graph_store.load_base(&tree.head)? {
        Some(base) if layer_is_current(&base) => return Ok(None),
        Some(_) => true,
        // `load_base` reports an unparseable file as absent.
        None => graph_store.base_path(&tree.head).exists(),
    };
    // A stale or unparseable base is overwritten in place, never deleted
    // first: a delete fails on a read-only cache, which must serve the rebuild
    // from memory, and can remove a racer's current base. A racer publishing
    // between this check and the write is harmless on either route:
    // `publish_base` keeps its base, `replace_base` overwrites it with an
    // equally current one.
    let outcome = if replace {
        replace_base_with_working_tree(store, graph_store, project_root, tree)?
    } else {
        let scope = SourceGraphScope::Base {
            revision: tree.head.clone(),
        };
        reconcile_with_working_tree(store, graph_store, project_root, scope, tree)?
    };
    Ok(Some(outcome))
}

fn ensure_stage_overlay(
    store: &ContextStore,
    graph_store: &GraphStore,
    project_root: &Path,
    tree: &WorkingTree,
    plan: String,
    stage: String,
) -> Result<SnapshotOutcome> {
    let overlay = Some((plan.clone(), stage.clone()));
    if overlay_is_current(graph_store, &plan, &stage, &tree.generation)? {
        return Ok(reused(
            tree,
            overlay,
            format!("stage overlay {plan}/{stage} generation current"),
        ));
    }
    let outcome = reconcile_overlay(store, graph_store, project_root, tree, &plan, &stage)?;
    from_reconcile(tree, overlay, outcome, "stage overlay refreshed")
}

#[allow(clippy::too_many_arguments)]
fn ensure_local_overlay(
    store: &ContextStore,
    graph_store: &GraphStore,
    project_root: &Path,
    tree: &WorkingTree,
    plan: String,
    stage: String,
    base: Option<SourceGraphOutcome>,
) -> Result<SnapshotOutcome> {
    let overlay = Some((plan.clone(), stage.clone()));
    if overlay_is_current(graph_store, &plan, &stage, &tree.generation)? {
        return match base {
            Some(outcome) => from_reconcile(tree, overlay, outcome, "base refreshed"),
            None => Ok(reused(
                tree,
                overlay,
                format!("local overlay {plan}/{stage} generation current"),
            )),
        };
    }

    let mut outcome = reconcile_overlay(store, graph_store, project_root, tree, &plan, &stage)?;
    if let Some(base) = base {
        outcome.counters.accumulate(base.counters);
        outcome.state_persisted &= base.state_persisted;
    }
    from_reconcile(tree, overlay, outcome, "local overlay refreshed")
}

fn reconcile_overlay(
    store: &ContextStore,
    graph_store: &GraphStore,
    project_root: &Path,
    tree: &WorkingTree,
    plan: &str,
    stage: &str,
) -> Result<SourceGraphOutcome> {
    reconcile_with_working_tree(
        store,
        graph_store,
        project_root,
        SourceGraphScope::Overlay {
            plan: plan.to_string(),
            stage: stage.to_string(),
        },
        tree,
    )
}

fn overlay_is_current(
    graph_store: &GraphStore,
    plan: &str,
    stage: &str,
    generation: &str,
) -> Result<bool> {
    let Some(layer) = graph_store.load_overlay(plan, stage)? else {
        return Ok(false);
    };
    if layer.generation != generation || !layer_is_current(&layer) {
        return Ok(false);
    }
    // A delta left its unchanged files to its base. With that base pruned it
    // no longer describes the tree; reconciling rebuilds it complete.
    Ok(layer.complete
        || graph_store
            .load_base(&layer.revision)?
            .is_some_and(|base| base.has_current_schema()))
}

/// A layer is current when it was written under
/// [`GRAPH_SCHEMA_VERSION`](crate::context::source_graph::GRAPH_SCHEMA_VERSION)
/// and every entry's parser version matches the extractor that would parse it
/// now ([`parser_version_matches`]).
fn layer_is_current(layer: &GraphLayer) -> bool {
    layer.has_current_schema() && entries_are_current(&layer.files, &extract::registry())
}

fn from_reconcile(
    tree: &WorkingTree,
    overlay: Option<(String, String)>,
    outcome: SourceGraphOutcome,
    reason: &str,
) -> Result<SnapshotOutcome> {
    if outcome.freshness.stale {
        let detail = outcome
            .freshness
            .detail
            .unwrap_or_else(|| "source graph could not be refreshed".to_string());
        return Ok(unavailable_with_tree(tree, detail));
    }
    let action = if outcome.counters.files_reused > 0 {
        SnapshotAction::Updated
    } else {
        SnapshotAction::Rebuilt
    };
    Ok(SnapshotOutcome {
        action,
        reason: reason.to_string(),
        revision: tree.head.clone(),
        generation: tree.generation.clone(),
        overlay,
        counters: outcome.counters,
        elapsed: Duration::ZERO,
        persisted: outcome.state_persisted,
        serving: None,
    })
}

fn reused(
    tree: &WorkingTree,
    overlay: Option<(String, String)>,
    reason: String,
) -> SnapshotOutcome {
    SnapshotOutcome {
        action: SnapshotAction::Reused,
        reason,
        revision: tree.head.clone(),
        generation: tree.generation.clone(),
        overlay,
        counters: SourceGraphCounters::default(),
        elapsed: Duration::ZERO,
        persisted: true,
        serving: None,
    }
}

fn unavailable(reason: String, elapsed: Duration) -> SnapshotOutcome {
    SnapshotOutcome {
        action: SnapshotAction::Unavailable,
        reason,
        revision: String::new(),
        generation: String::new(),
        overlay: None,
        counters: SourceGraphCounters::default(),
        elapsed,
        persisted: true,
        serving: None,
    }
}

fn unavailable_with_tree(tree: &WorkingTree, reason: String) -> SnapshotOutcome {
    SnapshotOutcome {
        revision: tree.head.clone(),
        generation: tree.generation.clone(),
        ..unavailable(reason, Duration::ZERO)
    }
}

fn tree_state(tree: &WorkingTree) -> &'static str {
    if tree.generation == clean_generation(&tree.head) {
        "tree clean"
    } else {
        "tree dirty"
    }
}
