//! `--impact`: what reaches a symbol or file, with per-edge confidence and
//! provenance. Reachability is never overclaimed: an empty result names what
//! was left untraversed.

use std::path::Path;

use colored::Colorize;
use serde_json::{json, Value};

use crate::context::graph_store::ResolvedGraph;
use crate::context::resolve::{impact_with, ImpactHit, ImpactOptions};
use crate::context::source_graph::SourceEdgeKind;
use crate::context::{CoverageReport, ResolutionStats};

use super::filters::{cap, language_of, Removed};
use super::json::safe;
use super::matching::{match_note, resolve_starts, ResolvedStarts};
use super::ViewOptions;

const ALL_EDGE_KINDS: [SourceEdgeKind; 6] = [
    SourceEdgeKind::Contains,
    SourceEdgeKind::Imports,
    SourceEdgeKind::Calls,
    SourceEdgeKind::References,
    SourceEdgeKind::Implements,
    SourceEdgeKind::Extends,
];

struct StartHits {
    id: String,
    hits: Vec<ImpactHit>,
    suppressed: usize,
    filtered: bool,
}

struct Impact {
    resolved: ResolvedStarts,
    per_start: Vec<StartHits>,
    removed: Removed,
}

impl Impact {
    fn suppressed(&self) -> usize {
        self.per_start.iter().map(|start| start.suppressed).sum()
    }
}

/// Walk every shown start. Edge filters (`--kinds`, `--evidence`,
/// `--min-confidence`) apply during traversal; `--path`, `--lang` and
/// `--limit` apply to the displayed hits afterwards.
fn collect(graph: &ResolvedGraph, project_root: &Path, arg: &str, opts: &ViewOptions) -> Impact {
    let resolved = resolve_starts(graph, project_root, arg);
    let mut removed = Removed::default();
    let mut per_start = Vec::new();
    for id in resolved.shown() {
        let result = impact_with(
            graph,
            id,
            &ImpactOptions {
                max_depth: opts.depth,
                kinds: opts.kinds.clone(),
                limit: 0,
                // A superset of the component-aware filter below; it keeps the
                // traversal's own `filtered_out` count meaningful.
                path_prefix: opts.filters.path_prefix().map(str::to_string),
                min_confidence: opts.min_confidence,
                provenances: opts.provenances.clone(),
                ..Default::default()
            },
        );
        let removed_before = removed.path + removed.lang;
        removed.path += result.filtered_out;
        let mut hits: Vec<ImpactHit> = result
            .hits
            .into_iter()
            .filter(|hit| {
                let path = hit.path.to_string_lossy();
                let language = language_of(graph, &path);
                opts.filters.admit(&path, language, &mut removed)
            })
            .collect();
        let suppressed = cap(&mut hits, opts.limit);
        per_start.push(StartHits {
            id: id.clone(),
            hits,
            suppressed,
            filtered: removed.path + removed.lang > removed_before,
        });
    }
    Impact {
        resolved,
        per_start,
        removed,
    }
}

/// Render what reaches a symbol or file, with per-edge confidence and provenance.
pub fn render_impact(
    graph: &ResolvedGraph,
    project_root: &Path,
    arg: &str,
    stats: &ResolutionStats,
    opts: &ViewOptions,
) -> String {
    let impact = collect(graph, project_root, arg, opts);

    // Flatten only for display - resolution above already ran on the raw `arg`.
    let safe_arg = safe(arg);
    if impact.resolved.starts.is_empty() {
        return format!("no indexed node named {safe_arg}");
    }

    let mut lines = Vec::new();
    lines.extend(match_note(&impact.resolved, &safe_arg));
    for start in &impact.per_start {
        lines.push(render_start(graph, stats, opts, &impact.resolved, start));
    }
    if impact.resolved.suppressed() > 0 {
        lines.push(format!(
            "  ... {} more suppressed",
            impact.resolved.suppressed()
        ));
    }
    lines.extend(opts.filters.note(impact.removed));
    lines.join("\n")
}

