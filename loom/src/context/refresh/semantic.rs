//! The semantic (source-graph) half of [`super::refresh`], and the typed
//! answer to "which layer did this sync actually write?".
//!
//! Lives beside `source_graph` rather than inside it because this logic grows
//! with every new outcome the CLI must report, and `source_graph.rs` sits at
//! its file-size limit.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::source_graph::dirty_tree_reason;
use super::{reconcile_source_graph, SourceGraphOutcome, SourceGraphScope};
use crate::context::graph_store::GraphStore;
use crate::context::local_overlay::local_overlay_key;
use crate::context::schema::Freshness;
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;
use crate::git::runner::run_git_checked;

/// Prefix for every advisory line about the source graph, on any surface.
///
/// A NEW convention introduced here: this codebase has no shared advisory
/// marker (`advisory_codex_lane_preflight` prints bare text,
/// `check_for_uncommitted_changes` uses a red "x", `foreground.rs` uses a
/// literal "Warning: "). `loom knowledge sync` and
/// `commands::run::checks::advisory_source_graph_preflight` share THIS one so
/// the two surfaces agree.
pub const SOURCE_GRAPH_PREFIX: &str = "source graph: ";

/// What the semantic half of [`super::refresh`] actually did.
///
/// [`Freshness`] cannot answer this on its own: it carries only a revision, a
/// timestamp and a staleness reason, so it can say neither which layer was
/// written nor how big it is. Machine-readable state belongs in [`Self::layer`]
/// — never make a caller substring-match `freshness.detail` prose to learn
/// which layer it got.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticOutcome {
    /// Which layer this call ended up writing.
    pub layer: SemanticLayer,
    pub files_extracted: usize,
    pub nodes: usize,
    pub edges: usize,
    /// Freshness of the semantic layer after this call.
    pub freshness: Freshness,
}

/// Which layer the semantic reconcile ended up writing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SemanticLayer {
    /// Immutable base published for a clean revision.
    Base { revision: String },
    /// Working-tree overlay, because the base publish was refused.
    LocalOverlay {
        plan: String,
        stage: String,
        /// Why the base publish was refused.
        refusal: String,
    },
    /// Nothing ran: `--structural-only`, or an unresolvable project root.
    Skipped { reason: String },
}

impl SemanticOutcome {
    /// The semantic layer was not touched by this call. Carries the freshness
    /// the store already held through unchanged, with zero counts, because
    /// nothing was walked.
    pub fn skipped(freshness: Freshness, reason: impl Into<String>) -> Self {
        Self {
            layer: SemanticLayer::Skipped {
                reason: reason.into(),
            },
            files_extracted: 0,
            nodes: 0,
            edges: 0,
            freshness,
        }
    }

    /// Pair a layer with the counts [`reconcile_source_graph`] reported for it.
    fn from_source_graph(layer: SemanticLayer, outcome: SourceGraphOutcome) -> Self {
        Self {
            layer,
            files_extracted: outcome.files_extracted,
            nodes: outcome.nodes,
            edges: outcome.edges,
            freshness: outcome.freshness,
        }
    }
}

/// Best-effort semantic reconciliation folded into `refresh`'s result: any
/// failure below degrades to a stale [`Freshness`] naming it; an unresolvable
/// project root leaves `current` as is.
pub(super) fn reconcile_semantic_best_effort(
    store: &ContextStore,
    knowledge_root: &Path,
    current: Freshness,
) -> SemanticOutcome {
    let Some(project_root) = derive_project_root(knowledge_root) else {
        return SemanticOutcome::skipped(
            current,
            "project root could not be derived from the knowledge root",
        );
    };

    match try_reconcile_semantic(store, project_root) {
        Ok(outcome) => outcome,
        Err(error) => {
            let reason = format!("semantic reconciliation skipped: {error}");
            let freshness = Freshness {
                stale: true,
                detail: Some(reason.clone()),
                ..current
            };
            SemanticOutcome::skipped(freshness, reason)
        }
    }
}

/// Derive the project root from `knowledge_root` (`<root>/doc/loom/knowledge`),
/// refusing to guess when the layout does not match - an unvalidated ancestor
/// would point `WorkDir`, `GraphStore` and `rev-parse` at the wrong tree.
fn derive_project_root(knowledge_root: &Path) -> Option<&Path> {
    let candidate = knowledge_root.ancestors().nth(3)?;
    let derived = candidate.join("doc/loom/knowledge");
    let matches = match (derived.canonicalize(), knowledge_root.canonicalize()) {
        (Ok(derived), Ok(actual)) => derived == actual,
        _ => derived == knowledge_root,
    };
    matches.then_some(candidate)
}

/// The fallible half of [`reconcile_semantic_best_effort`]; any error becomes a
/// named staleness reason.
///
/// The dirty-tree check runs HERE, before a scope is chosen, so the fallback is
/// unambiguous rather than inferred from a degraded return. A base layer is
/// immutable and keyed to a revision, so a dirty tree can never publish one -
/// but publishing nothing leaves the user with no graph at all. Falling back to
/// the working-tree overlay at the address `local_overlay_key` owns means sync
/// always produces a graph, and always one that retrieval (which defaults to
/// that same local scope) can actually read.
fn try_reconcile_semantic(store: &ContextStore, project_root: &Path) -> Result<SemanticOutcome> {
    let work_dir = WorkDir::new(project_root)?;
    let graph_store = GraphStore::new(store.root(), work_dir.root());
    let revision = run_git_checked(&["rev-parse", "HEAD"], project_root)?;

    if let Some(refusal) = dirty_tree_reason(project_root, &revision) {
        let (plan, stage) = local_overlay_key(project_root);
        let scope = SourceGraphScope::Overlay {
            plan: plan.clone(),
            stage: stage.clone(),
        };
        let outcome = reconcile_source_graph(store, &graph_store, project_root, scope)?;
        return Ok(SemanticOutcome::from_source_graph(
            SemanticLayer::LocalOverlay {
                plan,
                stage,
                refusal,
            },
            outcome,
        ));
    }

    let scope = SourceGraphScope::Base {
        revision: revision.clone(),
    };
    let outcome = reconcile_source_graph(store, &graph_store, project_root, scope)?;
    Ok(SemanticOutcome::from_source_graph(
        SemanticLayer::Base { revision },
        outcome,
    ))
}
