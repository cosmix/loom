//! Freshness evaluation and structural-layer rebuilds for the context store.
//!
//! [`evaluate`] answers "is the cached catalog current?" without touching
//! disk beyond a read. [`refresh`] acts on that answer: it rebuilds and
//! persists the structural layer when stale, and is a no-op otherwise. The
//! semantic (source-graph) layer lives in `source_graph`.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::context::fingerprint::{fingerprint_tree, tree_revision, FileFingerprint};
use crate::context::ingest::IngestReport;
use crate::context::schema::Freshness;
use crate::context::store::{ContextStore, StoreState};
use crate::fs::knowledge::catalog::{self, Catalog};

mod semantic;
pub mod snapshot;
mod source_graph;
mod structural_snapshot;

pub use semantic::{SemanticLayer, SemanticOutcome, SOURCE_GRAPH_PREFIX};
pub use snapshot::{ensure_snapshot, SnapshotAction, SnapshotOutcome, SnapshotPolicy};
pub(crate) use source_graph::{clean_generation, excluded, working_tree, EXCLUDED_ROOTS};
pub use source_graph::{
    mark_semantic_stale, reconcile_source_graph, SourceGraphCounters, SourceGraphOutcome,
    SourceGraphScope,
};

pub(crate) use crate::context::extract::BoxedExtractor;

struct Evaluated {
    state: StoreState,
    fingerprints: Vec<FileFingerprint>,
    catalog: Option<Catalog>,
}

/// What a refresh actually did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefreshOutcome {
    /// False when the cached catalog was already current.
    pub rebuilt: bool,
    /// Structural (catalog) freshness after this call.
    pub structural: Freshness,
    /// What the semantic (source graph) half of this call did - which layer it
    /// wrote, how big it is, and how fresh it now is.
    pub semantic: SemanticOutcome,
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
            unavailable: false,
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
/// claims was built (missing, deleted, or overwritten out from under it).
///
/// The semantic layer is still owned by the source-graph stage — this
/// function never rebuilds it, and an absent semantic revision is reported as
/// never built, same as always. What DOES happen now: when a semantic
/// revision has been recorded, it is checked against `git rev-parse HEAD` for
/// the project `knowledge_root` belongs to (`semantic_freshness_against_head`
/// below), and reported `stale: true` when HEAD has moved past it. This is
/// display plus a trigger for a later background reconcile — NOT a rebuild:
/// [`refresh`] only ever rebuilds the STRUCTURAL layer when it finds
/// `structural.stale`, and never reads this function's semantic verdict to
/// decide whether to rebuild anything, so a moved HEAD reported here cannot
/// start one. `revision` itself always stays the STORED value: the revision
/// last published to the shared cache, which no longer selects what a query
/// reads. `load_resolved_graph` (`context/retrieve/graph.rs`) keys its read by
/// the overlay's recorded revision or the checkout's HEAD, uses the stored
/// value only when HEAD cannot be read, and rewrites the freshness to describe
/// what it served. Only `stale`/`detail` change here. A knowledge root whose
/// project cannot be resolved degrades to passing the stored semantic freshness
/// through unchanged. A project with no
/// git HEAD (not a repository, no commits, `git` unavailable) is reported
/// `unavailable` with the stored revision kept — a missing git repository is
/// data, not a crash (`refresh/source_graph.rs`'s module doc).
pub fn evaluate(store: &ContextStore, knowledge_root: &Path) -> Result<StoreState> {
    Ok(evaluate_inner(store, knowledge_root)?.state)
}

