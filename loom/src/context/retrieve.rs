//! The one retrieval entry point.
//!
//! Every caller that wants context for a stage — the `loom knowledge context`
//! command, signal generation, the prompt hook — goes through
//! [`retrieve_for_stage`]. Having a single pipeline is what makes a brief
//! rendered at spawn time and a brief pulled by hand comparable: same catalog,
//! same freshness evaluation, same ranking, same packer.
//!
//! Retrieval is deterministic and offline. There is no model call, no network
//! access and no randomness here; a [`ContextPack`] is a pure function of the
//! bytes on disk and the [`StageQuery`].

use crate::context::fuse::fuse;
use crate::context::graph_store::{GraphStore, ResolvedGraph};
use crate::context::ingest::ingest;
use crate::context::local_overlay::OverlayScope;
use crate::context::pack::{pack, PackRequest};
use crate::context::rank::{rank, RankQuery, RankedCandidate};
use crate::context::rank_source::rank_source;
use crate::context::refresh::{evaluate, refresh};
use crate::context::schema::{Channel, ContextPack, Freshness};
use crate::context::store::{ContextStore, StoreState};
use crate::fs::knowledge::catalog::Catalog;
use crate::fs::knowledge::KnowledgeDir;
use crate::fs::work_dir::WorkDir;
use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Everything a stage-scoped retrieval needs to be reproducible.
#[derive(Debug, Clone)]
pub struct StageQuery {
    /// Directory to start the `.work/` search from ([`WorkDir::new`] semantics).
    pub work_dir_hint: PathBuf,
    /// Free text the ranker scores against.
    pub text: String,
    /// Chunk ids the caller demands verbatim.
    pub required_ids: Vec<String>,
    /// Chunk ids referenced by stages this query's stage depends on.
    pub stage_dependency_ids: Vec<String>,
    /// Channels to rank. Use `Channel::all().to_vec()` unless narrowing.
    pub scope: Vec<Channel>,
    /// Which source-graph overlay to read on top of the base layer.
    ///
    /// Defaults to [`OverlayScope::Local`] in [`StageQuery::new`], not to a
    /// base-only read: a query that names no stage means "the tree in front of
    /// me", and on a dirty tree there is no base layer for HEAD — the
    /// working-tree overlay is the only thing describing it.
    pub overlay: OverlayScope,
}

impl StageQuery {
    /// A whole-scope query rooted at `work_dir_hint`.
    pub fn new(work_dir_hint: impl Into<PathBuf>, text: impl Into<String>) -> Self {
        StageQuery {
            work_dir_hint: work_dir_hint.into(),
            text: text.into(),
            required_ids: Vec::new(),
            stage_dependency_ids: Vec::new(),
            scope: Channel::all().to_vec(),
            overlay: OverlayScope::Local,
        }
    }
}

/// Message for a caller that cannot proceed without a knowledge tree.
const NO_KNOWLEDGE_DIR: &str = "Knowledge directory not found. Run 'loom init' to create it.";

/// Structural freshness detail for a project that has no knowledge tree.
///
/// Never built, and therefore stale — not "current" over an empty catalog. A
/// derived layer that reports itself fresh when nothing was ever derived is the
/// failure `doc/loom/knowledge/mistakes/store-without-consumer.md` records: the
/// pack looks authoritative and its emptiness reads as "there is nothing to
/// say" rather than "nothing was ever built".
const NO_KNOWLEDGE_TREE_DETAIL: &str =
    "no knowledge directory; the knowledge channel is unavailable here";

/// Semantic freshness detail when no source graph has ever been built here.
const NO_SOURCE_GRAPH_DETAIL: &str = "source graph not built; run 'loom map' to build one";

/// Resolve the knowledge root and the derived-artifact store together, so no two
/// callers can disagree about which tree or which cache they are working on.
///
/// The knowledge root is `None` when `doc/loom/knowledge/` does not exist. That
/// is a degraded retrieval, not a failure: the source channel ranks over the
/// resolved graph, which lives in the context cache and needs no catalog at
/// all, so a checkout with a mapped source graph and no knowledge tree still
/// has something to answer with. Callers that genuinely require a tree use
/// [`resolve_roots`].
pub(crate) fn resolve_roots_optional(
    work_dir_hint: &Path,
) -> Result<(Option<PathBuf>, ContextStore)> {
    let work_dir = WorkDir::new(work_dir_hint)?;
    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?;
    let knowledge = KnowledgeDir::new(project_root);
    let knowledge_root = knowledge.exists().then(|| knowledge.root().to_path_buf());

    let store = ContextStore::open(&work_dir)?;
    Ok((knowledge_root, store))
}

