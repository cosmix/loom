//! `loom knowledge context` — deterministic, offline context retrieval.
//!
//! Resolves a token-budgeted [`ContextPack`] for a query by ranking the
//! knowledge catalog (and, once it exists, the source graph), fusing the
//! per-channel rank lists, and packing them into the requested budget. There
//! is no model call and no network access anywhere in this path.

use crate::context::fuse::fuse;
use crate::context::ingest::ingest;
use crate::context::pack::{pack, PackRequest};
use crate::context::rank::{rank, RankQuery};
use crate::context::refresh::{evaluate, refresh};
use crate::context::store::ContextStore;
use crate::context::{
    Channel, Confidence, ContextItem, ContextPack, Freshness, OmissionSummary, SelectionReason,
};
use crate::fs::knowledge::catalog::Catalog;
use crate::fs::knowledge::KnowledgeDir;
use crate::fs::work_dir::WorkDir;
use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

/// Resolve the knowledge root and the derived-artifact store together, so the
/// three context commands can never disagree about which tree or cache they use.
pub(super) fn resolve() -> Result<(PathBuf, ContextStore)> {
    let work_dir = WorkDir::new(".")?;
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

/// Parse `--scope` into the channels it names.
fn parse_scope(scope: &str) -> Result<Vec<Channel>> {
    match scope.to_ascii_lowercase().as_str() {
        "knowledge" => Ok(vec![Channel::Knowledge]),
        "source" => Ok(vec![Channel::Source]),
        "all" => Ok(Channel::all().to_vec()),
        other => bail!("Invalid --scope '{other}': expected one of knowledge, source, all"),
    }
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

/// Fail loudly when `--require-id` names a chunk id absent from the catalog.
///
/// Without this check the flag silently does nothing on a typo: the id never
/// matches, ranking proceeds as if it were never passed, and the command
/// exits 0 with no signal that the requested chunk was never included.
fn reject_unknown_require_ids(catalog: &Catalog, require_id: &[String]) -> Result<()> {
    if require_id.is_empty() {
        return Ok(());
    }
    let known_ids: std::collections::BTreeSet<&str> = catalog
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
/// per channel. The source channel has no nodes until the source-graph stage
/// lands; ranking it over the knowledge chunks would double-count them, so it
/// always ranks an empty candidate slice.
fn rank_channels(
    channels: &[Channel],
    rank_query: &RankQuery,
    catalog: &Catalog,
) -> Vec<Vec<crate::context::rank::RankedCandidate>> {
    channels
        .iter()
        .map(|channel| match channel {
            Channel::Knowledge => rank(rank_query, &catalog.chunks, Channel::Knowledge),
            Channel::Source => rank(rank_query, &[], Channel::Source),
        })
        .collect()
}

/// Retrieve a token-budgeted context pack for `query`.
pub fn context(
    query: String,
    budget_tokens: usize,
    scope: String,
    require_id: Vec<String>,
    explain: bool,
    json: bool,
) -> Result<()> {
    let channels = parse_scope(&scope)?;
    let (knowledge_root, store) = resolve()?;

    let catalog = resolve_catalog(&store, &knowledge_root)?;
    let state = evaluate(&store, &knowledge_root)?;

    reject_unknown_require_ids(&catalog, &require_id)?;

    let rank_query = RankQuery {
        text: query.clone(),
        required_ids: require_id,
        stage_dependency_ids: Vec::new(),
    };

    let lists = rank_channels(&channels, &rank_query, &catalog);
    let fused = fuse(&lists);

    let request = PackRequest {
        query,
        scope: channels,
        budget_tokens,
        structural_freshness: state.structural,
        semantic_freshness: state.semantic,
    };
    let context_pack = pack(&request, &fused, &catalog.chunks);

    if json {
        println!("{}", serde_json::to_string_pretty(&context_pack)?);
    } else {
        print_human(&context_pack, explain);
    }

    Ok(())
}

fn print_human(pack: &ContextPack, explain: bool) {
    println!("{} {}", "Query:".bold(), pack.query);
    println!(
        "{} {}/{} tokens",
        "Budget:".bold(),
        pack.estimated_tokens,
        pack.budget_tokens
    );
    print_freshness_line("Structural", &pack.structural_freshness);
    print_freshness_line("Semantic", &pack.semantic_freshness);
    println!();

    if pack.items.is_empty() {
        println!("{}", "No items matched.".dimmed());
    } else {
        for item in &pack.items {
            print_item(item, explain);
        }
    }

    println!();
    print_omissions(&pack.omitted);
}

fn print_freshness_line(label: &str, freshness: &Freshness) {
    let marker = if freshness.stale {
        "stale".yellow()
    } else {
        "current".green()
    };
    match &freshness.detail {
        Some(detail) => println!("{label}: {marker} ({detail})"),
        None => println!("{label}: {marker}"),
    }
}

fn print_item(item: &ContextItem, explain: bool) {
    let anchor = if item.pointer.anchor.is_empty() {
        String::new()
    } else {
        format!("#{}", item.pointer.anchor)
    };
    println!(
        "  {:>6.2}  {}  {}{}  {}",
        item.score,
        item.id,
        item.pointer.path.display(),
        anchor,
        item.summary
    );
    if explain {
        let reasons = item
            .reasons
            .iter()
            .map(SelectionReason::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "          {reasons} | confidence: {} | state: {}",
            confidence_label(item.confidence),
            item.state
        );
    }
}

fn confidence_label(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::High => "high",
        Confidence::Medium => "medium",
        Confidence::Low => "low",
    }
}

fn print_omissions(omitted: &OmissionSummary) {
    println!(
        "{} {} omitted (weakest included score: {:.2}) — {}/{} items, {}/{} tokens",
        "Coverage:".cyan().bold(),
        omitted.omitted,
        omitted.weakest_included_score,
        omitted.coverage.included,
        omitted.coverage.candidates,
        omitted.coverage.included_tokens,
        omitted.coverage.candidate_tokens,
    );
}

#[cfg(test)]
#[path = "tests_context.rs"]
mod tests;