fn evaluate_inner(store: &ContextStore, knowledge_root: &Path) -> Result<Evaluated> {
    let (stored, catalog, state_unavailable) = store
        .load_snapshot()
        .map(|(state, catalog)| (state, catalog, false))
        .unwrap_or_else(|error| {
            eprintln!("warning: context cache unreadable ({error}); rebuilding cache");
            (StoreState::default(), Ok(None), true)
        });
    let catalog = catalog.unwrap_or_else(|error| {
        eprintln!("warning: context catalog unreadable ({error}); rebuilding cache");
        None
    });

    let fingerprints = fingerprint_once(knowledge_root)?;
    let current_revision = tree_revision(&fingerprints);

    let structural = structural_snapshot::catalog_freshness(&stored, &catalog, &current_revision);

    // The semantic layer is populated by the source-graph stage, not this
    // one. Never invent a revision for it, and never rebuild it here — see
    // the doc comment above and on `semantic_freshness_against_head`.
    let semantic = if state_unavailable {
        Freshness::unavailable("semantic freshness unavailable: context state unreadable")
    } else if stored.semantic.revision.is_empty() {
        Freshness::never_built("source graph not built; see the source-graph stage")
    } else {
        semantic_freshness_against_head(knowledge_root, stored.semantic.clone())
    };

    Ok(Evaluated {
        state: StoreState {
            structural,
            semantic,
            catalog_revision: stored.catalog_revision,
        },
        fingerprints,
        catalog,
    })
}

fn fingerprint_once(knowledge_root: &Path) -> Result<Vec<FileFingerprint>> {
    #[cfg(test)]
    FINGERPRINT_PASSES.with(|count| count.set(count.get().saturating_add(1)));
    fingerprint_tree(knowledge_root)
}

