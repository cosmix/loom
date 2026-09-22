//! Directory cluster partition and content digests for knowledge bootstrap.
//!
//! `loom knowledge bootstrap` briefs an interactive session directory by
//! directory rather than file by file, so a repository's files are grouped
//! into [`Cluster`]s here, each with a content [`Cluster::digest`] that lets
//! `--refresh` skip a cluster nothing has touched since the last completed
//! run. Everything in this module is pure: it reads a [`ResolvedGraph`] and
//! returns data, with no I/O of its own.

use std::collections::{BTreeMap, HashMap};

use crate::context::graph_store::ResolvedGraph;
use crate::context::source_graph::{body_hash, FileCoverage, SourceNodeKind};

/// The knowledge tree and its receipt are outputs of bootstrap, never inputs.
pub(super) const KNOWLEDGE_PREFIX: &str = "doc/loom/knowledge/";

/// A directory holding at most this many files forms one cluster; past it,
/// [`partition`] splits by child directory.
pub(super) const MAX_CLUSTER_FILES: usize = 40;
/// A child directory with fewer files than this folds into its parent's
/// residual cluster instead of forming its own.
pub(super) const MIN_CHILD_CLUSTER_FILES: usize = 8;

/// One file's facts, extracted from the resolved graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileFacts {
    /// Repo-relative, '/'-separated path.
    pub path: String,
    /// `FileEntry::content_hash`.
    pub content_hash: String,
    /// Symbol node count, excluding the whole-file node every entry carries.
    pub symbols: usize,
}

/// A directory-scoped group of files, briefed to the interactive session as
/// one unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Cluster {
    /// Repo-relative directory, "." for root.
    pub id: String,
    /// Sorted member paths.
    pub files: Vec<String>,
    pub symbols: usize,
    /// `"sha256:<hex>"` over the cluster's file paths and content hashes.
    pub digest: String,
    /// Top 3 member files by cross-file fan-in, fan-in > 0 only, ties by path.
    pub hot_files: Vec<(String, usize)>,
}

/// Facts for every graph file whose coverage status is not "deleted" and
/// whose path does not start with [`KNOWLEDGE_PREFIX`], sorted by path.
pub(super) fn file_facts(graph: &ResolvedGraph) -> Vec<FileFacts> {
    graph
        .files
        .iter()
        .filter(|(path, entry)| {
            entry.coverage != FileCoverage::Deleted && !path.starts_with(KNOWLEDGE_PREFIX)
        })
        .map(|(path, entry)| FileFacts {
            path: path.clone(),
            content_hash: entry.content_hash.clone(),
            symbols: entry
                .nodes
                .iter()
                .filter(|node| node.kind != SourceNodeKind::File)
                .count(),
        })
        .collect()
}

/// Cross-file fan-in: for each edge whose two endpoints both resolve to a
/// file path, count it against the `to` path when the two paths differ.
/// Unresolved endpoints are skipped.
pub(super) fn fan_in(graph: &ResolvedGraph) -> BTreeMap<String, usize> {
    // A per-edge `graph.node()` lookup is a linear scan over every node in
    // the graph; build the id -> path map once instead.
    let mut owner: HashMap<&str, &str> = HashMap::new();
    for (path, entry) in &graph.files {
        for node in &entry.nodes {
            owner.insert(node.id.as_str(), path.as_str());
        }
    }

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for edge in graph.edges() {
        let from_path = owner.get(edge.from.as_str());
        let to_path = owner.get(edge.to.as_str());
        if let (Some(&from_path), Some(&to_path)) = (from_path, to_path) {
            if from_path != to_path {
                *counts.entry(to_path.to_string()).or_insert(0) += 1;
            }
        }
    }
    counts
}

/// Deterministic directory partition (see module-level rule), clusters
/// sorted by id.
pub(super) fn partition(files: &[FileFacts], fan_in: &BTreeMap<String, usize>) -> Vec<Cluster> {
    let mut root = DirNode::new(".");
    for file in files {
        root.insert(&file.path);
    }

    let mut raw = Vec::new();
    collect_clusters(&root, &mut raw);

    let facts_by_path: HashMap<&str, &FileFacts> = files
        .iter()
        .map(|fact| (fact.path.as_str(), fact))
        .collect();

    let mut clusters: Vec<Cluster> = raw
        .into_iter()
        .map(|(id, mut paths)| {
            paths.sort();
            let symbols = paths
                .iter()
                .filter_map(|path| facts_by_path.get(path.as_str()))
                .map(|fact| fact.symbols)
                .sum();
            let digest = cluster_digest(&paths, &facts_by_path);
            let hot_files = hot_files(&paths, fan_in);
            Cluster {
                id,
                files: paths,
                symbols,
                digest,
                hot_files,
            }
        })
        .collect();

    clusters.sort_by(|a, b| a.id.cmp(&b.id));
    clusters
}

