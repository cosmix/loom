//! `--outline`: the indexed symbols of one file, in source order.
//!
//! The row format `L<a>-L<b>  kind  scope  signature` is the contract of
//! `loom-hooks/_read_discipline.sh`; change it and that hook stops parsing.

use std::path::Path;

use colored::Colorize;
use serde_json::{json, Map, Value};

use crate::context::graph_store::ResolvedGraph;
use crate::context::source_graph::{FileCoverage, SourceNode, SourceNodeKind};

use super::json::safe;
use super::matching::MatchMode;
use super::project_relative;

/// Signature column is collapsed to single spaces and capped at this many
/// characters (including the trailing ellipsis) so an outline stays readable.
const SIGNATURE_MAX_LEN: usize = 80;

/// Render every indexed node of one file, in source order.
pub fn render_outline(graph: &ResolvedGraph, project_root: &Path, arg: &str) -> String {
    let rel = project_relative(project_root, arg).unwrap_or_else(|| arg.to_string());
    // Flatten only for display - the graph lookup below keys on the raw
    // `rel`, since flattening must never change matching behaviour.
    let safe_rel = safe(&rel);

    let Some(entry) = graph.files.get(&rel) else {
        return format!("no indexed file at {safe_rel}");
    };

    let mut lines = vec![format!("{} Outline: {}", "→".cyan().bold(), safe_rel)];
    lines.extend(
        symbols_in_order(&entry.nodes)
            .iter()
            .map(|node| format!("  {}", format_node_line(node))),
    );
    lines.push(file_coverage_line(&entry.coverage));
    lines.join("\n")
}

pub fn outline_json(graph: &ResolvedGraph, project_root: &Path, arg: &str) -> Value {
    let rel = project_relative(project_root, arg).unwrap_or_else(|| arg.to_string());
    let path = &rel;
    let Some(entry) = graph.files.get(&rel) else {
        return json!({"path": path, "error": "no indexed file", "match": MatchMode::Id.as_str()});
    };
    json!({
        "path": path,
        "match": MatchMode::Id.as_str(),
        "coverage": coverage_json(&entry.coverage),
        "symbols": symbols_in_order(&entry.nodes).into_iter().map(symbol_json).collect::<Vec<_>>(),
    })
}

fn symbols_in_order(nodes: &[SourceNode]) -> Vec<&SourceNode> {
    let mut symbols: Vec<&SourceNode> = nodes
        .iter()
        .filter(|node| node.kind != SourceNodeKind::File)
        .collect();
    symbols.sort_by_key(|node| node.span.start_byte);
    symbols
}

/// Format one outline row: line range, kind, scope path, signature.
///
/// `scope` and `signature` both originate in the parsed source file, not in
/// this program - they go through [`crate::context::untrusted::inline_safe`]
/// like every other graph-derived value rendered on this agent-facing
/// surface (see the module's containment rule).
fn format_node_line(node: &SourceNode) -> String {
    let range = format!("L{}-L{}", node.span.line_start, node.span.line_end);
    let scope = safe(&node.scope.join("::"));
    let signature = safe(&node.signature);
    let signature = crate::utils::truncate_for_display(&signature, SIGNATURE_MAX_LEN);
    format!(
        "{:<11} {:<11} {:<30} {}",
        range,
        node.kind.as_str(),
        scope,
        signature
    )
}

/// A file's own coverage status, with the detail that explains a degraded
/// status — a file with no symbols must say why.
///
/// Every `detail` here is flattened through
/// [`crate::context::untrusted::inline_safe`]: `ParseError`'s detail is
/// built from a raw line of the offending source file
/// (`context::extract::treesitter::collect::first_error`), so it is
/// repo-controlled text reaching agent-visible stdout the same way a
/// Knowledge Brief field is.
fn file_coverage_line(coverage: &FileCoverage) -> String {
    let status = coverage.status();
    match coverage {
        FileCoverage::Deleted | FileCoverage::Full => format!("coverage: {status}"),
        FileCoverage::Partial { detail } | FileCoverage::LexicalOnly { detail } => {
            format!("coverage: {status} - {}", safe(detail))
        }
        FileCoverage::Oversized { bytes, limit } => {
            format!("coverage: {status} - {bytes} bytes (limit {limit})")
        }
        FileCoverage::ParseError { detail, .. } => {
            format!("coverage: {status} - {}", safe(detail))
        }
    }
}

fn symbol_json(node: &SourceNode) -> Value {
    json!({
        "id": node.id,
        "kind": node.kind.as_str(),
        "scope": node.scope.iter().map(|segment| safe(segment)).collect::<Vec<_>>(),
        "line_start": node.span.line_start,
        "line_end": node.span.line_end,
        "signature": safe(&node.signature),
    })
}

fn coverage_json(coverage: &FileCoverage) -> Value {
    let mut value = Map::new();
    value.insert("status".to_string(), json!(coverage.status()));
    let detail = match coverage {
        FileCoverage::Partial { detail }
        | FileCoverage::LexicalOnly { detail }
        | FileCoverage::ParseError { detail, .. } => Some(safe(detail)),
        FileCoverage::Oversized { bytes, limit } => Some(format!("{bytes} bytes (limit {limit})")),
        FileCoverage::Deleted | FileCoverage::Full => None,
    };
    if let Some(detail) = detail {
        value.insert("detail".to_string(), json!(detail));
    }
    Value::Object(value)
}
