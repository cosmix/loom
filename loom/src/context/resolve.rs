//! Cross-file symbol resolution and bounded reverse-impact traversal.
//!
//! Extraction is per file, so a call to a function defined elsewhere arrives
//! here as an [`EdgeProvenance::Inferred`] edge pointing at
//! [`UNRESOLVED_TARGET`](crate::context::source_graph::UNRESOLVED_TARGET): the
//! grammar that parsed the call site never saw the definition. This module is
//! the only place holding the whole graph at once, so it is the only place that
//! can turn some of those guesses into targets.
//!
//! **What it may claim.** An unresolved edge may be retargeted when the name it
//! recorded has exactly one definition in the entire graph, published at
//! [`UNIQUE_MATCH_CONFIDENCE`] — above the extraction-time ceiling, below
//! certainty. An import is matched against file paths by a fixed, conservative
//! list of candidate spellings — over the written path, then over progressively
//! shorter prefixes of it, since the tail of a `use` path names an item rather
//! than a file — and only ever when exactly one file matches.
//!
//! **What it may never claim.**
//!
//! - *Never a complete call graph.* Resolution raises confidence where it can
//!   justify it and leaves the rest alone; [`ResolutionStats`] exists so a caller
//!   reports that residue instead of implying completeness.
//! - *Two candidates is ambiguity, not a coin flip.* Two definitions of one name
//!   leave the edge unresolved. Guessing would be indistinguishable from knowing,
//!   which is the failure this subsystem exists to avoid. The single exception is
//!   an `impl` block, which is indexed under its type's name without defining it
//!   (see `SymbolIndex::definitions`) — everything else contests.
//! - *A [`EdgeProvenance::Parser`] edge is never rewritten or downgraded, and
//!   nothing is ever promoted to `Parser`.* That provenance means one grammar saw
//!   both endpoints in one file; name matching cannot manufacture it.
//! - *A resolved edge stays `Inferred`.* Retargeting changes what an edge points
//!   at and how far to trust it, never what evidence produced it.
//!
//! [`impact`](fn@impact) walks the result backwards and reports, for every node
//! reached, the confidence of the *weakest* edge on the path taken — so a chain
//! passing through one guess is never presented as stronger than that guess. It
//! lives in the private `impact` submodule and is re-exported here, so every
//! caller keeps one path: `crate::context::resolve::impact`.

use std::collections::{BTreeMap, BTreeSet};

use crate::context::graph_store::ResolvedGraph;
use crate::context::source_graph::{
    EdgeProvenance, SourceEdge, SourceEdgeKind, SourceNode, SourceNodeKind,
};

mod impact;
pub use impact::{impact, ImpactHit};

/// Confidence for a cross-file name match that is unique in the whole graph.
///
/// Above [`MAX_INFERRED_CONFIDENCE`](crate::context::source_graph::MAX_INFERRED_CONFIDENCE)
/// because a definition unique across every indexed file is strictly stronger
/// evidence than the same-file guess that ceiling governs: the extractor could
/// see one file and had to hedge, while resolution has checked that no other
/// candidate exists anywhere.
///
/// Below 1.0 because a name match is still not a parse. It is wrong when a local
/// binding or a method on another type shadows the name at the call site; when
/// the real callee is chosen at run time through a trait object, virtual method,
/// or generic instantiation; when the name is re-exported and the definition
/// found is an alias rather than the body that runs; or when the true definition
/// lives in a dependency outside the graph and the in-graph match is a namesake.
pub const UNIQUE_MATCH_CONFIDENCE: f32 = 0.75;

/// What resolution did, so a view can report it instead of implying completeness.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResolutionStats {
    /// Unresolved edges retargeted onto a unique definition.
    pub retargeted: usize,
    /// Edges left unresolved because two or more definitions matched.
    pub ambiguous: usize,
    /// Edges still pointing at `UNRESOLVED_TARGET` after resolution.
    pub unresolved: usize,
}

/// Name -> ids of the nodes defining that name.
#[derive(Debug, Clone, Default)]
pub struct SymbolIndex {
    by_name: BTreeMap<String, Vec<String>>,
    /// Ids of [`SourceNodeKind::Implementation`] nodes, which are indexed by name
    /// but do not define one. See `SymbolIndex::definitions`.
    implementations: BTreeSet<String>,
}

impl SymbolIndex {
    /// Index every node by its name: the last `scope` segment, or the file name
    /// for a file node.
    pub fn build(graph: &ResolvedGraph) -> Self {
        let mut index = SymbolIndex::default();
        for node in graph.nodes() {
            for name in node_names(node) {
                index.by_name.entry(name).or_default().push(node.id.clone());
            }
            if node.kind == SourceNodeKind::Implementation {
                index.implementations.insert(node.id.clone());
            }
        }
        settle(&mut index.by_name);
        index
    }

    /// Node ids defining `name`, sorted and deduplicated. Empty when unknown.
    pub fn lookup(&self, name: &str) -> &[String] {
        const UNKNOWN: &[String] = &[];
        self.by_name.get(name).map_or(UNKNOWN, Vec::as_slice)
    }

