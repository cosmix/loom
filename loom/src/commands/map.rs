//! Map command - read-only queries over the derived source graph.

use std::path::Path;

use anyhow::{bail, Context, Result};
use colored::Colorize;

use crate::context::graph_store::{GraphStore, ResolvedGraph};
use crate::context::local_overlay::local_overlay_key;
use crate::context::refresh::{reconcile_source_graph, SourceGraphScope};
use crate::context::store::ContextStore;
use crate::context::{resolve_graph, ResolutionStats};
use crate::fs::work_dir::WorkDir;

/// Arguments for `loom map`. Lives here rather than in the CLI enum so the
/// command owns its own surface.
#[derive(Debug, clap::Args)]
pub struct MapArgs {
    /// Print the indexed symbols of one file, in source order
    #[arg(long, value_name = "PATH")]
    pub outline: Option<String>,
    /// Print every indexed node whose name matches
    #[arg(long, value_name = "SYMBOL")]
    pub find_all: Option<String>,
    /// Print what reaches a symbol or file, with path confidence
    #[arg(long, value_name = "SYMBOL_OR_PATH")]
    pub impact: Option<String>,
}

/// Execute the map command: a read-only view over the derived source graph.
pub fn execute(args: MapArgs) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?
        .to_path_buf();

    if args.outline.is_none() && args.find_all.is_none() && args.impact.is_none() {
        bail!(
            "loom map needs a view flag: --outline <PATH>, --find-all <SYMBOL>, or --impact <SYMBOL_OR_PATH>"
        );
    }

    run_views(&project_root, &work_dir, &args)
}

/// Run whichever of `--outline` / `--find-all` / `--impact` were given, in
/// that order, each under its own heading if more than one is present.
fn run_views(project_root: &Path, work_dir: &WorkDir, args: &MapArgs) -> Result<()> {
    let (graph, stats) = load_graph(project_root, work_dir)?;

    let requested = [
        args.outline.is_some(),
        args.find_all.is_some(),
        args.impact.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();

    if let Some(path) = &args.outline {
        if requested > 1 {
            println!("{}", "== outline ==".bold());
        }
        crate::map::views::outline(&graph, project_root, path);
    }

    if let Some(symbol) = &args.find_all {
        if requested > 1 {
            println!("{}", "== find-all ==".bold());
        }
        crate::map::views::find_all(&graph, symbol);
    }

    if let Some(target) = &args.impact {
        if requested > 1 {
            println!("{}", "== impact ==".bold());
        }
        crate::map::views::impact(&graph, project_root, target, &stats);
    }

    Ok(())
}

/// Build (or reconcile) the working-tree source graph and resolve it against
/// its overlay. Works even when the graph has never been built before, and
/// degrades rather than erroring when the cache can't be written: `loom map`
/// is a read-only query, so a read-only `.work` (a sandboxed stage worktree,
/// for instance) must never stop it from reading whatever layers already sit
/// on disk. A write failure prints one warning and falls back to the
/// revision the store already recorded; if that leaves no readable layer at
/// all, a second warning says so.
fn load_graph(project_root: &Path, work_dir: &WorkDir) -> Result<(ResolvedGraph, ResolutionStats)> {
    let store = ContextStore::open(work_dir)?;
    if let Err(error) = store.ensure() {
        eprintln!(
            "warning: could not prepare the source graph cache ({error}); reading the layers already on disk"
        );
    }
    let graph_store = GraphStore::new(store.root(), work_dir.root());
    let (plan, stage) = local_overlay_key(project_root);
    let revision = match reconcile_source_graph(
        &store,
        &graph_store,
        project_root,
        SourceGraphScope::Overlay {
            plan: plan.clone(),
            stage: stage.clone(),
        },
    ) {
        Ok(outcome) => outcome.freshness.revision,
        Err(error) => {
            eprintln!(
                "warning: could not refresh the working-tree source graph ({error}); reading the layers already on disk"
            );
            store
                .load_state()
                .map(|state| state.semantic.revision)
                .unwrap_or_default()
        }
    };
    let mut graph = graph_store.resolved(&revision, Some((plan.as_str(), stage.as_str())))?;
    if graph.files.is_empty() {
        eprintln!(
            "warning: no readable source-graph layer exists for revision {revision}; run `loom knowledge sync` (or `loom init`) to publish one"
        );
    }
    let stats = resolve_graph(&mut graph);
    Ok((graph, stats))
}