/// [`resolve_roots_optional`] for a caller that cannot work without a knowledge
/// tree — the `loom knowledge` commands, which read and write that tree.
pub(crate) fn resolve_roots(work_dir_hint: &Path) -> Result<(PathBuf, ContextStore)> {
    let (knowledge_root, store) = resolve_roots_optional(work_dir_hint)?;
    let knowledge_root = knowledge_root.ok_or_else(|| anyhow!(NO_KNOWLEDGE_DIR))?;
    Ok((knowledge_root, store))
}

/// Bring the catalog current and return it. A read-only query must never die
/// because the cache is unwritable: a `refresh` failure is downgraded to a
/// warning and the catalog is built in memory instead.
///
/// With no knowledge tree there is nothing to ingest, so the catalog is empty
/// and the knowledge channel contributes no candidates. [`evaluate_state`] is
/// what keeps that honest.
fn resolve_catalog(store: &ContextStore, knowledge_root: Option<&Path>) -> Result<Catalog> {
    let Some(knowledge_root) = knowledge_root else {
        return Ok(Catalog {
            revision: String::new(),
            chunks: Vec::new(),
            issues: Vec::new(),
        });
    };

    if let Err(error) = refresh(store, knowledge_root, true) {
        eprintln!(
            "warning: failed to refresh the context cache ({error}); using an in-memory catalog for this query"
        );
        let (catalog, _report) = ingest(knowledge_root)?;
        return Ok(catalog);
    }

    match store.load_catalog()? {
        Some(catalog) => Ok(catalog),
        None => {
            let (catalog, _report) = ingest(knowledge_root)?;
            Ok(catalog)
        }
    }
}

/// Freshness of both derived layers for this query.
///
/// With a knowledge tree this is [`evaluate`] verbatim. Without one there is
/// nothing to fingerprint, so the structural layer reports itself never built
/// while the semantic layer keeps whatever the store records — the source graph
/// is not derived from the knowledge tree, and its revision is what
/// [`load_resolved_graph`] reads the base layer by.
fn evaluate_state(store: &ContextStore, knowledge_root: Option<&Path>) -> Result<StoreState> {
    let Some(knowledge_root) = knowledge_root else {
        let stored = store.load_state()?;
        let semantic = if stored.semantic.revision.is_empty() {
            Freshness::never_built(NO_SOURCE_GRAPH_DETAIL)
        } else {
            stored.semantic
        };
        return Ok(StoreState {
            structural: Freshness::never_built(NO_KNOWLEDGE_TREE_DETAIL),
            semantic,
            catalog_revision: stored.catalog_revision,
        });
    };

    evaluate(store, knowledge_root)
}

/// Fail loudly when a caller demands a chunk or source-node id absent from
/// this query's scope.
///
/// Without this check the demand silently does nothing on a typo: the id
/// never matches, ranking proceeds as if it were never passed, and the caller
/// gets a successful pack with no signal that the requested id was never
/// included.
///
/// `graph` is consulted only when the caller passes `Some` — which
/// `retrieve_for_stage` does only when `Channel::Source` is in the query's
/// scope. A source-node id accepted while the source channel is out of scope
/// would pass this check and then never be ranked (`rank_source` is the only
/// reader of `graph`, and it never runs for a channel outside `scope`): the
/// exact silent no-op this function exists to prevent, just moved one step
/// later.
pub(crate) fn reject_unknown_require_ids(
    catalog: &Catalog,
    graph: Option<&ResolvedGraph>,
    require_id: &[String],
) -> Result<()> {
    if require_id.is_empty() {
        return Ok(());
    }
    let mut known_ids: BTreeSet<&str> = catalog
        .chunks
        .iter()
        .map(|chunk| chunk.id.as_str())
        .collect();
    if let Some(graph) = graph {
        known_ids.extend(graph.nodes().map(|node| node.id.as_str()));
    }
    let unknown: Vec<&str> = require_id
        .iter()
        .map(String::as_str)
        .filter(|id| !known_ids.contains(id))
        .collect();
    if !unknown.is_empty() {
        let ids = unknown.join(", ");
        bail!(
            "Unknown --require-id value(s): {ids}. No chunk or source node with that id \
             exists in scope; run 'loom knowledge context' without --require-id to see \
             available ids."
        );
    }
    Ok(())
}

