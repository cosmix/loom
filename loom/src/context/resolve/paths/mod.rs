//! Matching a module spec as written onto files in the graph, one convention per
//! dialect family.
//!
//! Every probe compares against file node ids already in the graph and never
//! touches the filesystem. A spec only ever matches files of the importer's own
//! family, so a verbatim `./styles.css` probe cannot land on a stylesheet and a
//! Python import cannot land on a Go file.
//!
//! What a spec names differs per family, and each lives in its own submodule:
//! Rust anchors on a crate root or the citing module, ECMAScript resolves
//! relative specifiers against the importing file, Go and Java map to package
//! directories, C# and PHP go through the namespace index, and Ruby and C
//! distinguish relative from search-path forms.
//!
//! Candidate lists are sorted and deduplicated: the resolver's "exactly one"
//! rules depend on it.
//!
//! Every lookup records the keys it consulted into the caller's set
//! (`pathset:{family}` for any path or package-scope lookup, `ns:{family}:{ns}`
//! for a namespace lookup), so an incremental relink can tell which edges a
//! changed file may have affected.

mod c;
mod csharp;
mod ecmascript;
mod go;
mod jvm;
mod php;
mod python;
mod ruby;
mod rust;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::context::extract::dialect::{dialect_for_path, DialectSpec};
use crate::context::graph_store::ResolvedGraph;
use crate::context::source_graph::SourceNodeKind;
use crate::fs::lexical::normalize_lexically;

/// Families whose files declare a namespace or package in a `Module` node.
const NAMESPACE_FAMILIES: [&str; 3] = ["java", "csharp", "php"];

/// Files with these names are crate roots, which is what `crate::` names.
const CRATE_ROOT_FILES: [&str; 2] = ["lib.rs", "main.rs"];

/// The lookup key recorded for any module-path or package-scope lookup.
pub(super) fn pathset_key(family: &str) -> String {
    format!("pathset:{family}")
}

/// The lookup key recorded for a namespace-index lookup.
pub(super) fn ns_key(family: &str, namespace: &str) -> String {
    format!("ns:{family}:{namespace}")
}

/// Everything a module spec can be matched against, built once per resolution.
#[derive(Debug, Default)]
pub(super) struct PathIndex {
    /// Every file node id.
    files: BTreeSet<String>,
    /// File ids bucketed by final path segment, so a suffix match does not scan
    /// every file in the graph.
    by_last_segment: BTreeMap<String, BTreeSet<String>>,
    /// Directory -> ids of the files directly inside it.
    dirs: BTreeMap<String, BTreeSet<String>>,
    /// Last segment of a directory -> directories ending in it.
    dirs_by_name: BTreeMap<String, BTreeSet<String>>,
    /// Directories holding a Rust crate root, where a `crate::` path starts.
    crate_roots: BTreeSet<String>,
    /// `(family, namespace)` -> ids of the files declaring it.
    namespaces: BTreeMap<(String, String), BTreeSet<String>>,
    /// File id -> namespaces it declares.
    declared: BTreeMap<String, BTreeSet<String>>,
}

impl PathIndex {
    pub(super) fn build(graph: &ResolvedGraph) -> Self {
        let mut index = PathIndex::default();
        for (path, entry) in &graph.files {
            for node in &entry.nodes {
                match node.kind {
                    SourceNodeKind::File => index.add_file(&node.id),
                    SourceNodeKind::Module => index.add_namespace(path, &node.scope),
                    _ => {}
                }
            }
        }
        index
    }

    fn add_file(&mut self, id: &str) {
        self.files.insert(id.to_string());
        let directory = directory_of(id);
        let bucket = self.by_last_segment.entry(last_segment(id).to_string());
        bucket.or_default().insert(id.to_string());
        self.dirs
            .entry(directory.to_string())
            .or_default()
            .insert(id.to_string());
        self.dirs_by_name
            .entry(last_segment(directory).to_string())
            .or_default()
            .insert(directory.to_string());
        if CRATE_ROOT_FILES.contains(&last_segment(id)) && family_of(id) == Some("rust") {
            self.crate_roots.insert(directory.to_string());
        }
    }

    /// Record that `file` declares the namespace a `Module` node's scope spells,
    /// when `file` belongs to a family with namespaces.
    fn add_namespace(&mut self, file: &str, scope: &[String]) {
        let Some(family) = family_of(file).filter(|f| NAMESPACE_FAMILIES.contains(f)) else {
            return;
        };
        let namespace = scope.join(".");
        if namespace.is_empty() {
            return;
        }
        let key = (family.to_string(), namespace.clone());
        self.namespaces
            .entry(key)
            .or_default()
            .insert(file.to_string());
        self.declared
            .entry(file.to_string())
            .or_default()
            .insert(namespace);
    }

