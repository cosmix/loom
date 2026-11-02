//! Contracts for stage resolved-view: the persisted resolved view.
//!
//! Each test pins one rule of section 12 of
//! `doc/plans/briefs/source-graph-mechanism/design.md`: an incremental relink
//! equals the cold build byte for byte, and a corrupt view file is rebuilt,
//! never served.

#[path = "test_support/mod.rs"]
mod test_support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use loom::context::extract::{extract_file, registry};
use loom::context::graph_store::{FileEntry, GraphStore, ResolvedGraph};
use loom::context::refresh::{ensure_snapshot, SnapshotAction, SnapshotPolicy};
use loom::context::source_graph::{SourceEdge, SourceEdgeKind};
use loom::context::store::ContextStore;
use loom::context::view::{
    build_cold, canonical_bytes, relink, ResolvedView, ViewIdentity, RESOLVER_VERSION,
};
use loom::fs::work_dir::WorkDir;
use tempfile::TempDir;

const A_RS: (&str, &str) = ("src/a.rs", "pub fn helper() {}\n");
const B_RS: (&str, &str) = ("src/b.rs", "pub fn run() {\n    helper();\n}\n");
const C_RS: (&str, &str) = ("src/c.rs", "pub fn helper() {}\n");
const A_HELPER: &str = "src/a.rs#function:helper";

/// Extracts every `(path, source)` pair into a graph holding extraction-time
/// edges only; `build_cold` and `relink` resolve it themselves.
fn extracted(revision: &str, files: &[(&str, &str)]) -> ResolvedGraph {
    let extractors = registry();
    let mut entries = BTreeMap::new();
    for (path, source) in files {
        let bytes = source.as_bytes();
        let extraction = extract_file(&extractors, Path::new(path), bytes);
        entries.insert(
            (*path).to_string(),
            FileEntry::from_extraction(bytes, extraction),
        );
    }
    ResolvedGraph {
        base_revision: revision.to_string(),
        overlaid: BTreeSet::new(),
        files: entries,
    }
}

/// The single Calls edge from `src/b.rs`'s `run` to `helper`.
fn run_calls_helper(view: &ResolvedView) -> &SourceEdge {
    let edges: Vec<&SourceEdge> = view
        .graph
        .files
        .values()
        .flat_map(|entry| entry.edges.iter())
        .filter(|e| {
            e.kind == SourceEdgeKind::Calls
                && e.symbol == "helper"
                && e.from.starts_with("src/b.rs")
        })
        .collect();
    assert_eq!(
        edges.len(),
        1,
        "expected one run -> helper call, got {edges:#?}"
    );
    edges[0]
}

fn identity(revision: &str) -> ViewIdentity {
    let id = ViewIdentity::current(revision, "");
    assert_eq!(id.resolver_version, RESOLVER_VERSION);
    assert_eq!(id.base_revision, revision);
    id
}

/// Relinks `v0` onto `next` and asserts it equals the cold build of `next`.
fn assert_relink_equals_cold(v0: &ResolvedView, next: &[(&str, &str)]) -> ResolvedView {
    let id1 = identity("r1");
    let relinked = relink(v0, extracted("r1", next), id1.clone());
    let cold = build_cold(extracted("r1", next), id1);
    let relinked_bytes = canonical_bytes(&relinked).unwrap();
    let cold_bytes = canonical_bytes(&cold).unwrap();
    assert!(
        relinked_bytes == cold_bytes,
        "relink differs from cold build\nrelink: {}\ncold:   {}",
        String::from_utf8_lossy(&relinked_bytes),
        String::from_utf8_lossy(&cold_bytes)
    );
    relinked
}

#[test]
fn relink_equals_cold_build_after_namesake_added() {
    let v0 = build_cold(extracted("r0", &[A_RS, B_RS]), identity("r0"));
    assert_eq!(
        run_calls_helper(&v0).to,
        A_HELPER,
        "the unique helper binds in the cold build of G0"
    );

    let relinked = assert_relink_equals_cold(&v0, &[A_RS, B_RS, C_RS]);

    assert_ne!(
        run_calls_helper(&relinked).to,
        A_HELPER,
        "helper is ambiguous once src/c.rs declares a namesake"
    );
}

#[test]
fn relink_equals_cold_build_after_target_removed() {
    let v0 = build_cold(extracted("r0", &[A_RS, B_RS]), identity("r0"));
    assert_eq!(run_calls_helper(&v0).to, A_HELPER);

    let relinked = assert_relink_equals_cold(&v0, &[B_RS]);

    assert_ne!(
        run_calls_helper(&relinked).to,
        A_HELPER,
        "the call still targets a node that no longer exists"
    );
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
fn corrupt_view_file_is_rebuilt_not_served() {
    let temp = committed_repo();
    let root = temp.path();
    let work_dir = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    store.ensure().unwrap();
    let graph_store = GraphStore::new(store.root(), work_dir.root());

    let snapshot = ensure_snapshot(&store, &graph_store, root, SnapshotPolicy::BaseOnly);
    assert_ne!(
        snapshot.action,
        SnapshotAction::Unavailable,
        "snapshot: {snapshot:#?}"
    );
    let head = snapshot.revision.clone();
    let path = graph_store.view_path(&ViewIdentity::current(&head, ""), None);
    assert!(path.is_file(), "no materialized view at {}", path.display());
    std::fs::write(&path, b"not json").unwrap();

    // A fresh store stands in for a second process: its in-process view cache
    // is empty, so the corrupt file is what it finds.
    let reader = GraphStore::new(store.root(), work_dir.root());
    let view = reader
        .view(&head, None)
        .expect("a corrupt view file is rebuilt, not an error");

    assert_eq!(view.identity.base_revision, head);
    let nodes: usize = view.graph.files.values().map(|e| e.nodes.len()).sum();
    assert!(nodes > 0, "rebuilt view has no nodes");
    let bytes = std::fs::read(&path).unwrap();
    let reparsed: ResolvedView = serde_json::from_slice(&bytes).unwrap_or_else(|err| {
        panic!(
            "view file at {} still does not parse: {err}",
            path.display()
        )
    });
    assert_eq!(reparsed.identity, view.identity);
}