/// Rank the catalog once per requested channel, producing one candidate list
/// per channel.
///
/// Each channel is ranked over its own corpus: the knowledge channel over the
/// catalog's chunks via [`rank`], the source channel over `graph`'s nodes via
/// [`rank_source`]. `graph` is `None` when the source graph was never built or
/// could not be read for this query — that is a degraded pack, not an error,
/// so the source channel simply contributes no candidates rather than failing
/// the whole retrieval.
fn rank_channels(
    channels: &[Channel],
    rank_query: &RankQuery,
    catalog: &Catalog,
    graph: Option<&ResolvedGraph>,
) -> Vec<Vec<RankedCandidate>> {
    channels
        .iter()
        .map(|channel| match channel {
            Channel::Knowledge => rank(rank_query, &catalog.chunks, Channel::Knowledge),
            Channel::Source => graph
                .map(|g| rank_source(rank_query, g))
                .unwrap_or_default(),
        })
        .collect()
}

/// Load the resolved source graph for `query`'s overlay, degrading to `None`
/// on any error.
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
/// Retrieval itself never builds or refreshes this graph: [`resolve_catalog`]
/// calls [`refresh`] with `structural_only = true`, which skips the semantic
/// reconcile on every call this pipeline makes. So this function only ever
/// reads what a prior `loom map` run, or a merge, already wrote for
/// `semantic_revision`. **Real, currently-reachable degraded mode:** on a
/// checkout where `loom map` has never run and no merge has published a base
/// for `semantic_revision`, neither layer exists — `resolved` still returns
/// an empty graph rather than an error, so this degrades silently, and the
/// source channel contributes nothing to the pack with no signal to the
/// caller that anything is missing.
fn load_resolved_graph(
    work_dir_hint: &Path,
    store: &ContextStore,
    semantic_revision: &str,
    overlay: &OverlayScope,
) -> Option<ResolvedGraph> {
    let work_dir = WorkDir::new(work_dir_hint).ok()?;
    let project_root = work_dir.project_root()?;
    let (plan, stage) = overlay.resolve(project_root);
    let graph_store = GraphStore::new(store.root(), work_dir.root());
    graph_store
        .resolved(semantic_revision, Some((&plan, &stage)))
        .ok()
}

/// Retrieve a token-budgeted pack for `query`.
///
/// Deterministic and offline: no model call, no network access, no randomness.
/// The result is a pure function of the bytes on disk and `query`.
///
/// Returns `Err` when the catalog cannot be built or `query.required_ids` names
/// an id the catalog does not hold. An ABSENT knowledge directory is not an
/// error: retrieval runs source-only, over the resolved graph alone, and the
/// pack says the structural layer was never built. Callers on a hot path —
/// signal generation, the prompt hook — must degrade rather than propagate: log
/// at `tracing::debug` and carry on with no pack.
pub fn retrieve_for_stage(query: &StageQuery, budget_tokens: usize) -> Result<ContextPack> {
    let (knowledge_root, store) = resolve_roots_optional(&query.work_dir_hint)?;

    let catalog = resolve_catalog(&store, knowledge_root.as_deref())?;
    let state = evaluate_state(&store, knowledge_root.as_deref())?;

    // The source graph is keyed by the semantic revision, not the structural
    // one — they are different hash domains over different subjects (see the
    // comment at `refresh.rs:177-182`), and the graph is semantic-derived data.
    let graph = load_resolved_graph(
        &query.work_dir_hint,
        &store,
        &state.semantic.revision,
        &query.overlay,
    );

    // A required id can only ever name a source node when the source channel
    // is actually in scope; see `reject_unknown_require_ids`'s doc comment.
    let source_graph_for_require_ids = query
        .scope
        .contains(&Channel::Source)
        .then_some(graph.as_ref())
        .flatten();
    reject_unknown_require_ids(&catalog, source_graph_for_require_ids, &query.required_ids)?;

    let rank_query = RankQuery {
        text: query.text.clone(),
        required_ids: query.required_ids.clone(),
        stage_dependency_ids: query.stage_dependency_ids.clone(),
    };

    let lists = rank_channels(&query.scope, &rank_query, &catalog, graph.as_ref());
    let fused = fuse(&lists);

    let request = PackRequest {
        query: query.text.clone(),
        scope: query.scope.clone(),
        budget_tokens,
        structural_freshness: state.structural,
        semantic_freshness: state.semantic,
    };
    Ok(pack(&request, &fused, &catalog.chunks, graph.as_ref()))
}

/// Identity of the derived-data generation a pack was built from.
///
/// Two packs share an epoch exactly when both derived layers were built from
/// the same bytes, which is what makes "already delivered" a safe thing to say.
/// Rebuilding either layer changes the epoch and re-opens delivery. A layer that
/// was never built contributes its empty revision string; the epoch stays well
/// defined.
pub fn context_epoch(pack: &ContextPack) -> String {
    let mut hasher = Sha256::new();
    hasher.update(pack.structural_freshness.revision.as_bytes());
    hasher.update(b"\n");
    hasher.update(pack.semantic_freshness.revision.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}
