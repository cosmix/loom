//! `--callers`, `--callees` and `--references`: one hop over call or
//! reference edges, each row carrying the sites that wrote the edge.

use std::path::Path;

use colored::Colorize;
use serde_json::{json, Value};

use crate::context::graph_store::ResolvedGraph;
use crate::context::resolve::{direct_callees, direct_callers, direct_references, Neighbor};
use crate::context::source_graph::Span;

use super::filters::{cap, language_of, Removed};
use super::json::safe;
use super::matching::{match_note, resolve_starts, ResolvedStarts};
use super::ViewOptions;

type Query = fn(&ResolvedGraph, &str, usize) -> (Vec<Neighbor>, usize);

/// The three direct-neighbour views.
#[derive(Clone, Copy)]
enum Relation {
    Callers,
    Callees,
    References,
}

impl Relation {
    fn heading(self) -> &'static str {
        match self {
            Relation::Callers => "Callers",
            Relation::Callees => "Callees",
            Relation::References => "References",
        }
    }

    fn empty_state(self) -> &'static str {
        match self {
            Relation::Callers => "no direct callers (call edges only)",
            Relation::Callees => "no direct callees (call edges only)",
            Relation::References => "no direct references (reference edges only)",
        }
    }

    fn query(self) -> Query {
        match self {
            Relation::Callers => direct_callers,
            Relation::Callees => direct_callees,
            Relation::References => direct_references,
        }
    }

    /// True when the row's neighbour is the side that wrote the site, so the
    /// queried node is the target.
    fn neighbor_is_source(self) -> bool {
        !matches!(self, Relation::Callees)
    }
}

struct StartRows {
    id: String,
    /// The queried node's file and declaration line.
    path: String,
    line: Option<usize>,
    rows: Vec<Neighbor>,
    suppressed: usize,
}

struct Neighbors {
    resolved: ResolvedStarts,
    per_start: Vec<StartRows>,
    removed: Removed,
}

impl Neighbors {
    fn suppressed(&self) -> usize {
        self.per_start.iter().map(|start| start.suppressed).sum()
    }
}

