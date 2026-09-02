//! Matching a module path as written onto a file in the graph.
//!
//! Two rules, in order. A path that says where it starts from — `crate::`,
//! `self::`, `super::` — is **anchored**: it is joined onto a crate root or
//! onto the module the citing file belongs to, and only files under that anchor
//! can match. Anything else is **suffix-matched** against every file, which is
//! all a bare `a::b` or a Go import path supports.
//!
//! Anchoring is what separates two files of the same name: `crate::codex` is
//! `<crate root>/codex.rs` and never the `codex.rs` sitting four directories
//! deeper. Suffix matching cannot tell them apart, so it declines — the
//! uniqueness rule still decides both paths.
//!
//! A relative anchor is weaker than a rooted one and is used as such: `self::`
//! and `super::` reach files *below* the module they name but never that module
//! itself, because the same line means different modules at the top of a file
//! and inside an inline `mod` block, and extraction does not record which.

use std::collections::{BTreeMap, BTreeSet};

use crate::context::graph_store::ResolvedGraph;
use crate::context::source_graph::SourceNodeKind;

use super::symbols::settle;

/// Spellings tried after the bare module path, in order. The first form matching
/// anything decides; later forms are never consulted.
///
/// The `/lib.rs` and `/main.rs` forms are what let a directory match as a crate
/// root, so an item written `crate::x` and defined in `lib.rs` itself is
/// reachable; they stay last so a module file is always preferred.
const MODULE_SUFFIXES: [&str; 10] = [
    ".rs",
    ".ts",
    ".tsx",
    ".py",
    ".go",
    "/mod.rs",
    "/index.ts",
    "/__init__.py",
    "/lib.rs",
    "/main.rs",
];

/// Files whose directory is a Rust crate root, which is what `crate::` names.
const CRATE_ROOT_FILES: [&str; 2] = ["lib.rs", "main.rs"];

/// Files that are the module they sit in rather than a module below it, so a
/// path written inside them starts from their own directory.
const MODULE_ROOT_FILES: [&str; 3] = ["mod.rs", "lib.rs", "main.rs"];

/// File node ids bucketed by final path segment, so a candidate module path is
/// suffix-matched without scanning every file in the graph.
#[derive(Debug, Default)]
pub(super) struct PathIndex {
    by_last_segment: BTreeMap<String, Vec<String>>,
    /// First path segment of every file, e.g. `src`, so a leading segment naming
    /// a crate or package rather than a directory can be recognised.
    first_segments: BTreeSet<String>,
    /// Directories holding a crate root, which is where a `crate::` path starts.
    crate_roots: BTreeSet<String>,
}

