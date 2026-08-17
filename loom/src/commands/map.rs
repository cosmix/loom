//! Map command - analyze codebase structure and write to knowledge files,
//! or query the derived source graph read-only.

use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::context::graph_store::{GraphStore, ResolvedGraph};
use crate::context::local_overlay::local_overlay_key;
use crate::context::refresh::{reconcile_source_graph, SourceGraphScope};
use crate::context::store::ContextStore;
use crate::context::{resolve_graph, ResolutionStats};
use crate::fs::knowledge::KnowledgeDir;
use crate::fs::work_dir::WorkDir;
use crate::map::{analyze_codebase, knowledge_sync};

/// Arguments for `loom map`. Lives here rather than in the CLI enum so the
/// command owns its own surface.
#[derive(Debug, clap::Args)]
pub struct MapArgs {
    /// Deep analysis (more thorough, slower)
    #[arg(short, long)]
    pub deep: bool,
    /// Focus on specific area (e.g., "auth", "api", "db")
    #[arg(short, long)]
    pub focus: Option<String>,
    /// Overwrite existing knowledge sections (default: only add missing ones)
    #[arg(long)]
    pub overwrite: bool,
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

/// Execute the map command: either a read-only source-graph view, or the
/// original knowledge-file analysis.
pub fn execute(args: MapArgs) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?
        .to_path_buf();

    if args.outline.is_some() || args.find_all.is_some() || args.impact.is_some() {
        return run_views(&project_root, &work_dir, &args);
    }

    crate::utils::print_logo_header("Codebase Map");
    println!(
        "{} Mapping codebase{}...",
        "→".cyan().bold(),
        if args.deep { " (deep mode)" } else { "" }
    );

    // Run analysis
    let result = analyze_codebase(&project_root, args.deep, args.focus.as_deref())?;

    // Initialize knowledge if needed
    let knowledge = KnowledgeDir::new(&project_root);
    if !knowledge.exists() {
        knowledge.initialize()?;
    }

    // Write results to knowledge files
    let written = knowledge_sync::write_analysis(&knowledge, &result, args.overwrite)?;
    for file in &written {
        println!("  {} {}", "→".cyan(), file.filename());
    }

    println!("\n{} Codebase mapped successfully!", "✓".green().bold());
    println!("  Run 'loom knowledge show' to view results.");

    Ok(())
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
/// its overlay. Works even when the graph has never been built before.
fn load_graph(project_root: &Path, work_dir: &WorkDir) -> Result<(ResolvedGraph, ResolutionStats)> {
    let store = ContextStore::open(work_dir)?;
    store.ensure()?;
    let graph_store = GraphStore::new(store.root(), work_dir.root());
    let (plan, stage) = local_overlay_key(project_root);
    let outcome = reconcile_source_graph(
        &store,
        &graph_store,
        project_root,
        SourceGraphScope::Overlay {
            plan: plan.clone(),
            stage: stage.clone(),
        },
    )?;
    let mut graph = graph_store.resolved(
        &outcome.freshness.revision,
        Some((plan.as_str(), stage.as_str())),
    )?;
    let stats = resolve_graph(&mut graph);
    Ok((graph, stats))
}
