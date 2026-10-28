//! Map command - read-only queries over the derived source graph.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::context::census::{self, CensusOptions};
use crate::context::freshness::GraphState;
use crate::context::graph_store::{GraphStore, ResolvedGraph};
use crate::context::refresh::{ensure_snapshot, SnapshotAction, SnapshotOutcome, SnapshotPolicy};
use crate::context::source_graph::{EdgeProvenance, SourceEdgeKind};
use crate::context::store::ContextStore;
use crate::context::view::ViewOrigin;
use crate::context::window::{read_window, SourceWindow, WindowError};
use crate::context::ResolutionStats;
use crate::fs::work_dir::WorkDir;
use crate::map::views::eval_edges;
use crate::map::views::snapshot::{identity_of, SnapshotIdentity};
use crate::map::views::timings::{timed, Timings};
use crate::map::views::{
    human_text, human_views, json_payload, json_views, parse_language, parse_provenance,
    ViewContext, ViewFilters, ViewOptions, ViewQuery,
};

const EDGE_KIND_NAMES: &str = "contains, imports, calls, references, implements, extends";

/// Exit code of `--window` for an id the graph does not name.
const EXIT_UNKNOWN_ID: i32 = 2;
/// Exit code of `--window` for a file edited since the snapshot.
const EXIT_CHANGED_SINCE_SNAPSHOT: i32 = 3;

/// Arguments for `loom map`. Lives here rather than in the CLI enum so the
/// command owns its own surface.
#[derive(Debug, clap::Parser)]
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
    /// Print direct callers (one hop over call edges) with their call sites
    #[arg(long, value_name = "SYMBOL")]
    pub callers: Option<String>,
    /// Print direct callees (one hop over call edges) with their call sites
    #[arg(long, value_name = "SYMBOL")]
    pub callees: Option<String>,
    /// Print direct references (one hop over reference edges) with their sites
    #[arg(long, value_name = "SYMBOL")]
    pub references: Option<String>,
    /// Print the exact source of a node id or a site id (path@start-end)
    #[arg(long, value_name = "ID")]
    pub window: Option<String>,
    /// Maximum lines --window prints
    #[arg(long, default_value_t = 60)]
    pub window_lines: usize,
    /// Print what the graph can and cannot see in this checkout and any --root
    #[arg(
        long,
        conflicts_with_all = [
            "outline", "find_all", "impact", "callers", "callees", "references", "window",
            "eval_edges",
        ]
    )]
    pub census: bool,
    /// Another git work tree to census (needs --census); repeatable
    #[arg(long, value_name = "DIR")]
    pub root: Vec<PathBuf>,
    /// Score a labelled corpus (or a directory of corpora) against the thresholds
    #[arg(long, value_name = "DIR")]
    pub eval_edges: Option<PathBuf>,
    /// Thresholds file for --eval-edges (default: the published thresholds)
    #[arg(long, value_name = "FILE", requires = "eval_edges")]
    pub thresholds: Option<PathBuf>,
    /// Maximum impact traversal depth
    #[arg(long, default_value_t = 3)]
    pub depth: usize,
    /// Comma-separated edge kinds included in impact traversal
    #[arg(
        long,
        value_name = "LIST",
        value_delimiter = ',',
        default_value = "calls,references,implements,extends",
        value_parser = parse_edge_kind
    )]
    pub kinds: Vec<SourceEdgeKind>,
    /// Comma-separated evidence classes impact may traverse (default: all)
    #[arg(
        long,
        value_name = "LIST",
        value_delimiter = ',',
        value_parser = parse_provenance
    )]
    pub evidence: Vec<EdgeProvenance>,
    /// Maximum rows per view; zero means unlimited
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
    /// Restrict hits to a project-relative path prefix (whole path components)
    #[arg(long, value_name = "PREFIX")]
    pub path: Option<String>,
    /// Restrict hits to one language (dialect id, such as rust or python)
    #[arg(long, value_name = "DIALECT", value_parser = parse_language)]
    pub lang: Option<String>,
    /// Minimum edge confidence included in impact traversal
    #[arg(long, default_value_t = 0.0)]
    pub min_confidence: f32,
    /// Emit one machine-readable JSON object
    #[arg(long)]
    pub json: bool,
    /// Print per-phase wall time and peak memory on stderr (and in --json)
    #[arg(long)]
    pub timings: bool,
}

