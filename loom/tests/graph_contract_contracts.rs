//! Contracts for stage graph-contract: the evidence model and dialect registry.
//!
//! Each test pins one rule of `doc/plans/briefs/source-graph-mechanism/design.md`
//! (sections 3, 4 and 6.2, plus the schema-versioned base layer of section 2).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use loom::context::extract::{extract_file, registry, FileExtraction};
use loom::context::graph_store::GraphStore;
use loom::context::refresh::{ensure_snapshot, SnapshotAction, SnapshotPolicy};
use loom::context::source_graph::{
    EdgeProvenance, SourceEdge, SourceEdgeKind, SourceNodeKind, GRAPH_SCHEMA_VERSION,
    UNRESOLVED_TARGET,
};
use loom::context::store::ContextStore;
use loom::fs::work_dir::WorkDir;
use tempfile::TempDir;

#[path = "test_support/mod.rs"]
mod test_support;

fn extract_lib(source: &str) -> FileExtraction {
    extract_file(&registry(), Path::new("src/lib.rs"), source.as_bytes())
}

/// Every Calls edge leaving `from` whose symbol is `symbol`.
fn calls_from<'a>(extraction: &'a FileExtraction, from: &str, symbol: &str) -> Vec<&'a SourceEdge> {
    extraction
        .edges
        .iter()
        .filter(|e| e.kind == SourceEdgeKind::Calls && e.from == from && e.symbol == symbol)
        .collect()
}

/// The single Calls edge leaving `from` for `symbol`; panics on zero or many.
fn only_call<'a>(extraction: &'a FileExtraction, from: &str, symbol: &str) -> &'a SourceEdge {
    let edges = calls_from(extraction, from, symbol);
    assert_eq!(
        edges.len(),
        1,
        "expected exactly one Calls edge {from} -> {symbol}, got {edges:#?}"
    );
    edges[0]
}

fn assert_unbound(edge: &SourceEdge, candidates: &[&str]) {
    assert_eq!(edge.provenance, EdgeProvenance::Syntax, "edge: {edge:#?}");
    assert_eq!(edge.to, UNRESOLVED_TARGET, "edge: {edge:#?}");
    assert_eq!(edge.candidates, candidates, "edge: {edge:#?}");
    assert!(edge.confidence < 1.0, "edge: {edge:#?}");
}

#[test]
fn ambiguous_local_call_keeps_candidates() {
    let extraction = extract_lib(
        "mod a { pub fn helper() {} }\nmod b { pub fn helper() {} }\nfn run() { helper(); }\n",
    );

    let edge = only_call(&extraction, "src/lib.rs#function:run", "helper");

    assert_unbound(
        edge,
        &[
            "src/lib.rs#function:a::helper",
            "src/lib.rs#function:b::helper",
        ],
    );
}

#[test]
fn repeated_calls_keep_every_site() {
    let extraction = extract_lib("fn helper() {}\nfn run() {\n    helper();\n    helper();\n}\n");

    let edge = only_call(&extraction, "src/lib.rs#function:run", "helper");

    let lines: Vec<usize> = edge.sites.iter().map(|site| site.line_start).collect();
    assert_eq!(lines, vec![3, 4], "edge: {edge:#?}");
    assert_eq!(edge.to, "src/lib.rs#function:helper", "edge: {edge:#?}");
    assert_eq!(
        edge.provenance,
        EdgeProvenance::LocalName,
        "edge: {edge:#?}"
    );
    assert!((edge.confidence - 0.8).abs() < 1e-6, "edge: {edge:#?}");
}

/// Ids of the Function nodes whose scope is `[W, from]`, checked against design 4.
fn from_ids(source: &str) -> BTreeSet<String> {
    let extraction = extract_lib(source);
    let pattern = regex::Regex::new(r"^src/lib\.rs#function:W::from@[0-9a-f]{8}$").unwrap();
    let nodes: Vec<_> = extraction
        .nodes
        .iter()
        .filter(|n| n.kind == SourceNodeKind::Function && n.scope == ["W", "from"])
        .collect();
    assert_eq!(nodes.len(), 2, "nodes: {nodes:#?}");
    for node in &nodes {
        assert!(
            pattern.is_match(&node.id),
            "undisambiguated id: {}",
            node.id
        );
        assert_eq!(
            node.symbol_key, "src/lib.rs#function:W::from",
            "node: {node:#?}"
        );
    }
    let ids: BTreeSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
    assert_eq!(ids.len(), 2, "ids collide: {ids:?}");
    ids
}

