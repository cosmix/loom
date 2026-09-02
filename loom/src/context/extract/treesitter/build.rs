//! Assemble a [`FileExtraction`] from one file's collected matches.

use std::collections::BTreeMap;
use std::path::Path;

use crate::context::extract::{file_node, FileExtraction};
use crate::context::source_graph::{
    body_hash, file_node_id, node_id, FileCoverage, NodeLanguage, SourceEdge, SourceEdgeKind,
    SourceNode, Span,
};

use super::collect::{enclosing, Collected, Definition, Reference};
use super::{IMPORT_CONFIDENCE, UNRESOLVED_CALL_CONFIDENCE};

/// Scope bookkeeping produced while walking definitions, needed to resolve
/// the calls that come after them.
struct DefinitionScopes {
    /// Span of every definition, for locating which one encloses a call site.
    spans: Vec<(Span, String)>,
    /// Every spelling a definition answers to — its bare name and each
    /// scope-qualified suffix, so `Widget::new()` finds the `new` inside
    /// `impl Widget` rather than any other. Last definition of a spelling wins,
    /// matching how a reader resolves a shadowed name by reading top to bottom.
    by_spelling: BTreeMap<String, String>,
}

/// Read-only, file-wide context every emitted definition node carries.
struct DefinitionContext<'a> {
    path: &'a Path,
    file_id: &'a str,
    node_language: &'a NodeLanguage,
    parser_version: &'a str,
    coverage: &'a FileCoverage,
}

/// Assemble nodes and edges from one file's collected matches.
pub(super) fn build(
    path: &Path,
    bytes: &[u8],
    node_language: NodeLanguage,
    parser_version: String,
    walk: Collected,
) -> FileExtraction {
    let coverage = coverage_of(&walk);

    let file_id = file_node_id(path);
    let mut nodes = vec![file_node(
        path,
        bytes,
        node_language.clone(),
        parser_version.clone(),
        &coverage,
    )];
    let mut edges = Vec::new();

    let ctx = DefinitionContext {
        path,
        file_id: &file_id,
        node_language: &node_language,
        parser_version: &parser_version,
        coverage: &coverage,
    };
    let scopes = build_definitions(&walk.definitions, &ctx, &mut nodes, &mut edges);

    import_edges(&walk.imports, &file_id, &mut edges);
    call_edges(&walk.calls, &scopes, &file_id, &mut edges);

    dedupe(&mut edges);

    FileExtraction {
        nodes,
        edges,
        coverage,
    }
}

/// `FileCoverage::Full` unless the walk skipped an unnamed definition match.
fn coverage_of(walk: &Collected) -> FileCoverage {
    if walk.skipped == 0 {
        FileCoverage::Full
    } else {
        FileCoverage::Partial {
            detail: format!("{} definition matches had no usable name", walk.skipped),
        }
    }
}

/// Emit a node and a `Contains` edge for every definition, in source order.
///
/// Definitions are in source order, so a stack of still-open enclosing
/// definitions is enough to derive scope without re-walking the tree.
fn build_definitions(
    definitions: &[Definition],
    ctx: &DefinitionContext,
    nodes: &mut Vec<SourceNode>,
    edges: &mut Vec<SourceEdge>,
) -> DefinitionScopes {
    let mut open: Vec<(Span, String, String)> = Vec::new();
    let mut scopes = DefinitionScopes {
        spans: Vec::new(),
        by_spelling: BTreeMap::new(),
    };

    for definition in definitions {
        emit_definition(definition, ctx, &mut open, &mut scopes, nodes, edges);
    }

    scopes
}

