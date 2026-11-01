//! Rust module paths.
//!
//! Three rules, in order. A path that says where it starts from — `crate::`,
//! `self::`, `super::` — is **anchored**: it is joined onto a crate root or onto
//! the module the citing file belongs to, and only files under that anchor can
//! match. When the anchor cannot be established, the path stays unresolved.
//!
//! Any other path (`x::y`) is internal only when its first segment names a module
//! file (`x.rs`, `x/mod.rs`) or directory under the citing file's own crate root
//! (the deepest crate-root directory holding it) or under the citing module's
//! directory. Otherwise the import is external: `use serde::de::*` is never the
//! `de.rs` of this project, and in a workspace `use log::info;` in one crate is
//! never another crate's `log.rs`.
//!
//! Package names from Cargo manifests are not indexed, so an integration test's
//! own-package imports cannot be distinguished from external imports.
//!
//! Anchoring is what separates two files of the same name: `crate::codex` is
//! `<crate root>/codex.rs` and never the `codex.rs` sitting four directories
//! deeper. A relative anchor is weaker than a rooted one and is used as such:
//! `self::` and `super::` reach files *below* the module they name but never that
//! module itself, because the same line means different modules at the top of a
//! file and inside an inline `mod` block, and extraction does not record which.

use std::collections::BTreeSet;

use super::{directory_of, join, last_segment, prefixes, PathIndex};

const FAMILY: &str = "rust";

/// Spellings tried after the bare module path, in order. The first form matching
/// anything decides; later forms are never consulted.
///
/// The `/lib.rs` and `/main.rs` forms are what let a directory match as a crate
/// root, so an item written `crate::x` and defined in `lib.rs` itself is
/// reachable; they stay last so a module file is always preferred.
const MODULE_SUFFIXES: [&str; 4] = [".rs", "/mod.rs", "/lib.rs", "/main.rs"];

/// Path roots that say where the path starts from.
const ANCHORS: [&str; 3] = ["crate", "self", "super"];

/// Files that are the module they sit in rather than a module below it, so a
/// path written inside them starts from their own directory.
const MODULE_ROOT_FILES: [&str; 3] = ["mod.rs", "lib.rs", "main.rs"];

pub(super) fn module_files(paths: &PathIndex, spec: &str, from: &str) -> Vec<String> {
    let written = written_path(spec);
    let (root, rest) = written.split_once("::").unwrap_or((written, ""));
    if !ANCHORS.contains(&root) {
        return unanchored(paths, &to_path(written), from);
    }
    anchored(paths, root, &to_path(rest), from)
}

/// The path part of an import as written: everything before a brace group, a
/// glob, or an `as` clause, none of which name a file. A `use` path can arrive
/// wrapped across lines, so whitespace ends the path too.
fn written_path(spec: &str) -> &str {
    let end = spec
        .find(['{', '*', ' ', '\t', '\n', '\r'])
        .unwrap_or(spec.len());
    spec[..end].trim_end_matches(':')
}

fn to_path(written: &str) -> String {
    written.replace("::", "/")
}

/// Candidates for a path that names where it starts from.
///
/// `crate::` uses the citing file's own crate root and may land on its `lib.rs`.
///
/// `self::` and `super::` are tried strictly *below* the module the citing file
/// belongs to, and never resolve to that module itself.
fn anchored(paths: &PathIndex, root: &str, relative: &str, from: &str) -> Vec<String> {
    let mut matched: Vec<String> = match root {
        "crate" => own_crate_root(paths, from)
            .map(|anchor| under_or_anchor(paths, anchor, relative))
            .unwrap_or_default(),
        "self" => under(paths, &module_dir(from), relative),
        _ => parent_module(from)
            .map(|anchor| under(paths, &anchor, relative))
            .unwrap_or_default(),
    };
    matched.sort();
    matched.dedup();
    matched
}

/// Candidates for a path whose first segment is not an anchor keyword: the path
/// read from the citing file's own crate root and from the citing module's own
/// directory. Each probe is exact, so a first segment that is no module file or
/// directory under either anchor — an external crate — matches nothing.
fn unanchored(paths: &PathIndex, relative: &str, from: &str) -> Vec<String> {
    let crate_root = own_crate_root(paths, from).map(str::to_string);
    let anchors: BTreeSet<String> = crate_root.into_iter().chain([module_dir(from)]).collect();
    let mut matched: Vec<String> = anchors
        .iter()
        .flat_map(|anchor| under(paths, anchor, relative))
        .collect();
    matched.sort();
    matched.dedup();
    matched
}

/// The crate-root directory `file` belongs to: the deepest one holding it.
/// `None` for a file outside every crate root — an integration test, bench,
/// example or build script, or any file of a graph with no crate root.
pub(super) fn own_crate_root<'a>(paths: &'a PathIndex, file: &str) -> Option<&'a str> {
    paths
        .crate_roots
        .iter()
        .filter(|root| is_within(file, root))
        .max_by_key(|root| root.len())
        .map(String::as_str)
}

/// Whether `file` lies in `dir` or below it, compared on path components. The
/// empty directory is the repository root and holds every file.
fn is_within(file: &str, dir: &str) -> bool {
    dir.is_empty()
        || file
            .strip_prefix(dir)
            .is_some_and(|tail| tail.starts_with('/'))
}

/// Candidates for `relative` under `anchor`, dropping trailing segments — which
/// name items rather than files — until something matches. The anchor itself is
/// never shortened and never matched, so an anchored path can only ever reach a
/// file *below* the module it named.
fn under(paths: &PathIndex, anchor: &str, relative: &str) -> Vec<String> {
    prefixes(relative, 1)
        .filter(|prefix| !prefix.is_empty())
        .map(|prefix| paths.first_exact(FAMILY, &spellings(&join(anchor, prefix))))
        .find(|matched| !matched.is_empty())
        .unwrap_or_default()
}

/// [`under`], falling back to the anchor's own module file. Only for an anchor
/// that one written path can mean, which is a crate root and nothing else.
fn under_or_anchor(paths: &PathIndex, anchor: &str, relative: &str) -> Vec<String> {
    let below = under(paths, anchor, relative);
    if below.is_empty() {
        return paths.first_exact(FAMILY, &spellings(anchor));
    }
    below
}

/// Every spelling a module path `base` can take as a file.
fn spellings(base: &str) -> Vec<String> {
    if base.is_empty() {
        return Vec::new();
    }
    MODULE_SUFFIXES
        .iter()
        .map(|suffix| format!("{base}{suffix}"))
        .collect()
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