/// What a request prints on stdout.
enum Output {
    Text(String),
    Json(Value),
}

/// The resolved graph a request is answered from.
struct Loaded {
    graph: ResolvedGraph,
    stats: ResolutionStats,
    snapshot: SnapshotIdentity,
}

/// Execute the map command in a checkout whether or not `loom init` has run.
pub fn execute(args: MapArgs) -> Result<()> {
    require_view(&args)?;
    let eval_exit = args
        .eval_edges
        .as_deref()
        .map(|dir| eval_edges_output(dir, &args))
        .transpose()?;
    if requested_views(&args) > usize::from(eval_exit.is_some()) {
        execute_graph_views(&args)?;
    }
    match eval_exit {
        Some(code) if code != 0 => std::process::exit(code),
        _ => Ok(()),
    }
}

/// Evaluate in memory, print the result, and return the exit code it maps to.
/// It never opens the `ContextStore` or takes a snapshot.
fn eval_edges_output(dir: &Path, args: &MapArgs) -> Result<i32> {
    let outcome = eval_edges::run(dir, args.thresholds.as_deref(), args.json)?;
    println!("{}", outcome.text);
    if args.json {
        eprintln!("{}", outcome.summary);
    }
    Ok(outcome.exit_code)
}

fn execute_graph_views(args: &MapArgs) -> Result<()> {
    let started = Instant::now();
    let work_dir = WorkDir::new(".")?;
    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?
        .to_path_buf();

    let mut timings = Timings::default();
    let output = if args.census {
        census_output(&work_dir, &project_root, args, &mut timings)?
    } else {
        let loaded = load_graph(&project_root, &work_dir, &mut timings)?;
        view_output(&loaded, &project_root, args, &mut timings)?
    };
    match output {
        Output::Text(text) => println!("{text}"),
        Output::Json(mut payload) => {
            if args.timings {
                payload["timings"] = timings.to_json(started.elapsed());
            }
            println!("{}", serde_json::to_string(&payload)?);
        }
    }
    if args.timings {
        eprintln!("{}", timings.summary(started.elapsed()));
    }
    Ok(())
}

fn require_view(args: &MapArgs) -> Result<()> {
    if !args.root.is_empty() && !args.census {
        bail!("--root only applies to --census");
    }
    if requested_views(args) == 0 {
        bail!(
            "loom map needs a view flag: --outline <PATH>, --find-all <SYMBOL>, \
             --impact <SYMBOL_OR_PATH>, --callers <SYMBOL>, --callees <SYMBOL>, \
             --references <SYMBOL>, --window <ID>, --census, or --eval-edges <DIR>"
        );
    }
    Ok(())
}