/// Resolve one definition's scope against the still-open stack, then record
/// its node, its `Contains` edge, and its scope-lookup entries.
fn emit_definition(
    definition: &Definition,
    ctx: &DefinitionContext,
    open: &mut Vec<(Span, String, String)>,
    scopes: &mut DefinitionScopes,
    nodes: &mut Vec<SourceNode>,
    edges: &mut Vec<SourceEdge>,
) {
    open.retain(|(span, _, _)| span.end_byte >= definition.span.end_byte);

    let mut scope: Vec<String> = open.iter().map(|(_, name, _)| name.clone()).collect();
    scope.push(definition.name.clone());
    let id = node_id(ctx.path, definition.kind, &scope);
    for spelling in spellings(&scope) {
        scopes.by_spelling.insert(spelling, id.clone());
    }

    let parent = open
        .last()
        .map(|(_, _, parent_id)| parent_id.clone())
        .unwrap_or_else(|| ctx.file_id.to_string());
    edges.push(SourceEdge::parser(
        parent,
        id.clone(),
        SourceEdgeKind::Contains,
        definition.name.clone(),
    ));

    nodes.push(SourceNode {
        id: id.clone(),
        kind: definition.kind,
        path: ctx.path.to_path_buf(),
        scope,
        span: definition.span,
        signature: definition.signature.clone(),
        body_hash: body_hash(&definition.body),
        language: ctx.node_language.clone(),
        parser_version: ctx.parser_version.to_string(),
        coverage: ctx.coverage.clone(),
    });

    scopes.spans.push((definition.span, id.clone()));
    open.push((definition.span, definition.name.clone(), id));
}

/// Every spelling a scope answers to, from the bare name outwards:
/// `example::Widget::helper` is also `Widget::helper` and `helper`.
fn spellings(scope: &[String]) -> impl Iterator<Item = String> + '_ {
    (0..scope.len()).map(|start| scope[start..].join("::"))
}

/// Import edges: the imported file is a different translation unit; nothing
/// here can resolve it, so it is inferred by construction.
fn import_edges(imports: &[Reference], file_id: &str, edges: &mut Vec<SourceEdge>) {
    for import in imports {
        edges.push(SourceEdge::inferred(
            file_id,
            crate::context::source_graph::UNRESOLVED_TARGET,
            SourceEdgeKind::Imports,
            import.symbol.clone(),
            IMPORT_CONFIDENCE,
        ));
    }
}

/// Call edges: a callee this file defines is a parser edge, anything else is
/// inferred and capped below full confidence.
///
/// A qualified callee matches only the spelling it was written with, so
/// `Widget::new()` finds the `new` inside `impl Widget` and a path into another
/// module (`crate::other::new()`) matches nothing here — whole-graph resolution
/// decides that one, with evidence this file does not have.
fn call_edges(
    calls: &[Reference],
    scopes: &DefinitionScopes,
    file_id: &str,
    edges: &mut Vec<SourceEdge>,
) {
    for call in calls {
        let from = enclosing(&scopes.spans, call.at_byte).unwrap_or_else(|| file_id.to_string());
        edges.push(match scopes.by_spelling.get(&call.symbol) {
            Some(target) => SourceEdge::parser(
                from,
                target.clone(),
                SourceEdgeKind::Calls,
                call.symbol.clone(),
            ),
            None => SourceEdge::inferred(
                from,
                crate::context::source_graph::UNRESOLVED_TARGET,
                SourceEdgeKind::Calls,
                call.symbol.clone(),
                UNRESOLVED_CALL_CONFIDENCE,
            ),
        });
    }
}

/// Sort edges into a canonical order and drop exact duplicates, so cold and
/// incremental extraction of the same bytes serialize identically.
fn dedupe(edges: &mut Vec<SourceEdge>) {
    edges.sort_by(|a, b| {
        (&a.from, &a.to, a.kind, a.provenance, &a.symbol).cmp(&(
            &b.from,
            &b.to,
            b.kind,
            b.provenance,
            &b.symbol,
        ))
    });
    edges.dedup_by(|a, b| {
        a.from == b.from
            && a.to == b.to
            && a.kind == b.kind
            && a.provenance == b.provenance
            && a.symbol == b.symbol
    });
}
