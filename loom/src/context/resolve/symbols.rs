//! What each node in the graph answers to, and which nodes a written name
//! could mean.
//!
//! A node is indexed under its bare name and under every scope-qualified
//! suffix of it, so a call written `Widget::new()` can be matched against the
//! `new` inside `impl Widget` without the bare `new` — a name dozens of types
//! share — deciding anything.

use std::collections::{BTreeMap, BTreeSet};

use crate::context::graph_store::ResolvedGraph;
use crate::context::source_graph::{SourceNode, SourceNodeKind};

/// Order every bucket and drop repeats, so a candidate list is always sorted
/// before its length or first element is read. Determinism depends on it.
pub(super) fn settle(buckets: &mut BTreeMap<String, Vec<String>>) {
    for ids in buckets.values_mut() {
        ids.sort();
        ids.dedup();
    }
}

/// Name -> ids of the nodes defining that name.
#[derive(Debug, Clone, Default)]
pub struct SymbolIndex {
    by_name: BTreeMap<String, Vec<String>>,
    /// Ids of [`SourceNodeKind::Implementation`] nodes, which are indexed by
    /// name but do not define one. See [`SymbolIndex::definitions`].
    implementations: BTreeSet<String>,
}

impl SymbolIndex {
    /// Index every node by its name: the last `scope` segment, or the file name
    /// for a file node, plus every longer suffix of its scope.
    pub fn build(graph: &ResolvedGraph) -> Self {
        let mut index = SymbolIndex::default();
        for node in graph.nodes() {
            for name in node_names(node).into_iter().chain(qualified_names(node)) {
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

    /// Whether two or more nodes answer to `name`, counted before `impl` blocks
    /// are filtered out, so a name genuinely fought over still reports as
    /// contested even when the filter leaves nothing behind.
    pub(super) fn contested(&self, name: &str) -> bool {
        self.lookup(name).len() >= 2
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
    pub(super) fn definitions(&self, name: &str) -> Vec<String> {
        self.lookup(name)
            .iter()
            .filter(|id| !self.implementations.contains(id.as_str()))
            .cloned()
            .collect()
    }

    /// Ids defining `name` inside one of `files`, for a call whose written path
    /// already named the module the callee lives in. No files means no
    /// candidates: a qualifier the graph cannot place is a call out of it.
    pub(super) fn definitions_in(&self, name: &str, files: &[String]) -> Vec<String> {
        self.definitions(name)
            .into_iter()
            .filter(|id| files.iter().any(|file| declared_in(id, file)))
            .collect()
    }
}

/// Whether a node id belongs to `file`. Ids are `<path>#<kind>:<scope>`, so the
/// separator has to be checked or `src/a.rs` would claim `src/a.rs.bak`.
fn declared_in(id: &str, file: &str) -> bool {
    id.strip_prefix(file)
        .is_some_and(|rest| rest.starts_with('#'))
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

/// Every scope-qualified spelling a node also answers to: a `helper` in
/// `impl Widget` inside `mod example` is `Widget::helper` and
/// `example::Widget::helper`. The one-segment spelling is [`node_names`]'s job,
/// and a file node has no scope to qualify.
///
/// This is the whole-graph twin of the same-file lookup the extractor builds in
/// `extract::treesitter::build::spellings`; the two must agree on what a scope
/// is spelled as, or one pass would resolve a call the other could not see.
fn qualified_names(node: &SourceNode) -> Vec<String> {
    (0..node.scope.len().saturating_sub(1))
        .map(|start| node.scope[start..].join("::"))
        .collect()
}