impl PathIndex {
    pub(super) fn build(graph: &ResolvedGraph) -> Self {
        let mut index = PathIndex::default();
        for node in graph
            .nodes()
            .filter(|node| node.kind == SourceNodeKind::File)
        {
            let id = node.id.clone();
            index.first_segments.insert(first_segment(&id).to_string());
            if CRATE_ROOT_FILES.contains(&last_segment(&id)) {
                index.crate_roots.insert(directory_of(&id).to_string());
            }
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

fn directory_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(head, _)| head)
}

/// Join a directory onto a relative path, tolerating an empty directory so a
/// file at the repository root anchors to the root rather than to `/`. An empty
/// directory and an empty relative path together give an empty base, which
/// [`matching_spelling`] declines rather than matching every file.
fn join(directory: &str, relative: &str) -> String {
    match (directory.is_empty(), relative.is_empty()) {
        (true, _) => relative.to_string(),
        (false, true) => directory.to_string(),
        (false, false) => format!("{directory}/{relative}"),
    }
}

/// File node ids matched by an import's module path, or by the qualifier of a
/// qualified call. Empty when nothing matched.
pub(super) fn import_candidates(symbol: &str, from: &str, paths: &PathIndex) -> Vec<String> {
    let written = written_path(symbol);
    let anchored = anchored_candidates(written, from, paths);
    if !anchored.is_empty() {
        return anchored;
    }
    suffix_candidates(written, paths)
}

/// The path part of an import as written: everything before a brace group, a
/// glob, or an `as` clause, none of which name a file. A `use` path can arrive
/// wrapped across lines, so whitespace ends the path too.
fn written_path(symbol: &str) -> &str {
    let end = symbol
        .find(['{', '*', ' ', '\t', '\n', '\r'])
        .unwrap_or(symbol.len());
    symbol[..end].trim_end_matches([':', '/', '.'])
}

/// Candidates for a path that names where it starts from.
///
/// `crate::` is tried under every crate root in the graph — more than one means
/// more than one crate is indexed, and the uniqueness rule settles it — and may
/// land on the root's own `lib.rs`, since an item re-exported there is still
/// written `crate::x`.
///
/// `self::` and `super::` are tried strictly *below* the module the citing file
/// belongs to, and never resolve to that module itself. Extraction does not
/// record whether the path was written inside a `mod { ... }` block, and that is
/// exactly what decides: `use super::*` at the top of `a/b.rs` means `a`, while
/// the same line inside an inline `mod tests` means `b`. Nothing here can tell
/// those apart, so a path naming the anchored module itself resolves to nothing
/// rather than to whichever reading happens to exist as a file.
///
/// A path starting anywhere else is not anchored and gets nothing here.
fn anchored_candidates(written: &str, from: &str, paths: &PathIndex) -> Vec<String> {
    let (root, rest) = written.split_once("::").unwrap_or((written, ""));
    let relative = to_path(rest);

    let mut matched: Vec<String> = match root {
        "crate" => paths
            .crate_roots
            .iter()
            .flat_map(|anchor| under_or_anchor(anchor, &relative, paths))
            .collect(),
        "self" => under(&module_dir(from), &relative, paths),
        "super" => parent_module(from)
            .map(|anchor| under(&anchor, &relative, paths))
            .unwrap_or_default(),
        _ => return Vec::new(),
    };
    matched.sort();
    matched.dedup();
    matched
}

/// Candidates for `relative` under `anchor`, dropping trailing segments — which
/// name items rather than files — until something matches. The anchor itself is
/// never shortened and never matched, so an anchored path can only ever reach a
/// file *below* the module it named.
fn under(anchor: &str, relative: &str, paths: &PathIndex) -> Vec<String> {
    let mut prefix = relative;
    while !prefix.is_empty() {
        if let Some(matched) = matching_spelling(&join(anchor, prefix), paths) {
            return matched;
        }
        prefix = prefix.rsplit_once('/').map_or("", |(head, _)| head);
    }
    Vec::new()
}

/// [`under`], falling back to the anchor's own module file. Only for an anchor
/// that one written path can mean, which is a crate root and nothing else.
fn under_or_anchor(anchor: &str, relative: &str, paths: &PathIndex) -> Vec<String> {
    let below = under(anchor, relative, paths);
    if below.is_empty() {
        return matching_spelling(anchor, paths).unwrap_or_default();
    }
    below
}

/// File node ids matched by a path that does not say where it starts.
///
/// The last segment of a `use`/`import` path is usually the *item*, not the
/// file: `crate::context::graph_store::GraphStore` names a type inside
/// `context/graph_store.rs`. So when no spelling of the full path matches, the
/// trailing segment is dropped and the spellings are tried again, down to a
/// single segment. Truncation only ever widens the candidate set, and the
/// uniqueness rule still decides: an over-short prefix like `mod` matches many
/// files and therefore resolves nothing.
fn suffix_candidates(written: &str, paths: &PathIndex) -> Vec<String> {
    let base = normalize_module_path(written, paths);
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

/// Reduce a module path as written to a project-relative path shape, covering the
/// separators the extractors emit: Rust `crate::a::b`, Python `a.b.c`, TypeScript
/// `./a/b`, Go `mod/a/b`. A path carrying an explicit extension (`./a/b.js`) is
/// mangled by the dot conversion and simply fails to match — the safe outcome.
fn normalize_module_path(written: &str, paths: &PathIndex) -> String {
    let trimmed = written.trim_start_matches(['.', '/', '@']);
    strip_foreign_root(&to_path(trimmed), paths).to_string()
}

/// Convert the separators a module path is written with into path separators.
fn to_path(written: &str) -> String {
    written.replace("::", "/").replace('.', "/")
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

/// The directory a module path written inside `file` starts from: the file's own
/// directory when it is a module root, and the directory the file names
/// otherwise — `src/a.rs` is module `a`, whose children live in `src/a/`.
fn module_dir(file: &str) -> String {
    let name = last_segment(file);
    if MODULE_ROOT_FILES.contains(&name) {
        return directory_of(file).to_string();
    }
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    join(directory_of(file), stem)
}

/// The directory one module above `file`, or `None` at the top of the tree.
fn parent_module(file: &str) -> Option<String> {
    module_dir(file)
        .rsplit_once('/')
        .map(|(head, _)| head.to_string())
}
