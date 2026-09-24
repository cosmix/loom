//! Libtest paths of Rust test functions, as `cargo test -- --list` prints
//! them, derived from the `mod` declarations the source graph records.
//!
//! A function's path is its file's module path from the crate root, then its
//! enclosing inline modules and its name. A file's module path follows the
//! declaration that makes it a module: the conventional parent file first,
//! then a `#[path]` attribute naming it from its directory or the one above.
//! A step no declaration supports leaves the path underived; none is guessed.

use regex::Regex;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use crate::context::graph_store::{FileEntry, ResolvedGraph};
use crate::context::source_graph::{
    file_node_id, NodeLanguage, SourceNode, SourceNodeKind, MAX_EXTRACTED_FILE_BYTES,
};
use crate::fs::safe_read::read_bounded;
use crate::verify::contracts::normalize;

/// `mod` declarations followed from a file towards its crate root before the
/// derivation gives up, so a `#[path]` cycle ends.
const MAX_MODULE_DEPTH: usize = 64;

/// A `#[path = "<file>"]` attribute on a `mod <name>;` declaration, with other
/// attributes and a visibility allowed in between.
static PATH_ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"#\[path\s*=\s*"([^"]+)"\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;"#,
    )
    .expect("Invalid regex")
});

/// Libtest paths over one graph; each file's module path is derived once.
pub(super) struct LibtestPaths<'a> {
    graph: &'a ResolvedGraph,
    /// The checkout the graph's paths are relative to.
    root: &'a Path,
    memo: BTreeMap<String, Option<Vec<String>>>,
}

impl<'a> LibtestPaths<'a> {
    pub(super) fn new(graph: &'a ResolvedGraph, root: &'a Path) -> Self {
        Self {
            graph,
            root,
            memo: BTreeMap::new(),
        }
    }

    /// The libtest path of Rust function `node` in the package at `package`
    /// (checkout-relative), or `None` when a step towards its crate root is
    /// not a module declaration the graph records.
    pub(super) fn of(&mut self, node: &SourceNode, package: &Path) -> Option<String> {
        if node.language != NodeLanguage::Rust {
            return None;
        }
        let graph = self.graph;
        let file = file_node_id(&node.path);
        let entry = graph.files.get(&file)?;
        let enclosing = node.scope.len().checked_sub(1)?;
        if !(1..=enclosing).all(|depth| declares_module(entry, &node.scope[..depth])) {
            return None;
        }
        let mut path = self.module_path(&file, package, 0)?;
        path.extend(node.scope.iter().cloned());
        Some(path.join("::"))
    }

    fn module_path(&mut self, file: &str, package: &Path, depth: usize) -> Option<Vec<String>> {
        if let Some(known) = self.memo.get(file) {
            return known.clone();
        }
        let path = if depth < MAX_MODULE_DEPTH {
            self.derive_module_path(file, package, depth)
        } else {
            None
        };
        self.memo.insert(file.to_string(), path.clone());
        path
    }

    fn derive_module_path(
        &mut self,
        file: &str,
        package: &Path,
        depth: usize,
    ) -> Option<Vec<String>> {
        if is_crate_root(Path::new(file).strip_prefix(package).ok()?) {
            return Some(Vec::new());
        }
        let (parent, name) = self.declaring_file(file)?;
        let mut path = self.module_path(&parent, package, depth + 1)?;
        path.push(name);
        Some(path)
    }

    /// The file whose `mod` declaration makes `file` a module, and the name
    /// it declares.
    fn declaring_file(&self, file: &str) -> Option<(String, String)> {
        let path = Path::new(file);
        let dir = path.parent()?;
        let stem = path.file_stem()?.to_str()?;
        let (name, outer) = if stem == "mod" {
            (dir.file_name()?.to_str()?, dir.parent()?)
        } else {
            (stem, dir)
        };
        let scope = [name.to_string()];
        let parents = [
            outer.with_extension("rs"),
            outer.join("mod.rs"),
            outer.join("lib.rs"),
            outer.join("main.rs"),
        ];
        let conventional = parents
            .iter()
            .map(|parent| file_node_id(parent))
            .find(|parent| {
                self.graph
                    .files
                    .get(parent)
                    .is_some_and(|entry| declares_module(entry, &scope))
            });
        match conventional {
            Some(parent) => Some((parent, name.to_string())),
            None => self.path_attribute_declaration(path),
        }
    }

    /// A Rust file in `file`'s directory or the one above whose `#[path]`
    /// attribute resolves to `file`, and the module it declares there.
    fn path_attribute_declaration(&self, file: &Path) -> Option<(String, String)> {
        let dirs = [file.parent(), file.parent().and_then(Path::parent)];
        let target = file_node_id(file);
        self.graph
            .files
            .keys()
            .filter(|key| {
                key.ends_with(".rs")
                    && key.as_str() != target
                    && dirs.contains(&Path::new(key.as_str()).parent())
            })
            .find_map(|key| {
                let name = self.declared_by_attribute(key, &target)?;
                Some((key.clone(), name))
            })
    }

    /// The module `declarer` declares with a `#[path]` attribute naming `target`.
    fn declared_by_attribute(&self, declarer: &str, target: &str) -> Option<String> {
        let entry = self.graph.files.get(declarer)?;
        let bytes = read_bounded(self.root, Path::new(declarer), MAX_EXTRACTED_FILE_BYTES).ok()?;
        let text = String::from_utf8(bytes).ok()?;
        let base = Path::new(declarer).parent()?;
        PATH_ATTRIBUTE.captures_iter(&text).find_map(|captures| {
            let name = captures.get(2)?.as_str();
            let resolved = normalize(&base.join(captures.get(1)?.as_str()).to_string_lossy());
            (resolved == target && declares_module(entry, &[name.to_string()]))
                .then(|| name.to_string())
        })
    }
}

/// Whether `relative` (to its package) is a crate root by Cargo's layout: the
/// library, a binary, an integration test, a bench, an example, or the build
/// script.
fn is_crate_root(relative: &Path) -> bool {
    let parts: Vec<&str> = relative.iter().filter_map(|part| part.to_str()).collect();
    matches!(
        parts.as_slice(),
        ["src", "lib.rs" | "main.rs"]
            | ["build.rs"]
            | ["src", "bin", _]
            | ["src", "bin", _, "main.rs"]
            | ["tests" | "benches" | "examples", _]
            | ["tests" | "benches" | "examples", _, "main.rs"]
    )
}

/// Whether `entry` declares a module whose scope is `scope`.
fn declares_module(entry: &FileEntry, scope: &[String]) -> bool {
    entry
        .nodes
        .iter()
        .any(|node| node.kind == SourceNodeKind::Module && node.scope == scope)
}