/// Top 3 by fan-in descending, ties by path ascending, fan-in > 0 only.
fn hot_files(paths: &[String], fan_in: &BTreeMap<String, usize>) -> Vec<(String, usize)> {
    let mut hot: Vec<(String, usize)> = paths
        .iter()
        .filter_map(|path| {
            fan_in
                .get(path)
                .copied()
                .filter(|&count| count > 0)
                .map(|count| (path.clone(), count))
        })
        .collect();
    hot.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    hot.truncate(3);
    hot
}

/// `sha256:<hex>` over `"{path}\t{content_hash}\n"` for each file in path
/// order.
fn cluster_digest(paths: &[String], facts_by_path: &HashMap<&str, &FileFacts>) -> String {
    let mut buf = String::new();
    for path in paths {
        if let Some(fact) = facts_by_path.get(path.as_str()) {
            buf.push_str(path);
            buf.push('\t');
            buf.push_str(&fact.content_hash);
            buf.push('\n');
        }
    }
    body_hash(buf.as_bytes())
}

/// A directory in the partition trie: the files directly inside it, and its
/// immediate child directories keyed by name (a [`BTreeMap`] iterates in
/// name order, which the partition rule requires).
struct DirNode {
    id: String,
    files: Vec<String>,
    children: BTreeMap<String, DirNode>,
}

impl DirNode {
    fn new(id: impl Into<String>) -> Self {
        DirNode {
            id: id.into(),
            files: Vec::new(),
            children: BTreeMap::new(),
        }
    }

    /// Insert a file's full repo-relative `path` into the tree: `"foo.rs"`
    /// lands directly at the root, `"src/a/foo.rs"` walks (creating as
    /// needed) into child `"src"` then `"a"`. The full path is always what
    /// ends up in a node's `files`, never the shrinking directory suffix
    /// used to walk there.
    fn insert(&mut self, path: &str) {
        self.insert_remaining(path, path);
    }

    fn insert_remaining(&mut self, path: &str, remaining: &str) {
        match remaining.split_once('/') {
            None => self.files.push(path.to_string()),
            Some((head, rest)) => {
                let child_id = if self.id == "." {
                    head.to_string()
                } else {
                    format!("{}/{}", self.id, head)
                };
                self.children
                    .entry(head.to_string())
                    .or_insert_with(|| DirNode::new(child_id))
                    .insert_remaining(path, rest);
            }
        }
    }

    /// `n(d)`: total files anywhere under this node.
    fn total(&self) -> usize {
        self.files.len() + self.children.values().map(DirNode::total).sum::<usize>()
    }

    /// Every file path under this node.
    fn all_files(&self) -> Vec<String> {
        let mut out = self.files.clone();
        for child in self.children.values() {
            out.extend(child.all_files());
        }
        out
    }
}

/// Partition rule: if `n(d) <= MAX_CLUSTER_FILES`, all files under `d` form
/// one cluster. Otherwise, each child directory with `n(c) >=
/// MIN_CHILD_CLUSTER_FILES` is partitioned on its own; every other child's
/// files, plus `d`'s own direct files, join a residual cluster for `d`.
fn collect_clusters(node: &DirNode, out: &mut Vec<(String, Vec<String>)>) {
    if node.total() <= MAX_CLUSTER_FILES {
        let files = node.all_files();
        if !files.is_empty() {
            out.push((node.id.clone(), files));
        }
        return;
    }

    let mut residual = node.files.clone();
    for child in node.children.values() {
        if child.total() < MIN_CHILD_CLUSTER_FILES {
            residual.extend(child.all_files());
        } else {
            collect_clusters(child, out);
        }
    }
    if !residual.is_empty() {
        out.push((node.id.clone(), residual));
    }
}
