//! Freshness evaluation and structural-layer rebuilds for the context store.
//!
//! [`evaluate`] answers "is the cached catalog current?" without touching
//! disk beyond a read. [`refresh`] acts on that answer: it rebuilds and
//! persists the structural layer when stale, and is a no-op otherwise. The
//! semantic (source-graph) layer lives in [`source_graph`].

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::context::fingerprint::{fingerprint_tree, tree_revision};
use crate::context::ingest::{ingest, IngestReport};
use crate::context::schema::Freshness;
use crate::context::store::{ContextStore, StoreState};

mod source_graph;

pub use source_graph::{
    mark_semantic_stale, reconcile_source_graph, SourceGraphOutcome, SourceGraphScope,
};

/// One entry of the extractor registry the semantic refresh drives.
///
/// Named here rather than in `context::extract` because the boxing is the
/// refresh driver's requirement, not the trait's: the driver builds the whole
/// registry once and hands slices of it down its own call chain, which needs
/// `Send + Sync` for no reason a single extractor implementation cares about.
pub(crate) type BoxedExtractor =
    Box<dyn crate::context::extract::SourceGraphExtractor + Send + Sync>;

/// The revision a reconcile builds against, plus its two reuse sources: the
/// stage's own overlay and the published base at that revision, either of
/// which may be absent on a first build.
type ScopeLayers = (
    String,
    Option<crate::context::graph_store::GraphLayer>,
    Option<crate::context::graph_store::GraphLayer>,
);

/// [`ScopeLayers`] preceded by the tracked-file list to walk.
type ReconcileInputs = (
    Vec<String>,
    String,
    Option<crate::context::graph_store::GraphLayer>,
    Option<crate::context::graph_store::GraphLayer>,
);

/// What a refresh actually did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshOutcome {
    /// False when the cached catalog was already current.
    pub rebuilt: bool,
    /// Structural (catalog) freshness after this call.
    pub structural: Freshness,
    /// Semantic (source graph) freshness after this call.
    pub semantic: Freshness,
    /// Present only when a rebuild happened.
    pub report: Option<IngestReport>,
}

/// Decide whether the structural (catalog) layer is stale, given the stored
/// freshness record and the freshly computed tree revision.
fn structural_freshness(stored: &Freshness, current_revision: &str) -> Freshness {
    if stored.revision.is_empty() {
        Freshness::never_built("catalog has never been built")
    } else if stored.revision != current_revision {
        Freshness {
            revision: stored.revision.clone(),
            computed_at: stored.computed_at,
            stale: true,
            detail: Some("knowledge tree changed since the catalog was built".to_string()),
        }
    } else {
        Freshness {
            stale: false,
            detail: None,
            ..stored.clone()
        }
    }
}

/// Compare the cached state against the knowledge tree WITHOUT rebuilding.
///
/// `structural.stale` is true when the stored structural revision differs
/// from the freshly computed tree revision, when nothing has been built yet,
/// or when the persisted catalog on disk does not match what `state.json`
/// claims was built (missing, deleted, or overwritten out from under it —
/// `save_catalog` and `save_state` are two independent writes with no
/// cross-file transaction, so this is the only place that catches the two
/// files disagreeing). The semantic layer is not evaluated against the source
/// tree here — it is owned by the source-graph stage, so a stored semantic
/// revision is returned unchanged and an absent one is reported as never built.
pub fn evaluate(store: &ContextStore, knowledge_root: &Path) -> Result<StoreState> {
    let stored = store.load_state()?;

    let fingerprints = fingerprint_tree(knowledge_root)?;
    let current_revision = tree_revision(&fingerprints);

    let mut structural = structural_freshness(&stored.structural, &current_revision);

    // A structural revision was recorded, meaning a catalog was supposedly
    // built — confirm the catalog on disk still backs that claim. Skip this
    // when nothing has ever been built: that case already has its own detail
    // message above, and an absent catalog there is expected, not a mismatch.
    if !stored.structural.revision.is_empty() {
        let catalog = store.load_catalog()?;
        let catalog_is_missing_or_stale = match &catalog {
            None => true,
            Some(catalog) => catalog.revision != stored.catalog_revision,
        };
        if catalog_is_missing_or_stale {
            structural.stale = true;
            structural.detail = Some(
                "cached catalog is missing or does not match the recorded revision".to_string(),
            );
        }
    }

    // The semantic layer is populated by the source-graph stage, not this one.
    // Never invent a revision for it: pass a stored one through unchanged, and
    // report an absent one as never built.
    let semantic = if stored.semantic.revision.is_empty() {
        Freshness::never_built("source graph not built; see the source-graph stage")
    } else {
        stored.semantic.clone()
    };

    Ok(StoreState {
        structural,
        semantic,
        catalog_revision: stored.catalog_revision,
    })
}

/// Rebuild the structural layer when it is stale; persist catalog and state.
/// `structural_only` distinguishes catalog-only from also best-effort
/// reconciling the semantic layer (see [`source_graph::reconcile_semantic_best_effort`]).
pub fn refresh(
    store: &ContextStore,
    knowledge_root: &Path,
    structural_only: bool,
) -> Result<RefreshOutcome> {
    let evaluated = evaluate(store, knowledge_root)?;

    if !evaluated.structural.stale {
        let semantic = if structural_only {
            evaluated.semantic
        } else {
            source_graph::reconcile_semantic_best_effort(store, knowledge_root, evaluated.semantic)
        };
        return Ok(RefreshOutcome {
            rebuilt: false,
            structural: evaluated.structural,
            semantic,
            report: None,
        });
    }

    let mut outcome = rebuild_and_persist(store, knowledge_root, evaluated.semantic)?;
    if !structural_only {
        outcome.semantic =
            source_graph::reconcile_semantic_best_effort(store, knowledge_root, outcome.semantic);
    }
    Ok(outcome)
}

/// Rebuild the catalog from `knowledge_root`, persist it and the derived
/// state, and return the outcome. Only called from `refresh` once `evaluate`
/// has found the structural layer stale.
fn rebuild_and_persist(
    store: &ContextStore,
    knowledge_root: &Path,
    semantic: Freshness,
) -> Result<RefreshOutcome> {
    // Persist the SAME revision `evaluate` compares against — the tree revision.
    // `catalog.revision` is a different hash over a different subject (chunk ids
    // and their content hashes, not file paths and their bytes); storing it here
    // would compare two hash domains that can never be equal, making every
    // refresh rebuild from scratch. The catalog keeps its own revision inside
    // catalog.json as the catalog's identity.
    let fingerprints = fingerprint_tree(knowledge_root)?;
    let current_revision = tree_revision(&fingerprints);

    let (catalog, report) = ingest(knowledge_root)?;
    store.save_catalog(&catalog)?;

    let structural = Freshness {
        revision: current_revision,
        computed_at: Some(Utc::now()),
        stale: false,
        detail: None,
    };

    store.save_state(&StoreState {
        structural: structural.clone(),
        semantic: semantic.clone(),
        catalog_revision: catalog.revision.clone(),
    })?;

    Ok(RefreshOutcome {
        rebuilt: true,
        structural,
        semantic,
        report: Some(report),
    })
}