fn requested_views(args: &MapArgs) -> usize {
    [
        args.outline.is_some(),
        args.find_all.is_some(),
        args.impact.is_some(),
        args.callers.is_some(),
        args.callees.is_some(),
        args.references.is_some(),
        args.window.is_some(),
        args.census,
        args.eval_edges.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count()
}

/// Census the requested roots. The snapshot is taken only when this project is
/// one of them; a census of other checkouts alone leaves this project's `.loom/`
/// untouched.
fn census_output(
    work_dir: &WorkDir,
    project_root: &Path,
    args: &MapArgs,
    timings: &mut Timings,
) -> Result<Output> {
    let options = CensusOptions {
        roots: args.root.clone(),
    };
    let loaded = options
        .covers(project_root)
        .then(|| load_graph(project_root, work_dir, timings))
        .transpose()?;
    let current = loaded.as_ref().map(|loaded| (project_root, &loaded.graph));
    let report = timed(&mut timings.query, || census::run(&options, current))?;
    Ok(if args.json {
        Output::Json(serde_json::to_value(&report)?)
    } else {
        Output::Text(report.to_string().trim_end().to_string())
    })
}

fn view_output(
    loaded: &Loaded,
    project_root: &Path,
    args: &MapArgs,
    timings: &mut Timings,
) -> Result<Output> {
    let query = timed(&mut timings.query, || {
        build_query(loaded, project_root, args)
    })?;
    let ctx = ViewContext {
        graph: &loaded.graph,
        project_root,
        stats: &loaded.stats,
        snapshot: &loaded.snapshot,
    };
    if args.json {
        let views = timed(&mut timings.query, || json_views(&ctx, &query));
        Ok(Output::Json(timed(&mut timings.render, || {
            json_payload(&ctx, views)
        })))
    } else {
        let views = timed(&mut timings.query, || human_views(&ctx, &query));
        Ok(Output::Text(timed(&mut timings.render, || {
            human_text(&ctx, views)
        })))
    }
}

fn build_query(loaded: &Loaded, project_root: &Path, args: &MapArgs) -> Result<ViewQuery> {
    let window = args
        .window
        .as_deref()
        .map(|id| read_source_window(loaded, project_root, id, args.window_lines))
        .transpose()?;
    Ok(ViewQuery {
        outline: args.outline.clone(),
        find_all: args.find_all.clone(),
        impact: args.impact.clone(),
        callers: args.callers.clone(),
        callees: args.callees.clone(),
        references: args.references.clone(),
        window,
        options: ViewOptions {
            depth: args.depth,
            kinds: args.kinds.clone(),
            limit: args.limit,
            min_confidence: args.min_confidence,
            provenances: args.evidence.clone(),
            filters: ViewFilters::new(args.path.clone(), args.lang.clone()),
        },
    })
}

/// Read the window, or leave the process with the exit code its failure
/// names. No error path prints file bytes.
fn read_source_window(
    loaded: &Loaded,
    project_root: &Path,
    id: &str,
    max_lines: usize,
) -> Result<SourceWindow> {
    match read_window(&loaded.graph, project_root, id, max_lines) {
        Ok(window) => Ok(window),
        Err(error @ WindowError::UnknownId(_)) => {
            eprintln!("{error}");
            std::process::exit(EXIT_UNKNOWN_ID);
        }
        Err(error @ WindowError::ChangedSinceSnapshot { .. }) => {
            eprintln!("{error}");
            std::process::exit(EXIT_CHANGED_SINCE_SNAPSHOT);
        }
        Err(error @ WindowError::Unreadable { .. }) => bail!("{error}"),
    }
}

/// Ensure the local snapshot, then read the resolved view it selected.
///
/// The cache directory is not created up front: every cache write creates its own parent, and
/// a write the sandbox denies falls back to memory, so a read-only `.loom` still answers.
fn load_graph(project_root: &Path, work_dir: &WorkDir, timings: &mut Timings) -> Result<Loaded> {
    let store = ContextStore::open(work_dir)?;
    let graph_store = GraphStore::new(store.root(), work_dir.root());
    let snapshot = timed(&mut timings.snapshot, || {
        ensure_snapshot(
            &store,
            &graph_store,
            project_root,
            SnapshotPolicy::LocalCurrent,
        )
    });
    report_snapshot(&snapshot);
    let view_started = Instant::now();
    let overlay = snapshot
        .overlay
        .as_ref()
        .map(|(plan, stage)| (plan.as_str(), stage.as_str()));
    let view = graph_store.view(&snapshot.revision, overlay)?;
    let identity = identity_of(&graph_store, &snapshot, &view);
    let view_elapsed = view_started.elapsed();
    timings.view += view_elapsed;
    match view.origin {
        ViewOrigin::Materialized => timings.load += view_elapsed,
        ViewOrigin::Built => timings.resolve += view_elapsed,
    }
    Ok(Loaded {
        graph: view.graph,
        stats: view.stats,
        snapshot: identity,
    })
}

/// One stderr line about the snapshot, unless it was reused untouched.
fn report_snapshot(snapshot: &SnapshotOutcome) {
    if snapshot.state() == GraphState::NeverBuilt {
        eprintln!("source graph never built: {}", snapshot.reason);
    } else if snapshot.action != SnapshotAction::Reused {
        eprintln!("{}", snapshot.describe());
    }
}

fn parse_edge_kind(value: &str) -> std::result::Result<SourceEdgeKind, String> {
    [
        SourceEdgeKind::Contains,
        SourceEdgeKind::Imports,
        SourceEdgeKind::Calls,
        SourceEdgeKind::References,
        SourceEdgeKind::Implements,
        SourceEdgeKind::Extends,
    ]
    .into_iter()
    .find(|kind| kind.as_str() == value)
    .ok_or_else(|| format!("unknown edge kind '{value}'; valid kinds: {EDGE_KIND_NAMES}"))
}

#[cfg(test)]
#[path = "tests_map.rs"]
mod tests;