/// Query every shown start unlimited, then apply `--path`, `--lang` and
/// `--limit` to the finished rows.
fn collect(
    graph: &ResolvedGraph,
    project_root: &Path,
    arg: &str,
    opts: &ViewOptions,
    relation: Relation,
) -> Neighbors {
    let resolved = resolve_starts(graph, project_root, arg);
    let mut removed = Removed::default();
    let mut per_start = Vec::new();
    for id in resolved.shown() {
        let (mut rows, _) = relation.query()(graph, id, 0);
        rows.retain(|row| {
            opts.filters
                .admit(&row.path, language_of(graph, &row.path), &mut removed)
        });
        let suppressed = cap(&mut rows, opts.limit);
        let node = graph.node(id);
        per_start.push(StartRows {
            id: id.clone(),
            path: node
                .map(|n| n.path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            line: node.map(|n| n.span.line_start),
            rows,
            suppressed,
        });
    }
    Neighbors {
        resolved,
        per_start,
        removed,
    }
}

pub fn render_callers(
    graph: &ResolvedGraph,
    project_root: &Path,
    arg: &str,
    opts: &ViewOptions,
) -> String {
    render(graph, project_root, arg, opts, Relation::Callers)
}

pub fn render_callees(
    graph: &ResolvedGraph,
    project_root: &Path,
    arg: &str,
    opts: &ViewOptions,
) -> String {
    render(graph, project_root, arg, opts, Relation::Callees)
}

pub fn render_references(
    graph: &ResolvedGraph,
    project_root: &Path,
    arg: &str,
    opts: &ViewOptions,
) -> String {
    render(graph, project_root, arg, opts, Relation::References)
}

pub fn callers_json(
    graph: &ResolvedGraph,
    project_root: &Path,
    arg: &str,
    opts: &ViewOptions,
) -> Value {
    json_view(graph, project_root, arg, opts, Relation::Callers)
}

pub fn callees_json(
    graph: &ResolvedGraph,
    project_root: &Path,
    arg: &str,
    opts: &ViewOptions,
) -> Value {
    json_view(graph, project_root, arg, opts, Relation::Callees)
}

pub fn references_json(
    graph: &ResolvedGraph,
    project_root: &Path,
    arg: &str,
    opts: &ViewOptions,
) -> Value {
    json_view(graph, project_root, arg, opts, Relation::References)
}

fn render(
    graph: &ResolvedGraph,
    project_root: &Path,
    arg: &str,
    opts: &ViewOptions,
    relation: Relation,
) -> String {
    let found = collect(graph, project_root, arg, opts, relation);
    let safe_arg = safe(arg);
    if found.resolved.starts.is_empty() {
        return format!("no indexed node named {safe_arg}");
    }

    let mut lines = Vec::new();
    lines.extend(match_note(&found.resolved, &safe_arg));
    for start in &found.per_start {
        lines.push(format!(
            "{} {} of {}{}",
            "→".cyan().bold(),
            relation.heading(),
            safe(&start.id),
            found.resolved.mode.label()
        ));
        if start.rows.is_empty() {
            lines.push(format!("  {}", relation.empty_state()));
        }
        lines.extend(start.rows.iter().map(|row| row_line(start, row, relation)));
        if start.suppressed > 0 {
            lines.push(format!(
                "  ... {} more suppressed (raise --limit)",
                start.suppressed
            ));
        }
    }
    if found.resolved.suppressed() > 0 {
        lines.push(format!(
            "  ... {} more suppressed",
            found.resolved.suppressed()
        ));
    }
    lines.extend(opts.filters.note(found.removed));
    lines.join("\n")
}

/// `<site path>:L<site line> → <target path>:L<decl line>  symbol=<written>
/// <provenance> <confidence>  sites=<n>`, plus `candidate (<n> total)` for a
/// member of an ambiguous call's candidate set and the neighbour's node id.
fn row_line(start: &StartRows, row: &Neighbor, relation: Relation) -> String {
    let site_line = row.sites.first().map(|site| site.line_start);
    let (target_path, target_line) = if relation.neighbor_is_source() {
        (start.path.as_str(), start.line)
    } else {
        (row.path.as_str(), row.line_start)
    };
    let mut line = format!(
        "  {} → {}  symbol={}  {} {:.2}  sites={}",
        located(&row.site_path, site_line),
        located(target_path, target_line),
        safe(&row.symbol),
        row.provenance,
        row.confidence,
        row.sites.len()
    );
    if let Some(total) = row.candidate_of {
        line.push_str(&format!("  candidate ({total} total)"));
    }
    line.push_str(&format!("  node={}", safe(&row.id)));
    line
}

fn located(path: &str, line: Option<usize>) -> String {
    let line = line.map_or_else(|| "?".to_string(), |line| line.to_string());
    format!("{}:L{line}", safe(path))
}

fn json_view(
    graph: &ResolvedGraph,
    project_root: &Path,
    arg: &str,
    opts: &ViewOptions,
    relation: Relation,
) -> Value {
    let found = collect(graph, project_root, arg, opts, relation);
    json!({
        "match": found.resolved.mode.as_str(),
        "start_ids": found.per_start.iter().map(|start| &start.id).collect::<Vec<_>>(),
        "neighbors": found.per_start.iter().flat_map(|start| start.rows.iter().map(neighbor_json)).collect::<Vec<_>>(),
        "suppressed_starts": found.resolved.suppressed(),
        "suppressed": found.suppressed(),
        "filters": opts.filters.to_json(found.removed, "display"),
    })
}

fn neighbor_json(neighbor: &Neighbor) -> Value {
    json!({
        "id": neighbor.id,
        "kind": neighbor.kind.as_str(),
        "path": neighbor.path,
        "edge_kind": neighbor.edge_kind.as_str(),
        "confidence": neighbor.confidence,
        "provenance": neighbor.provenance.as_str(),
        "line_start": neighbor.line_start,
        "symbol": safe(&neighbor.symbol),
        "site_path": neighbor.site_path,
        "sites": neighbor.sites.iter().map(site_json).collect::<Vec<_>>(),
        "candidate_of": neighbor.candidate_of,
    })
}

fn site_json(site: &Span) -> Value {
    json!({
        "line_start": site.line_start,
        "line_end": site.line_end,
        "start_byte": site.start_byte,
        "end_byte": site.end_byte,
    })
}
