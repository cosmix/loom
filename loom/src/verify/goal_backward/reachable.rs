//! `reachable` checks (plan version 2): a named unit must be reachable from an
//! entry point in the worktree's source graph.
//!
//! The walk is [`impact_with`] run backwards from `symbol`; a hit on a `from`
//! node proves a path of resolved edges exists. Reachability here is
//! reachability in the derived graph: unresolved edges are never walked, so an
//! absent path means "not found", never "does not exist".

use crate::context::extract::{registry, SourceGraphExtractor};
use crate::context::graph_store::ResolvedGraph;
use crate::context::resolve::{impact_with, node_names, ImpactOptions};
use crate::context::source_graph::{SourceEdgeKind, SourceNode, SourceNodeKind};
use crate::context::worktree_graph::WorktreeGraph;
use crate::plan::schema::ReachableCheck;

use super::result::{GapType, VerificationGap};

/// Edge kinds a reachable walk follows (DESIGN D11).
pub(in crate::verify) const REACHABLE_KINDS: [SourceEdgeKind; 6] = [
    SourceEdgeKind::Calls,
    SourceEdgeKind::References,
    SourceEdgeKind::Implements,
    SourceEdgeKind::Extends,
    SourceEdgeKind::Contains,
    SourceEdgeKind::Imports,
];

/// Verify every `reachable` check against one worktree graph.
///
/// A check whose `symbol` or `from` resolves only to files no extractor
/// parses is skipped with a warning on stderr: the graph cannot see inside
/// such a file, so neither a pass nor a gap would be evidence.
pub fn verify_reachable(checks: &[ReachableCheck], graph: &WorktreeGraph) -> Vec<VerificationGap> {
    let extractors = registry();
    checks
        .iter()
        .filter_map(|check| verify_one(check, graph, &extractors))
        .collect()
}

fn verify_one(
    check: &ReachableCheck,
    graph: &WorktreeGraph,
    extractors: &[Box<dyn SourceGraphExtractor + Send + Sync>],
) -> Option<VerificationGap> {
    let symbols = exact_matches(&graph.graph, &check.symbol);
    let starts = exact_matches(&graph.graph, &check.from);
    let ends = [(&check.symbol, &symbols), (&check.from, &starts)];
    if let Some((name, _)) = ends.iter().find(|(_, nodes)| nodes.is_empty()) {
        return Some(not_found_gap(check, name, graph));
    }
    let unparsed = |nodes: &[&SourceNode]| {
        !nodes.iter().any(|node| {
            extractors
                .iter()
                .any(|extractor| extractor.supports(&node.path))
        })
    };
    if let Some((name, _)) = ends.iter().find(|(_, nodes)| unparsed(nodes.as_slice())) {
        eprintln!(
            "warning: reachable check skipped: {name} is only in files no source-graph \
             extractor parses ({})",
            check.description
        );
        return None;
    }
    let min_confidence = check.min_confidence.unwrap_or(0.0);
    let reached = symbols
        .iter()
        .any(|symbol| reaches(&graph.graph, symbol, &starts, min_confidence));
    (!reached).then(|| unreachable_gap(check, graph))
}

/// Nodes answering to `name` exactly: the exact branch of
/// `map::views::find_symbol_matches` (case-sensitive, never a substring).
///
/// A symbol-level match wins over a file of the same name. `main` is the
/// function, not `src/main.rs`, whose containment edges would otherwise make
/// every item in that file "reachable from main".
fn exact_matches<'a>(graph: &'a ResolvedGraph, name: &str) -> Vec<&'a SourceNode> {
    let exact: Vec<&SourceNode> = graph
        .nodes()
        .filter(|node| node_names(node).iter().any(|candidate| candidate == name))
        .collect();
    let is_symbol = |node: &&SourceNode| node.kind != SourceNodeKind::File;
    if exact.iter().any(is_symbol) {
        exact.into_iter().filter(is_symbol).collect()
    } else {
        exact
    }
}

/// True when any `starts` node is reached walking backwards from `symbol`.
fn reaches(
    graph: &ResolvedGraph,
    symbol: &SourceNode,
    starts: &[&SourceNode],
    min_confidence: f32,
) -> bool {
    let options = ImpactOptions {
        max_depth: usize::MAX,
        kinds: REACHABLE_KINDS.to_vec(),
        limit: 0,
        path_prefix: None,
        min_confidence,
    };
    impact_with(graph, &symbol.id, &options)
        .hits
        .iter()
        .any(|hit| starts.iter().any(|start| start.id == hit.id))
}

fn not_found_gap(check: &ReachableCheck, name: &str, graph: &WorktreeGraph) -> VerificationGap {
    gap(
        graph,
        format!("symbol not found: {name} ({})", check.description),
        format!(
            "Define {name}, or name it exactly (case-sensitive) as `loom map --find-all {name}` \
             lists it"
        ),
    )
}

fn unreachable_gap(check: &ReachableCheck, graph: &WorktreeGraph) -> VerificationGap {
    let (symbol, from) = (&check.symbol, &check.from);
    gap(
        graph,
        format!(
            "{symbol} is not reachable from {from} ({})",
            check.description
        ),
        format!(
            "Wire {symbol} into a path from {from} whose every edge has confidence >= {:.2}; \
             inspect with `loom map --impact {symbol}`",
            check.min_confidence.unwrap_or(0.0)
        ),
    )
}

/// Every reachable gap carries the graph's degraded mode as evidence.
fn gap(graph: &WorktreeGraph, description: String, suggestion: String) -> VerificationGap {
    let description = match &graph.degraded {
        Some(reason) => format!("{description} [source graph degraded: {reason}]"),
        None => description,
    };
    VerificationGap::new(GapType::Unreachable, description, suggestion)
}

#[cfg(test)]
#[path = "reachable_tests.rs"]
mod tests;