/// One start's heading and its reverse-reachability rows.
///
/// `start.id` and every `hit.id` are source-graph node ids (a file path or a
/// `scope::symbol` path lifted from the parsed source), so both are
/// flattened for display.
fn render_start(
    graph: &ResolvedGraph,
    stats: &ResolutionStats,
    opts: &ViewOptions,
    resolved: &ResolvedStarts,
    start: &StartHits,
) -> String {
    let heading = format!(
        "{} Impact of {} (depth <= {}, kinds: {}, reverse edges){}",
        "→".cyan().bold(),
        safe(&start.id),
        opts.depth,
        impact_kinds_label(&opts.kinds),
        resolved.mode.label(),
    );
    if start.hits.is_empty() {
        return format!(
            "{heading}\n  {}",
            untraversed_summary(graph, stats, start.filtered)
        );
    }
    let mut lines = vec![heading];
    lines.extend(start.hits.iter().map(hit_row));
    if start.suppressed > 0 {
        lines.push(format!(
            "  ... {} more suppressed (raise --limit)",
            start.suppressed
        ));
    }
    lines.join("\n")
}

fn hit_row(hit: &ImpactHit) -> String {
    let via = if hit.via_candidates {
        "  via-candidates"
    } else {
        ""
    };
    format!(
        "  d{}  {:.2}  {}  {}  {}{via}",
        hit.depth,
        hit.min_confidence,
        hit.weakest_provenance,
        hit.weakest_kind,
        safe(&hit.id)
    )
}

pub fn impact_json(
    graph: &ResolvedGraph,
    project_root: &Path,
    arg: &str,
    opts: &ViewOptions,
) -> Value {
    let impact = collect(graph, project_root, arg, opts);
    let hits: Vec<Value> = impact
        .per_start
        .iter()
        .flat_map(|start| start.hits.iter().map(impact_hit_json))
        .collect();
    json!({
        "match": impact.resolved.mode.as_str(),
        "start_ids": impact.per_start.iter().map(|start| &start.id).collect::<Vec<_>>(),
        "depth": opts.depth,
        "kinds": effective_impact_kinds(&opts.kinds).iter().map(|kind| kind.as_str()).collect::<Vec<_>>(),
        "hits": hits,
        "suppressed_starts": impact.resolved.suppressed(),
        "suppressed": impact.suppressed(),
        "filters": opts.filters.to_json(impact.removed, "display"),
    })
}

fn impact_hit_json(hit: &ImpactHit) -> Value {
    json!({
        "id": hit.id,
        "kind": hit.kind.as_str(),
        "path": hit.path.to_string_lossy(),
        "depth": hit.depth,
        "min_confidence": hit.min_confidence,
        "weakest_provenance": hit.weakest_provenance.as_str(),
        "weakest_kind": hit.weakest_kind.as_str(),
        "via_candidates": hit.via_candidates,
    })
}

fn impact_kinds_label(kinds: &[SourceEdgeKind]) -> String {
    let kinds = effective_impact_kinds(kinds);
    if ALL_EDGE_KINDS.iter().all(|kind| kinds.contains(kind)) {
        "all".to_string()
    } else {
        kinds
            .iter()
            .map(SourceEdgeKind::as_str)
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn effective_impact_kinds(kinds: &[SourceEdgeKind]) -> Vec<SourceEdgeKind> {
    if !kinds.is_empty() {
        return kinds.to_vec();
    }
    let options = ImpactOptions {
        kinds: kinds.to_vec(),
        ..Default::default()
    };
    ALL_EDGE_KINDS
        .into_iter()
        .filter(|kind| options.accepts_kind(*kind))
        .collect()
}

/// Explain an empty reverse-impact result honestly: the traversal only walks
/// RESOLVED edges, so it never proves nothing depends on a node — it can only
/// report what it did not (and could not) traverse.
fn untraversed_summary(graph: &ResolvedGraph, stats: &ResolutionStats, filtered: bool) -> String {
    let coverage = CoverageReport::of(graph);
    let untraversed_files = coverage.files.saturating_sub(coverage.symbol_level_files);
    format!(
        "no matching hits under these bounds{} ({} unresolved edges and {untraversed_files} \
         files without symbol coverage were not traversed)",
        if filtered { " and display filters" } else { "" },
        stats.unresolved
    )
}
