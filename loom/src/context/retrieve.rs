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
use crate::context::ingest::ingest;
use crate::context::pack::{pack, PackRequest};
use crate::context::rank::{rank, RankQuery, RankedCandidate};
use crate::context::refresh::{evaluate, refresh};
use crate::context::schema::{Channel, ContextPack};
use crate::context::store::ContextStore;
use crate::fs::knowledge::catalog::Catalog;
use crate::fs::knowledge::KnowledgeDir;
use crate::fs::work_dir::WorkDir;
use anyhow::{bail, Context, Result};
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
        }
    }
}

/// Resolve the knowledge root and the derived-artifact store together, so no two
/// callers can disagree about which tree or which cache they are working on.
pub(crate) fn resolve_roots(work_dir_hint: &Path) -> Result<(PathBuf, ContextStore)> {
    let work_dir = WorkDir::new(work_dir_hint)?;
    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?;
    let knowledge = KnowledgeDir::new(project_root);

    if !knowledge.exists() {
        bail!("Knowledge directory not found. Run 'loom knowledge init' to create it.");
    }

    let store = ContextStore::open(&work_dir)?;
    Ok((knowledge.root().to_path_buf(), store))
}

/// Bring the catalog current and return it. A read-only query must never die
/// because the cache is unwritable: a `refresh` failure is downgraded to a
/// warning and the catalog is built in memory instead.
fn resolve_catalog(store: &ContextStore, knowledge_root: &Path) -> Result<Catalog> {
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

/// Fail loudly when a caller demands a chunk id absent from the catalog.
///
/// Without this check the demand silently does nothing on a typo: the id never
/// matches, ranking proceeds as if it were never passed, and the caller gets a
/// successful pack with no signal that the requested chunk was never included.
pub(crate) fn reject_unknown_require_ids(catalog: &Catalog, require_id: &[String]) -> Result<()> {
    if require_id.is_empty() {
        return Ok(());
    }
    let known_ids: BTreeSet<&str> = catalog
        .chunks
        .iter()
        .map(|chunk| chunk.id.as_str())
        .collect();
    let unknown: Vec<&str> = require_id
        .iter()
        .map(String::as_str)
        .filter(|id| !known_ids.contains(id))
        .collect();
    if !unknown.is_empty() {
        let ids = unknown.join(", ");
        bail!(
            "Unknown --require-id value(s): {ids}. No chunk with that id exists in the \
             catalog; run 'loom knowledge context' without --require-id to see available ids."
        );
    }
    Ok(())
}

/// Rank the catalog once per requested channel, producing one candidate list
/// per channel.
///
/// The source graph exists, but [`rank`] still only accepts `&[KnowledgeChunk]`,
/// so the source channel contributes no candidates: ranking it over the
/// knowledge chunks would double-count them. Bridging the graph's nodes into the
/// ranker is a separate piece of work.
fn rank_channels(
    channels: &[Channel],
    rank_query: &RankQuery,
    catalog: &Catalog,
) -> Vec<Vec<RankedCandidate>> {
    channels
        .iter()
        .map(|channel| match channel {
            Channel::Knowledge => rank(rank_query, &catalog.chunks, Channel::Knowledge),
            Channel::Source => rank(rank_query, &[], Channel::Source),
        })
        .collect()
}

/// Retrieve a token-budgeted pack for `query`.
///
/// Deterministic and offline: no model call, no network access, no randomness.
/// The result is a pure function of the bytes on disk and `query`.
///
/// Returns `Err` when the knowledge directory is absent, the catalog cannot be
/// built, or `query.required_ids` names an id the catalog does not hold.
/// Callers on a hot path — signal generation, the prompt hook — must degrade
/// rather than propagate: log at `tracing::debug` and carry on with no pack.
pub fn retrieve_for_stage(query: &StageQuery, budget_tokens: usize) -> Result<ContextPack> {
    let (knowledge_root, store) = resolve_roots(&query.work_dir_hint)?;

    let catalog = resolve_catalog(&store, &knowledge_root)?;
    let state = evaluate(&store, &knowledge_root)?;

    reject_unknown_require_ids(&catalog, &query.required_ids)?;

    let rank_query = RankQuery {
        text: query.text.clone(),
        required_ids: query.required_ids.clone(),
        stage_dependency_ids: query.stage_dependency_ids.clone(),
    };

    let lists = rank_channels(&query.scope, &rank_query, &catalog);
    let fused = fuse(&lists);

    let request = PackRequest {
        query: query.text.clone(),
        scope: query.scope.clone(),
        budget_tokens,
        structural_freshness: state.structural,
        semantic_freshness: state.semantic,
    };
    Ok(pack(&request, &fused, &catalog.chunks))
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
