//! Contracts: a kept ignored file removed from the Git index is tombstoned in
//! the overlay, and paths marked `linguist-generated` never enter the graph.

use std::path::Path;

use tempfile::TempDir;

use crate::context::graph_store::GraphStore;
use crate::context::refresh::{reconcile_source_graph, SourceGraphScope};
use crate::context::source_graph::FileCoverage;
use crate::context::store::ContextStore;
use crate::test_git::{commit_file, git, TestRepo};

fn init_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    TestRepo::builder()
        .identity("t", "t@t.com")
        .no_seed()
        .init_in(temp.path());
    temp
}

fn stores(root: &Path) -> (ContextStore, GraphStore) {
    let state_root = root.join(".loom");
    let work = state_root.join("work");
    std::fs::create_dir_all(&work).unwrap();
    crate::git::worktree::capture_main_identity(root, &work).unwrap();
    let store = ContextStore::with_root(state_root.join("cache/context-v1"));
    let graph_store = GraphStore::new(store.root(), &work);
    (store, graph_store)
}

fn overlay() -> SourceGraphScope {
    SourceGraphScope::Overlay {
        plan: "p".to_string(),
        stage: "s".to_string(),
    }
}

fn reconcile_base(store: &ContextStore, graph_store: &GraphStore, root: &Path) -> String {
    let revision = git(root, &["rev-parse", "HEAD"]);
    let base = SourceGraphScope::Base {
        revision: revision.clone(),
    };
    reconcile_source_graph(store, graph_store, root, base).unwrap();
    revision
}

#[test]
fn ignored_file_removed_from_the_index_is_tombstoned() {
    let temp = init_repo();
    let root = temp.path();
    commit_file(root, "src.rs", "fn main() {}\n", "seed");
    let (store, graph_store) = stores(root);
    let revision = reconcile_base(&store, &graph_store, root);

    std::fs::write(root.join(".gitignore"), "src.rs\n").unwrap();
    git(root, &["rm", "--cached", "src.rs"]);
    assert!(
        root.join("src.rs").exists(),
        "git rm --cached keeps the file"
    );
    reconcile_source_graph(&store, &graph_store, root, overlay()).unwrap();

    let resolved = graph_store.resolved(&revision, Some(("p", "s"))).unwrap();
    assert!(
        !resolved.files.contains_key("src.rs"),
        "a kept ignored file removed from the index must not resolve through to its base entry"
    );
    let saved = graph_store.load_overlay("p", "s").unwrap().unwrap();
    assert_eq!(
        saved.files.get("src.rs").map(|entry| &entry.coverage),
        Some(&FileCoverage::Deleted)
    );
}

#[test]
fn a_path_marked_linguist_generated_is_absent_from_the_graph() {
    let temp = init_repo();
    let root = temp.path();
    commit_file(root, "src.rs", "fn main() {}\n", "seed");
    commit_file(root, "gen/bundle.js", "var bundled = 1;\n", "bundle");
    commit_file(
        root,
        ".gitattributes",
        "gen/** linguist-generated\n",
        "attrs",
    );
    let (store, graph_store) = stores(root);
    let revision = reconcile_base(&store, &graph_store, root);

    let base = graph_store.load_base(&revision).unwrap().unwrap();
    assert!(base.files.contains_key("src.rs"));
    assert!(
        !base.files.contains_key("gen/bundle.js"),
        "a committed linguist-generated path must not enter the base layer"
    );

    std::fs::write(root.join("gen/extra.js"), "var extra = 2;\n").unwrap();
    reconcile_source_graph(&store, &graph_store, root, overlay()).unwrap();
    let resolved = graph_store.resolved(&revision, Some(("p", "s"))).unwrap();
    assert!(resolved.files.contains_key("src.rs"));
    assert!(!resolved.files.contains_key("gen/bundle.js"));
    assert!(
        !resolved.files.contains_key("gen/extra.js"),
        "an untracked linguist-generated path must not enter the overlay"
    );
}
