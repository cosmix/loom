//! `--find-all`: every indexed node whose name matches.

use colored::Colorize;
use serde_json::{json, Value};

use crate::context::graph_store::ResolvedGraph;
use crate::context::resolve::node_names;
use crate::context::source_graph::SourceNode;

use super::filters::{cap, Removed};
use super::json::safe;
use super::matching::{find_symbol_matches, MatchMode};
use super::ViewOptions;

/// The matches that survive the `--path` and `--lang` scan filters, capped at
/// `--limit`.
struct FindAll<'a> {
    mode: MatchMode,
    hits: Vec<&'a SourceNode>,
    suppressed: usize,
    removed: Removed,
}

fn collect<'a>(graph: &'a ResolvedGraph, symbol: &str, opts: &ViewOptions) -> FindAll<'a> {
    let (matches, mode) = find_symbol_matches(graph, symbol);
    let mut removed = Removed::default();
    let mut hits: Vec<&SourceNode> = matches
        .into_iter()
        .filter(|node| {
            let path = node.path.to_string_lossy();
            opts.filters
                .admit(&path, node.language.as_str(), &mut removed)
        })
        .collect();
    hits.sort_by(|a, b| (&a.path, a.span.line_start).cmp(&(&b.path, b.span.line_start)));
    let suppressed = cap(&mut hits, opts.limit);
    FindAll {
        mode,
        hits,
        suppressed,
        removed,
    }
}

/// Render every indexed node whose name matches `symbol`: an exact,
/// case-sensitive match first, falling back to a case-insensitive substring
/// match only when the exact pass finds nothing.
pub fn render_find_all(graph: &ResolvedGraph, symbol: &str, opts: &ViewOptions) -> String {
    let found = collect(graph, symbol, opts);

    // Flatten only for display - matching above stays on the raw `symbol`.
    let safe_symbol = safe(symbol);
    let mut lines = Vec::new();
    if found.hits.is_empty() {
        lines.push(format!("no nodes match {safe_symbol}"));
    } else {
        lines.push(format!(
            "{} {} - {} matches{}",
            "→".cyan().bold(),
            safe_symbol,
            found.hits.len() + found.suppressed,
            found.mode.label()
        ));
        lines.extend(found.hits.iter().map(|node| find_all_row(node)));
    }
    if found.suppressed > 0 {
        lines.push(format!(
            "  ... {} more suppressed (raise --limit)",
            found.suppressed
        ));
    }
    lines.extend(opts.filters.note(found.removed));
    lines.join("\n")
}

pub fn find_all_json(graph: &ResolvedGraph, symbol: &str, opts: &ViewOptions) -> Value {
    let found = collect(graph, symbol, opts);
    json!({
        "symbol": symbol,
        "match": found.mode.as_str(),
        "exact": found.mode != MatchMode::Substring,
        "matches": found.hits.iter().map(|node| find_match_json(node)).collect::<Vec<_>>(),
        "suppressed_starts": 0,
        "suppressed": found.suppressed,
        "filters": opts.filters.to_json(found.removed, "scan"),
    })
}

/// One `find_all` row: location, kind, name, and — whenever the owning file's
/// coverage is not `full` — the coverage status, so a hit inside a degraded
/// file never renders identically to one from a fully-parsed file.
///
/// `node.path` and the matched name both come from the source graph (a
/// tracked file's path, or a symbol name parsed out of it), so both go
/// through [`crate::context::untrusted::inline_safe`] before reaching
/// agent-visible stdout.
fn find_all_row(node: &SourceNode) -> String {
    let path = safe(&node.path.display().to_string());
    let location = format!("{path}:{}", node.span.line_start);
    let name = safe(&node_names(node).into_iter().next().unwrap_or_default());
    let mut row = format!("  {:<36} {:<10} {}", location, node.kind.as_str(), name);
    if node.coverage.status() != "full" {
        row.push_str(&format!(" [{}]", node.coverage.status()));
    }
    row
}

fn find_match_json(node: &SourceNode) -> Value {
    json!({
        "id": node.id,
        "kind": node.kind.as_str(),
        "path": node.path.to_string_lossy(),
        "line_start": node.span.line_start,
        "coverage_status": node.coverage.status(),
    })
}