    /// File node ids a module spec written in `from` names, only files whose
    /// dialect is in the importer's family. Empty means the spec names nothing in
    /// the graph, which is what an external dependency looks like.
    pub(super) fn module_files(
        &self,
        spec: &str,
        from: &str,
        dialect: &DialectSpec,
        keys: &mut BTreeSet<String>,
    ) -> Vec<String> {
        keys.insert(pathset_key(dialect.family));
        let spec = spec.trim();
        if spec.is_empty() {
            return Vec::new();
        }
        match dialect.family {
            "rust" => rust::module_files(self, spec, from),
            "ecmascript" => ecmascript::module_files(self, spec, from),
            "python" => python::module_files(self, spec, from),
            "go" => go::module_files(self, spec),
            "java" => jvm::module_files(self, spec),
            "csharp" => csharp::module_files(self, spec, keys),
            "ruby" => ruby::module_files(self, spec, from),
            "php" => php::module_files(self, spec, from, keys),
            "c" => c::module_files(self, spec, from),
            _ => Vec::new(),
        }
    }

    /// File node ids sharing `from`'s package scope, excluding `from` itself. Go
    /// and Java scope a package to a directory; C# and PHP to a namespace. No
    /// other family has a package scope beyond its imports, so asking for one
    /// finds nothing and records no key.
    pub(super) fn package_files(
        &self,
        from: &str,
        dialect: &DialectSpec,
        keys: &mut BTreeSet<String>,
    ) -> Vec<String> {
        let family = dialect.family;
        let files = match family {
            "go" | "java" => self.dir_files(directory_of(from), family),
            "csharp" | "php" => self.same_namespace_files(from, family, keys),
            _ => return Vec::new(),
        };
        keys.insert(pathset_key(family));
        files.into_iter().filter(|id| id != from).collect()
    }

    /// Whether a type declared in `from` can have parts declared in `other`,
    /// both files of `family`: within one crate for Rust, whose inherent
    /// `impl` blocks live in the type's own crate; within one namespace for
    /// C#, whose `partial` parts share it; anywhere for any other family.
    pub(super) fn may_share_type(
        &self,
        family: &str,
        from: &str,
        other: &str,
        keys: &mut BTreeSet<String>,
    ) -> bool {
        match family {
            "rust" => {
                keys.insert(pathset_key(family));
                rust::own_crate_root(self, from) == rust::own_crate_root(self, other)
            }
            "csharp" => match (self.declared.get(from), self.declared.get(other)) {
                // Neither declares a namespace: both are in the global one.
                (None, None) => true,
                (Some(ours), Some(theirs)) => !ours.is_disjoint(theirs),
                _ => false,
            },
            _ => true,
        }
    }

    /// File node ids declaring `namespace` in `family`.
    pub(super) fn namespace_files(
        &self,
        family: &str,
        namespace: &str,
        keys: &mut BTreeSet<String>,
    ) -> Vec<String> {
        keys.insert(ns_key(family, namespace));
        let key = (family.to_string(), namespace.to_string());
        self.namespaces
            .get(&key)
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }

    /// Files declaring any namespace `from` declares, `from` included.
    fn same_namespace_files(
        &self,
        from: &str,
        family: &str,
        keys: &mut BTreeSet<String>,
    ) -> Vec<String> {
        let files: BTreeSet<String> = self
            .declared
            .get(from)
            .into_iter()
            .flatten()
            .flat_map(|namespace| self.namespace_files(family, namespace, keys))
            .collect();
        files.into_iter().collect()
    }

    /// Ids of the `family` files directly inside `dir`.
    fn dir_files(&self, dir: &str, family: &str) -> Vec<String> {
        self.dirs
            .get(dir)
            .into_iter()
            .flatten()
            .filter(|id| family_of(id) == Some(family))
            .cloned()
            .collect()
    }

    /// Ids of the `family` files directly inside any directory equal to `tail` or
    /// ending in `/<tail>`.
    fn files_in_dirs_ending(&self, tail: &str, family: &str) -> Vec<String> {
        let suffix = format!("/{tail}");
        let dirs = self.dirs_by_name.get(last_segment(tail));
        let files: BTreeSet<String> = dirs
            .into_iter()
            .flatten()
            .filter(|dir| dir.as_str() == tail || dir.ends_with(&suffix))
            .flat_map(|dir| self.dir_files(dir, family))
            .collect();
        files.into_iter().collect()
    }

