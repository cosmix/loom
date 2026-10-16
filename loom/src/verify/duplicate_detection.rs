//! Advisory exact-copy candidates from the shared source graph.
use crate::context::graph_store::ResolvedGraph;
use crate::context::source_graph::{SourceNode, SourceNodeKind};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
pub(crate) mod graph;

/// A potential symbol duplication between a new/modified file and an existing file.
#[derive(Debug)]
pub struct DuplicateSymbol {
    /// The name of the symbol that appears to be duplicated
    pub symbol_name: String,
    /// Path (relative to worktree) of the new or modified file
    pub new_file: String,
    /// Line number within `new_file` where the symbol is defined
    pub new_line: usize,
    /// Path (relative to worktree) of the existing file where the same name is found
    pub existing_file: String,
    /// Line number within `existing_file` where the name is found
    pub existing_line: usize,
    /// Human-readable symbol kind (e.g. "function", "struct", "class")
    pub symbol_type: String,
}

const NOISE_NAMES: &[&str] = &[
    "new",
    "default",
    "from",
    "into",
    "build",
    "test",
    "main",
    "run",
    "init",
    "error",
    "config",
    "types",
    "utils",
    "helpers",
    "common",
    "get",
    "set",
    "create",
    "update",
    "delete",
    "list",
    "parse",
    "format",
    "write",
    "read",
    "open",
    "close",
    "handle",
    "process",
    "execute",
    "start",
    "stop",
    "reset",
    "clone",
    "copy",
    "add",
    "remove",
    "insert",
    "find",
    "check",
    "validate",
    "load",
    "save",
    "send",
    "receive",
    "connect",
    "disconnect",
];

/// Compare graph definitions; equal names alone are never evidence of a copy.
/// Scoped methods must share their enclosing scope, language and exact bytes.
pub fn detect_duplicate_symbols(
    directory: &Path,
    base_branch: &str,
) -> Result<Vec<DuplicateSymbol>> {
    let root = graph::repository_root(directory)?;
    let changed = graph::changed_files(&root, base_branch, false)?;
    if changed.is_empty() {
        return Ok(Vec::new());
    }
    let built = graph::graph(&root)?;
    Ok(duplicate_candidates(&built.graph, &changed))
}

fn duplicate_candidates(graph: &ResolvedGraph, changed: &BTreeSet<String>) -> Vec<DuplicateSymbol> {
    let mut groups = BTreeMap::new();
    for node in graph.nodes().filter(|node| candidate(node)) {
        let key = (
            node.kind,
            node.language.as_str(),
            &node.scope,
            &node.body_hash,
        );
        groups.entry(key).or_insert_with(Vec::new).push(node);
    }
    let mut found = Vec::new();
    for nodes in groups.values() {
        for (index, left) in nodes.iter().enumerate() {
            for right in &nodes[index + 1..] {
                let left_changed = changed.contains(left.path.to_string_lossy().as_ref());
                let right_changed = changed.contains(right.path.to_string_lossy().as_ref());
                if (!left_changed && !right_changed) || !same_definition(left, right) {
                    continue;
                }
                let (new, existing) = if left_changed {
                    (*left, *right)
                } else {
                    (*right, *left)
                };
                found.push(DuplicateSymbol {
                    symbol_name: new.scope.last().cloned().unwrap_or_default(),
                    new_file: new.path.to_string_lossy().into_owned(),
                    new_line: new.span.line_start,
                    existing_file: existing.path.to_string_lossy().into_owned(),
                    existing_line: existing.span.line_start,
                    symbol_type: new.kind.to_string(),
                });
            }
        }
    }
    found
}

fn candidate(node: &SourceNode) -> bool {
    matches!(
        node.kind,
        SourceNodeKind::Function
            | SourceNodeKind::Type
            | SourceNodeKind::Interface
            | SourceNodeKind::Constant
    ) && node.coverage.has_symbols()
        && node
            .scope
            .last()
            .is_some_and(|name| !NOISE_NAMES.contains(&name.as_str()))
}

fn same_definition(left: &SourceNode, right: &SourceNode) -> bool {
    left.id != right.id
        && left.kind == right.kind
        && left.language == right.language
        && left.scope == right.scope
        && !left.body_hash.is_empty()
        && left.body_hash == right.body_hash
}

#[cfg(test)]
#[path = "tests_duplicate_detection.rs"]
mod tests;