#[cfg(test)]
std::thread_local! {
    static FINGERPRINT_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn take_fingerprint_pass_count() -> usize {
    FINGERPRINT_PASSES.with(|count| count.replace(0))
}

/// Check `stored` (a non-empty semantic revision) against the current
/// `git rev-parse HEAD` for the project `knowledge_root` belongs to.
///
/// Read-only: no rebuild, no write, no reconcile. Only `stale`/`detail` are
/// ever changed — `revision` is returned exactly as `stored` held it, because
/// that is the base layer actually on disk (see [`evaluate`]'s doc comment).
/// When the project root cannot be derived from `knowledge_root`, `stored` is
/// returned completely unchanged — this must never invent a verdict from an
/// unusable comparison. A project with no resolvable git HEAD is reported
/// `unavailable` (the stored `revision` kept), so no surface reads it as
/// current.
fn semantic_freshness_against_head(knowledge_root: &Path, stored: Freshness) -> Freshness {
    let Some(project_root) = semantic::derive_project_root(knowledge_root) else {
        return stored;
    };
    let Some(head) = source_graph::head_revision(project_root) else {
        return Freshness {
            revision: stored.revision,
            computed_at: stored.computed_at,
            ..Freshness::unavailable("project has no resolvable git HEAD")
        };
    };
    if head == stored.revision {
        return stored;
    }

    Freshness {
        revision: stored.revision.clone(),
        computed_at: stored.computed_at,
        stale: true,
        detail: Some(format!(
            "HEAD moved: {} → {}",
            short_revision(&stored.revision),
            short_revision(&head)
        )),
        unavailable: false,
    }
}

/// First 8 characters of `revision`, or the whole string when it is shorter.
///
/// Character-based, not a byte slice: `&revision[..8]` panics both when
/// `revision` has fewer than 8 bytes and when byte index 8 does not fall on a
/// UTF-8 character boundary. This runs inside `evaluate`, reachable from the
/// prompt hook, which is contractually forbidden to ever disturb a session —
/// see `loom-hooks/user-prompt-context.sh`'s fail-open contract.
///
/// `pub(crate)`: `context::retrieve::graph` needs the identical truncation
/// for its A.11 degraded-message (`"source graph base <rev8> missing"`) and
/// must not grow a second, possibly-drifting copy of a panic-safety rule.
pub(crate) fn short_revision(revision: &str) -> String {
    revision.chars().take(8).collect()
}

/// Why the semantic layer was skipped when the caller asked for the catalog only.
const STRUCTURAL_ONLY_REASON: &str = "--structural-only";

/// Rebuild the structural layer when it is stale; persist catalog and state.
/// `structural_only` distinguishes catalog-only from also best-effort
/// reconciling the semantic layer (see
/// `semantic::reconcile_semantic_best_effort_from_knowledge_root`).
pub fn refresh(
    store: &ContextStore,
    knowledge_root: &Path,
    structural_only: bool,
) -> Result<RefreshOutcome> {
    Ok(refresh_snapshot_inner(store, knowledge_root, structural_only, false)?.0)
}

pub(crate) fn refresh_snapshot(
    store: &ContextStore,
    knowledge_root: &Path,
    structural_only: bool,
) -> Result<(RefreshOutcome, Catalog)> {
    refresh_snapshot_inner(store, knowledge_root, structural_only, true)
}

fn refresh_snapshot_inner(
    store: &ContextStore,
    knowledge_root: &Path,
    structural_only: bool,
    tolerate_write_failure: bool,
) -> Result<(RefreshOutcome, Catalog)> {
    let evaluated = evaluate_inner(store, knowledge_root)?;
    let state = evaluated.state;

    if !state.structural.stale {
        let semantic = if structural_only {
            SemanticOutcome::skipped(state.semantic, STRUCTURAL_ONLY_REASON)
        } else {
            semantic::reconcile_semantic_best_effort_from_knowledge_root(
                store,
                knowledge_root,
                state.semantic,
            )
        };
        let outcome = RefreshOutcome {
            rebuilt: false,
            structural: state.structural,
            semantic,
            report: None,
        };
        return Ok((
            outcome,
            evaluated.catalog.expect("current catalog is validated"),
        ));
    }

    let (mut outcome, catalog) = rebuild_and_persist(
        store,
        knowledge_root,
        state.semantic,
        evaluated.fingerprints,
        tolerate_write_failure,
    )?;
    if !structural_only {
        let current = outcome.semantic.freshness.clone();
        outcome.semantic = semantic::reconcile_semantic_best_effort_from_knowledge_root(
            store,
            knowledge_root,
            current,
        );
    }
    Ok((outcome, catalog))
}

/// Rebuild the catalog from `knowledge_root`, persist it and the derived
/// state, and return the outcome. Only called from `refresh` once `evaluate`
/// has found the structural layer stale.
fn rebuild_and_persist(
    store: &ContextStore,
    knowledge_root: &Path,
    semantic: Freshness,
    fingerprints: Vec<FileFingerprint>,
    tolerate_write_failure: bool,
) -> Result<(RefreshOutcome, Catalog)> {
    // Persist the SAME revision `evaluate` compares against — the tree revision.
    // `catalog.revision` is a different hash over a different subject (chunk ids
    // and their content hashes, not file paths and their bytes); storing it here
    // would compare two hash domains that can never be equal, making every
    // refresh rebuild from scratch. The catalog keeps its own revision inside
    // catalog.json as the catalog's identity.
    let current_revision = tree_revision(&fingerprints);

    let catalog = catalog::build(knowledge_root)?;
    let report = IngestReport {
        knowledge_root: knowledge_root.to_path_buf(),
        files: fingerprints.len(),
        chunks: catalog.chunks.len(),
        revision: catalog.revision.clone(),
        issues: catalog.issues.clone(),
    };

    let mut structural = Freshness {
        revision: current_revision,
        computed_at: Some(Utc::now()),
        stale: false,
        detail: None,
        unavailable: false,
    };

    structural_snapshot::publish(store, &catalog, &mut structural, tolerate_write_failure)?;
    #[cfg(test)]
    AFTER_PUBLISH.with(|hook| {
        if let Some(hook) = hook.borrow_mut().take() {
            hook();
        }
    });

    Ok((
        RefreshOutcome {
            rebuilt: true,
            structural,
            semantic: SemanticOutcome::skipped(semantic, STRUCTURAL_ONLY_REASON),
            report: Some(report),
        },
        catalog,
    ))
}

#[cfg(test)]
thread_local! {
    static AFTER_PUBLISH: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = std::cell::RefCell::new(None);
}

#[cfg(test)]
#[path = "refresh/tests_freshness.rs"]
mod tests_freshness;

#[cfg(test)]
#[path = "refresh/tests_states.rs"]
mod tests_states;

#[cfg(test)]
#[path = "refresh/tests_snapshot.rs"]
mod tests_snapshot;

#[cfg(test)]
#[path = "refresh/tests_schema.rs"]
mod tests_schema;

#[cfg(test)]
#[path = "refresh/tests_view_lifecycle.rs"]
mod tests_view_lifecycle;