    /// Ids defining `name`, with `impl`-block nodes removed.
    ///
    /// An `impl` block is scoped under the bare type name, so it lands in the
    /// same bucket as the type — but it does not *define* that name, it attaches
    /// to a type already indexed under it. Counting it as a rival would make
    /// every type that has an `impl` permanently ambiguous, so this is the one
    /// place the ambiguity rule is deliberately narrowed, and it is narrowed for
    /// [`SourceNodeKind::Implementation`] alone: two functions sharing a name are
    /// still genuinely contested, and a name whose only candidates are `impl`
    /// blocks resolves to nothing.
    fn definitions(&self, name: &str) -> Vec<String> {
        self.lookup(name)
            .iter()
            .filter(|id| !self.implementations.contains(id.as_str()))
            .cloned()
            .collect()
    }
}

/// Order every bucket and drop repeats, so a candidate list is always sorted
/// before its length or first element is read. Determinism depends on it.
fn settle(buckets: &mut BTreeMap<String, Vec<String>>) {
    for ids in buckets.values_mut() {
        ids.sort();
        ids.dedup();
    }
}

/// Every name a node is indexed under. A file claims both its file name and its
/// extension-less stem, so `language` finds `src/language.rs`; a stem that also
/// names a symbol shares a bucket precisely so resolution refuses to pick.
pub(crate) fn node_names(node: &SourceNode) -> Vec<String> {
    if node.kind == SourceNodeKind::File {
        return [node.path.file_name(), node.path.file_stem()]
            .into_iter()
            .flatten()
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
    }
    node.scope.last().cloned().into_iter().collect()
}

/// File node ids bucketed by final path segment, so a candidate module path is
/// suffix-matched without scanning every file in the graph.
#[derive(Debug, Default)]
struct PathIndex {
    by_last_segment: BTreeMap<String, Vec<String>>,
    /// First path segment of every file, e.g. `src`, so a leading segment naming
    /// a crate or package rather than a directory can be recognised.
    first_segments: BTreeSet<String>,
}

impl PathIndex {
    fn build(graph: &ResolvedGraph) -> Self {
        let mut index = PathIndex::default();
        for node in graph
            .nodes()
            .filter(|node| node.kind == SourceNodeKind::File)
        {
            let id = node.id.clone();
            index.first_segments.insert(first_segment(&id).to_string());
            let bucket = index.by_last_segment.entry(last_segment(&id).to_string());
            bucket.or_default().push(id);
        }
        settle(&mut index.by_last_segment);
        index
    }

    /// File node ids equal to `candidate` or ending in `/<candidate>`, sorted.
    fn matches(&self, candidate: &str) -> Vec<String> {
        let suffix = format!("/{candidate}");
        let bucket = self.by_last_segment.get(last_segment(candidate));
        bucket
            .into_iter()
            .flatten()
            .filter(|id| id.as_str() == candidate || id.ends_with(&suffix))
            .cloned()
            .collect()
    }
}

fn last_segment(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn first_segment(path: &str) -> &str {
    path.split('/').next().unwrap_or(path)
}

/// Spellings tried after the bare module path, in order. The first form matching
/// anything decides; later forms are never consulted.
const MODULE_SUFFIXES: [&str; 8] = [
    ".rs",
    ".ts",
    ".tsx",
    ".py",
    ".go",
    "/mod.rs",
    "/index.ts",
    "/__init__.py",
];

/// Reduce a module path as written to a project-relative path shape, covering the
/// separators the extractors emit: Rust `crate::a::b`, Python `a.b.c`, TypeScript
/// `./a/b`, Go `mod/a/b`. A path carrying an explicit extension (`./a/b.js`) is
/// mangled by the dot conversion and simply fails to match — the safe outcome.
fn normalize_module_path(symbol: &str, paths: &PathIndex) -> String {
    let trimmed = symbol.trim_start_matches(['.', '/', '@']);
    let converted = trimmed.replace("::", "/").replace('.', "/");
    strip_foreign_root(&converted, paths).to_string()
}

/// Drop one leading segment no file in the graph starts with — `crate::`,
/// `self::`, `super::`, or a package name that is not a directory here.
fn strip_foreign_root<'a>(path: &'a str, paths: &PathIndex) -> &'a str {
    match path.split_once('/') {
        Some((first, rest)) if !rest.is_empty() && !paths.first_segments.contains(first) => rest,
        _ => path,
    }
}

/// File node ids matched by the first spelling of `base` that matches anything,
/// or `None` when no spelling matches a file in the graph.
fn matching_spelling(base: &str, paths: &PathIndex) -> Option<Vec<String>> {
    if base.is_empty() {
        return None;
    }
    std::iter::once(base.to_string())
        .chain(
            MODULE_SUFFIXES
                .iter()
                .map(|suffix| format!("{base}{suffix}")),
        )
        .map(|candidate| paths.matches(&candidate))
        .find(|matched| !matched.is_empty())
}