    /// The first candidate naming a `family` file exactly, as a one-element list.
    fn first_exact(&self, family: &str, candidates: &[String]) -> Vec<String> {
        candidates
            .iter()
            .find(|id| self.files.contains(id.as_str()) && family_of(id) == Some(family))
            .cloned()
            .into_iter()
            .collect()
    }

    /// Ids of `family` files equal to `candidate` or ending in `/<candidate>`,
    /// compared on path components, so `util.ts` never matches `myutil.ts`.
    fn suffix_matches(&self, candidate: &str, family: &str) -> Vec<String> {
        let suffix = format!("/{candidate}");
        let bucket = self.by_last_segment.get(last_segment(candidate));
        bucket
            .into_iter()
            .flatten()
            .filter(|id| id.as_str() == candidate || id.ends_with(&suffix))
            .filter(|id| family_of(id) == Some(family))
            .cloned()
            .collect()
    }

    /// The matches of the first candidate that suffix-matches anything.
    fn first_suffix(&self, family: &str, candidates: &[String]) -> Vec<String> {
        candidates
            .iter()
            .filter(|candidate| !candidate.is_empty() && !candidate.starts_with(".."))
            .map(|candidate| self.suffix_matches(candidate, family))
            .find(|matched| !matched.is_empty())
            .unwrap_or_default()
    }
}

/// File node ids matched by an import's module path, or by the qualifier of a
/// qualified call, by the conventions of the dialect of `from`. Empty when
/// nothing matched or `from` belongs to no dialect.
pub(super) fn import_candidates(
    symbol: &str,
    from: &str,
    paths: &PathIndex,
    keys: &mut BTreeSet<String>,
) -> Vec<String> {
    match dialect_for_path(Path::new(from)) {
        Some(dialect) => paths.module_files(symbol, from, dialect, keys),
        None => Vec::new(),
    }
}

/// The resolution family of the dialect owning `path`'s extension.
pub(super) fn family_of(path: impl AsRef<Path>) -> Option<&'static str> {
    dialect_for_path(path.as_ref()).map(|dialect| dialect.family)
}

fn last_segment(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn directory_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(head, _)| head)
}

/// Join a directory onto a relative path, tolerating an empty directory so a
/// file at the repository root anchors to the root rather than to `/`.
fn join(directory: &str, relative: &str) -> String {
    match (directory.is_empty(), relative.is_empty()) {
        (true, _) => relative.to_string(),
        (false, true) => directory.to_string(),
        (false, false) => format!("{directory}/{relative}"),
    }
}

/// `spec` resolved against the directory of `from`, for a spec written relative
/// to the importing file. `None` when it climbs above the repository root or, from a
/// file at the root, names an absolute path (which no repository-relative index holds).
fn relative_to(from: &str, spec: &str) -> Option<String> {
    let normal = normalize_lexically(Path::new(&join(directory_of(from), spec)))?;
    if normal.has_root() {
        return None;
    }
    normal.to_str().map(str::to_owned)
}

/// `path` and then, dropping one trailing segment at a time, each shorter prefix
/// that still has `min_segments` segments. Trailing segments of an import path
/// often name an item inside the file rather than the file.
fn prefixes(path: &str, min_segments: usize) -> impl Iterator<Item = &str> {
    let mut next = Some(path);
    std::iter::from_fn(move || {
        let current = next?;
        next = current
            .rsplit_once('/')
            .map(|(head, _)| head)
            .filter(|head| head.split('/').count() >= min_segments);
        Some(current)
    })
}

#[cfg(test)]
mod tests {
    use super::relative_to;

    #[test]
    fn a_spec_folds_against_the_importing_files_directory() {
        assert_eq!(
            relative_to("src/a/b.php", "../lib/x.php").as_deref(),
            Some("src/lib/x.php")
        );
        assert_eq!(
            relative_to("index.php", "./x.php").as_deref(),
            Some("x.php")
        );
    }

    #[test]
    fn a_spec_that_climbs_above_the_root_or_is_absolute_does_not_resolve() {
        assert_eq!(relative_to("src/a.php", "../../x.php"), None);
        assert_eq!(relative_to("index.php", "/etc/x.php"), None);
    }
}
