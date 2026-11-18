//! Assemble a [`FileExtraction`] from one file's collected matches.

use std::path::Path;

use crate::context::extract::{file_node, FileExtraction};
use crate::context::source_graph::{
    file_node_id, node_id, syntax_confidence, EdgeProvenance, FileCoverage, NodeLanguage,
    SourceEdge, SourceEdgeKind, SourceNode, Span,
};

use super::binding::{
    call_edges, reference_edges, Binder, BindingRules, DefinitionInfo, DefinitionScopes,
};
use super::collect::{Collected, Definition, Reference};
use super::ids::disambiguate;

/// Read-only, file-wide context every emitted definition node carries.
struct DefinitionContext<'a> {
    path: &'a Path,
    file_id: &'a str,
    node_language: &'a NodeLanguage,
    parser_version: &'a str,
    coverage: &'a FileCoverage,
}

/// A definition with its scope and innermost enclosing definition resolved.
struct Placed<'a> {
    definition: &'a Definition,
    scope: Vec<String>,
    /// Index of the innermost enclosing definition, if any.
    parent: Option<usize>,
}

/// Assemble nodes and edges from one file's collected matches.
pub(super) fn build(
    path: &Path,
    bytes: &[u8],
    node_language: NodeLanguage,
    parser_version: String,
    walk: Collected,
    rules: &BindingRules,
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
    let binder = Binder {
        scopes: &scopes,
        imports: &walk.bindings,
        rules,
        file_id: &file_id,
    };
    call_edges(&walk.calls, &binder, &mut edges);
    reference_edges(&walk.references, &binder, &mut edges);

    dedupe(&mut edges);

    FileExtraction {
        nodes,
        edges,
        coverage,
        imports: walk.bindings,
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
/// Every id is settled before anything is emitted: a parent's final id
/// depends on duplicates of it that may come after its children.
fn build_definitions(
    definitions: &[Definition],
    ctx: &DefinitionContext,
    nodes: &mut Vec<SourceNode>,
    edges: &mut Vec<SourceEdge>,
) -> DefinitionScopes {
    let placed = place(definitions);
    let bases: Vec<(String, &str)> = placed
        .iter()
        .map(|entry| {
            let definition = entry.definition;
            let base = node_id(ctx.path, definition.kind, &entry.scope);
            (base, definition.signature.as_str())
        })
        .collect();
    let ids = disambiguate(&bases);

    let mut scopes = DefinitionScopes::default();
    for (entry, (id, symbol_key)) in placed.iter().zip(&ids) {
        let definition = entry.definition;
        let parent = entry.parent.map(|index| ids[index].0.clone());
        edges.push(SourceEdge::structural(
            parent.as_deref().unwrap_or(ctx.file_id),
            id.clone(),
            definition.name.clone(),
        ));
        nodes.push(definition_node(entry, id, symbol_key, ctx));
        let info = DefinitionInfo {
            kind: definition.kind,
            scope: entry.scope.clone(),
            parent,
            qualified: !definition.qualifier.is_empty(),
        };
        record_scope(&mut scopes, id, definition.span, info);
    }
    scopes
}

/// Resolve every definition's scope and innermost enclosing definition.
///
/// Definitions are in source order, so a stack of still-open enclosing
/// definitions is enough to derive scope without re-walking the tree. A
/// definition's scope is its parent's, then any qualifier written on it, then
/// its own name.
fn place(definitions: &[Definition]) -> Vec<Placed<'_>> {
    let mut open: Vec<usize> = Vec::new();
    let mut placed: Vec<Placed> = Vec::with_capacity(definitions.len());
    for (index, definition) in definitions.iter().enumerate() {
        open.retain(|&outer| definitions[outer].span.end_byte >= definition.span.end_byte);
        let parent = open.last().copied();
        let mut scope = parent
            .map(|outer| placed[outer].scope.clone())
            .unwrap_or_default();
        scope.extend(definition.qualifier.iter().cloned());
        scope.push(definition.name.clone());
        placed.push(Placed {
            definition,
            scope,
            parent,
        });
        open.push(index);
    }
    placed
}

/// The node for one placed definition, under its final id.
fn definition_node(
    placed: &Placed,
    id: &str,
    symbol_key: &str,
    ctx: &DefinitionContext,
) -> SourceNode {
    let definition = placed.definition;
    SourceNode {
        id: id.to_string(),
        kind: definition.kind,
        path: ctx.path.to_path_buf(),
        scope: placed.scope.clone(),
        span: definition.span,
        signature: definition.signature.clone(),
        body_hash: definition.body_hash.clone(),
        language: ctx.node_language.clone(),
        parser_version: ctx.parser_version.to_string(),
        coverage: ctx.coverage.clone(),
        symbol_key: symbol_key.to_string(),
    }
}

/// Record a definition's span, its binding facts, and every spelling it
/// answers to. Final ids are unique, so each spelling lists an id once.
fn record_scope(scopes: &mut DefinitionScopes, id: &str, span: Span, info: DefinitionInfo) {
    for spelling in spellings(&info.scope) {
        scopes
            .by_spelling
            .entry(spelling)
            .or_default()
            .push(id.to_string());
    }
    scopes.spans.push((span, id.to_string()));
    scopes.by_id.insert(id.to_string(), info);
}

/// Every spelling a scope answers to, from the bare name outwards:
/// `example::Widget::helper` is also `Widget::helper` and `helper`.
fn spellings(scope: &[String]) -> impl Iterator<Item = String> + '_ {
    (0..scope.len()).map(|start| scope[start..].join("::"))
}

/// Import edges: the imported file is a different translation unit; nothing
/// here can resolve it, so it is a syntax edge by construction.
fn import_edges(imports: &[Reference], file_id: &str, edges: &mut Vec<SourceEdge>) {
    for import in imports {
        edges.push(SourceEdge::syntax(
            file_id,
            SourceEdgeKind::Imports,
            import.symbol.clone(),
            import.site,
            syntax_confidence(SourceEdgeKind::Imports),
        ));
    }
}

/// Sort edges into a canonical order and merge equal ones, so two calls to
/// one callee are one edge with two sites, and cold and incremental
/// extraction of the same bytes serialize identically.
///
/// Edges are equal on `(from, to, kind, provenance, symbol, receiver)`; their
/// sites merge, sorted by position and deduplicated.
fn dedupe(edges: &mut Vec<SourceEdge>) {
    edges.sort_by(|a, b| edge_key(a).cmp(&edge_key(b)));
    edges.dedup_by(|next, kept| {
        let equal = edge_key(next) == edge_key(kept);
        if equal {
            kept.sites.append(&mut next.sites);
        }
        equal
    });
    for edge in edges.iter_mut() {
        edge.sites
            .sort_by_key(|site| (site.start_byte, site.end_byte));
        edge.sites.dedup();
    }
}

/// The identity [`dedupe`] merges on: `(from, to, kind, provenance, symbol,
/// receiver)`.
type EdgeKey<'a> = (
    &'a str,
    &'a str,
    SourceEdgeKind,
    EdgeProvenance,
    &'a str,
    Option<&'a str>,
);

fn edge_key(edge: &SourceEdge) -> EdgeKey<'_> {
    (
        edge.from.as_str(),
        edge.to.as_str(),
        edge.kind,
        edge.provenance,
        edge.symbol.as_str(),
        edge.receiver.as_deref(),
    )
}