/// File node ids matched by an import's module path. Empty when nothing matched.
///
/// The last segment of a `use`/`import` path is usually the *item*, not the file:
/// `crate::context::graph_store::GraphStore` names a type inside
/// `context/graph_store.rs`. So when no spelling of the full path matches, the
/// trailing segment is dropped and the spellings are tried again, down to a
/// single segment. Truncation only ever widens the candidate set, and the
/// uniqueness rule still decides: an over-short prefix like `mod` matches many
/// files and therefore resolves nothing.
fn import_candidates(symbol: &str, paths: &PathIndex) -> Vec<String> {
    let base = normalize_module_path(symbol, paths);
    let mut prefix = base.as_str();
    loop {
        if let Some(matched) = matching_spelling(prefix, paths) {
            return matched;
        }
        match prefix.rsplit_once('/') {
            Some((head, _)) if !head.is_empty() => prefix = head,
            _ => return Vec::new(),
        }
    }
}

/// What resolution did to one edge.
enum Outcome {
    Retargeted,
    Ambiguous,
    Unresolved,
}

/// Point `edge` at `target`, through the one primitive allowed to raise an
/// inferred edge above the extraction-time ceiling.
///
/// That ceiling governs a guess made from a single file; this match was checked
/// against the whole graph, which is why the raise is justified. Going through
/// `SourceEdge::resolve_to` rather than assigning the fields keeps the limit
/// structural: it clamps to `MAX_RESOLVED_INFERRED_CONFIDENCE`, keeps provenance
/// `Inferred` — a name match is not a parse — and refuses outright on a `Parser`
/// edge or an already-resolved one, so resolution can neither overwrite what the
/// grammar proved nor relitigate its own earlier decision.
///
/// Returns whether the edge was eligible; an ineligible edge is left untouched.
fn retarget(edge: &mut SourceEdge, target: &str) -> bool {
    edge.resolve_to(target, UNIQUE_MATCH_CONFIDENCE)
}

/// Targets `edge` could name, and whether its name was contested by two or more
/// candidates *before* `impl` blocks were filtered out — so that a name genuinely
/// fought over by several definitions still reports as ambiguous even when the
/// filter leaves nothing behind.
fn candidates_for(
    edge: &SourceEdge,
    symbols: &SymbolIndex,
    paths: &PathIndex,
) -> (Vec<String>, bool) {
    match edge.kind {
        SourceEdgeKind::Calls | SourceEdgeKind::References if !edge.symbol.is_empty() => {
            let contested = symbols.lookup(&edge.symbol).len() >= 2;
            (symbols.definitions(&edge.symbol), contested)
        }
        SourceEdgeKind::Imports if !edge.symbol.is_empty() => {
            (import_candidates(&edge.symbol, paths), false)
        }
        // Containment, implementation and inheritance edges are emitted with both
        // endpoints inside one file. An unresolved one is a fact about the
        // extractor, not something a name match is entitled to repair.
        _ => (Vec::new(), false),
    }
}

/// Resolve one unresolved, inferred edge against the whole-graph indexes.
fn resolve_edge(edge: &mut SourceEdge, symbols: &SymbolIndex, paths: &PathIndex) -> Outcome {
    let (candidates, contested) = candidates_for(edge, symbols, paths);
    match candidates.as_slice() {
        // `retarget` refusing means the edge was not the unresolved inferred
        // edge this pass is meant to act on. Report what actually happened
        // rather than claiming a retarget that did not occur.
        [only] if *only != edge.from => {
            if retarget(edge, only) {
                Outcome::Retargeted
            } else {
                Outcome::Unresolved
            }
        }
        // A lone candidate that is the edge's own origin resolves nothing, and
        // neither does an empty set — but either can still be a contested name.
        [_] | [] if !contested => Outcome::Unresolved,
        _ => Outcome::Ambiguous,
    }
}

/// Rewrite the unresolved edges of `graph` in place where the evidence justifies it.
pub fn resolve_graph(graph: &mut ResolvedGraph) -> ResolutionStats {
    let symbols = SymbolIndex::build(graph);
    let paths = PathIndex::build(graph);
    let mut stats = ResolutionStats::default();

    for entry in graph.files.values_mut() {
        for edge in entry.edges.iter_mut() {
            // `Parser` — and the reserved `Lsp` — mean something stronger than a
            // name match already resolved both endpoints. Never rewritten, and
            // never counted: they are not part of the residue.
            if edge.provenance != EdgeProvenance::Inferred || !edge.is_unresolved() {
                continue;
            }
            match resolve_edge(edge, &symbols, &paths) {
                Outcome::Retargeted => stats.retargeted += 1,
                Outcome::Ambiguous => {
                    stats.ambiguous += 1;
                    stats.unresolved += 1;
                }
                Outcome::Unresolved => stats.unresolved += 1,
            }
        }
    }

    stats
}

#[cfg(test)]
pub(crate) mod fixtures;

#[cfg(test)]
#[path = "resolve/tests_resolve.rs"]
mod tests;