#[test]
fn duplicate_declarations_get_distinct_ids() {
    let from_u8 = "impl From<u8> for W { fn from(välue: u8) -> Self { W } }\n";
    let from_u16 = "impl From<u16> for W { fn from(välue: u16) -> Self { W } }\n";

    let original = from_ids(&format!("pub struct W;\n{from_u8}{from_u16}"));
    let swapped = from_ids(&format!("pub struct W;\n{from_u16}{from_u8}"));

    assert_eq!(original, swapped, "ids depend on declaration order");
}

/// Every file under `dir`, recursively.
fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn only_containment_is_certain() {
    let root = Path::new("tests/fixtures/source");
    let mut files = Vec::new();
    files_under(root, &mut files);
    let extractors = registry();

    let mut dialects = BTreeSet::new();
    let mut calls = 0usize;
    for file in &files {
        let relative = file.strip_prefix(root).unwrap();
        dialects.insert(relative.components().next().unwrap().as_os_str().to_owned());
        let bytes = std::fs::read(file).unwrap();
        let extraction = extract_file(&extractors, relative, &bytes);
        for edge in &extraction.edges {
            if edge.kind == SourceEdgeKind::Calls {
                calls += 1;
            }
            if edge.confidence >= 1.0 {
                assert!(
                    edge.kind == SourceEdgeKind::Contains
                        && edge.provenance == EdgeProvenance::Structural,
                    "{}: only structural containment may be certain: {edge:#?}",
                    file.display()
                );
            }
        }
    }

    assert!(
        dialects.len() >= 4,
        "dialect directories read: {dialects:?}"
    );
    assert!(calls > 0, "no Calls edge in any fixture");
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A temp git repo with one committed `src/lib.rs` and a `.loom/work` state dir.
fn committed_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git(root, &["init"]);
    git(root, &["config", "user.name", "t"]);
    git(root, &["config", "user.email", "t@t"]);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "fn helper() {}\nfn run() { helper(); }\n",
    )
    .unwrap();
    git(root, &["add", "src/lib.rs"]);
    git(root, &["commit", "-m", "seed"]);
    test_support::write_main_checkout_identity(root, &root.join(".loom/work"));
    temp
}

#[test]
fn corrupt_base_layer_is_rebuilt() {
    let temp = committed_repo();
    let root = temp.path();
    let work_dir = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    store.ensure().unwrap();
    let graph_store = GraphStore::new(store.root(), work_dir.root());

    let first = ensure_snapshot(&store, &graph_store, root, SnapshotPolicy::BaseOnly);
    assert_ne!(
        first.action,
        SnapshotAction::Unavailable,
        "first: {first:#?}"
    );
    let head = first.revision.clone();
    let base = graph_store.base_path(&head);
    assert!(base.is_file(), "no base layer at {}", base.display());
    std::fs::write(&base, b"not json").unwrap();

    let second = ensure_snapshot(&store, &graph_store, root, SnapshotPolicy::BaseOnly);

    assert_eq!(
        second.action,
        SnapshotAction::Rebuilt,
        "second: {second:#?}"
    );
    let layer = graph_store
        .load_base(&head)
        .expect("rebuilt base layer parses")
        .expect("rebuilt base layer exists");
    assert_eq!(layer.schema_version, GRAPH_SCHEMA_VERSION);
}

#[test]
fn out_of_scope_or_imported_name_is_not_bound_locally() {
    let imported = extract_lib(
        "mod hidden {\n    pub fn helper() {}\n}\nuse external_crate::helper;\nfn run() {\n    helper();\n}\n",
    );
    assert_unbound(
        only_call(&imported, "src/lib.rs#function:run", "helper"),
        &[],
    );

    let hidden =
        extract_lib("mod hidden {\n    pub fn helper() {}\n}\nfn run() {\n    helper();\n}\n");
    assert_unbound(
        only_call(&hidden, "src/lib.rs#function:run", "helper"),
        &["src/lib.rs#function:hidden::helper"],
    );

    let lexical = extract_lib(
        "mod m {\n    pub fn helper() {}\n    pub fn run() {\n        helper();\n    }\n}\n",
    );
    let edge = only_call(&lexical, "src/lib.rs#function:m::run", "helper");
    assert_eq!(
        edge.provenance,
        EdgeProvenance::LocalName,
        "edge: {edge:#?}"
    );
    assert_eq!(edge.to, "src/lib.rs#function:m::helper", "edge: {edge:#?}");
    assert!((edge.confidence - 0.8).abs() < 1e-6, "edge: {edge:#?}");
}
