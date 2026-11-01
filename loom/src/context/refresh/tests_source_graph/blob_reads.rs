use super::*;
use crate::context::graph_store::ResolvedGraph;
use crate::context::source_graph::body_hash;
use crate::context::window::read_window;
use crate::git::runner::{run_git_checked, run_git_checked_bytes};

fn isolated_stores(root: &Path, name: &str) -> (ContextStore, GraphStore) {
    let store = ContextStore::with_root(root.join(".loom").join(name));
    let graph = GraphStore::new(store.root(), &root.join(".loom/work"));
    (store, graph)
}

#[cfg(feature = "source-graph")]
#[test]
fn cold_dirty_base_equals_clean_base_after_exact_restoration() {
    let temp = init_repo();
    let root = temp.path();
    let source = b"\n\n  pub fn original() {}\n \t\n";
    let binary = b"\n\xff\x00 content \xfe\t\n";
    std::fs::write(root.join("src.rs"), source).unwrap();
    std::fs::write(root.join("bytes.txt"), binary).unwrap();
    run_git_checked(&["add", "."], root).unwrap();
    run_git_checked(
        &["-c", "commit.gpgsign=false", "commit", "-m", "exact bytes"],
        root,
    )
    .unwrap();
    let (clean_store, clean_graph) = isolated_stores(root, "clean-cache");
    reconcile_source_graph(&clean_store, &clean_graph, root, base_scope(root)).unwrap();
    let revision = head_sha(root);
    let clean = clean_graph.load_base(&revision).unwrap().unwrap();
    std::fs::write(root.join("src.rs"), "fn changed() {}\n").unwrap();
    std::fs::write(root.join("bytes.txt"), b"changed").unwrap();
    let (dirty_store, dirty_graph) = isolated_stores(root, "dirty-cache");
    reconcile_source_graph(&dirty_store, &dirty_graph, root, base_scope(root)).unwrap();
    let dirty = dirty_graph.load_base(&revision).unwrap().unwrap();
    assert_eq!(clean.files, dirty.files);
    assert_eq!(clean.blob_index, dirty.blob_index);
    assert_eq!(dirty.files["src.rs"].content_hash, body_hash(source));
    assert_eq!(dirty.files["bytes.txt"].content_hash, body_hash(binary));
    assert_eq!(
        run_git_checked_bytes(&["show", "HEAD:bytes.txt"], root).unwrap(),
        binary
    );
    std::fs::write(root.join("src.rs"), source).unwrap();
    std::fs::write(root.join("bytes.txt"), binary).unwrap();
    reconcile_source_graph(&dirty_store, &dirty_graph, root, base_scope(root)).unwrap();
    let restored = dirty_graph.load_base(&revision).unwrap().unwrap();
    assert_eq!(restored.files, clean.files);
    let graph = ResolvedGraph {
        files: restored.files,
        ..Default::default()
    };
    let window = read_window(&graph, root, "src.rs#function:original", 60).unwrap();
    assert_eq!(window.span.line_start, 3);
    assert_eq!(window.text, "  pub fn original() {}\n");
}

#[test]
fn oversized_blob_and_worktree_share_streamed_identity() {
    use crate::context::source_graph::MAX_EXTRACTED_FILE_BYTES;
    let temp = init_repo();
    let root = temp.path();
    let path = root.join("large.rs");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len((MAX_EXTRACTED_FILE_BYTES * 8) as u64).unwrap();
    run_git_checked(&["add", "large.rs"], root).unwrap();
    run_git_checked(
        &["-c", "commit.gpgsign=false", "commit", "-m", "large"],
        root,
    )
    .unwrap();
    let (store, graph_store) = isolated_stores(root, "oversized-clean");
    reconcile_source_graph(&store, &graph_store, root, base_scope(root)).unwrap();
    let clean = graph_store.load_base(&head_sha(root)).unwrap().unwrap();
    std::fs::write(&path, b"fn changed() {}\n").unwrap();
    let (store, graph_store) = isolated_stores(root, "oversized-dirty");
    reconcile_source_graph(&store, &graph_store, root, base_scope(root)).unwrap();
    let dirty = graph_store.load_base(&head_sha(root)).unwrap().unwrap();
    assert_eq!(clean.files["large.rs"], dirty.files["large.rs"]);
    assert!(matches!(
        dirty.files["large.rs"].coverage,
        FileCoverage::Oversized { .. }
    ));
}

fn local_scope(root: &Path) -> SourceGraphScope {
    let (plan, stage) = crate::context::local_overlay::local_overlay_key(root);
    SourceGraphScope::Overlay { plan, stage }
}

#[cfg(feature = "source-graph")]
#[test]
fn restoring_one_of_two_dirty_files_removes_its_old_overlay_entry() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph) = stores(&temp);
    reconcile_source_graph(&store, &graph, root, base_scope(root)).unwrap();
    std::fs::write(root.join("src.rs"), "fn dirty_symbol() {}\n").unwrap();
    std::fs::write(root.join("docs/notes.txt"), "dirty notes\n").unwrap();
    reconcile_source_graph(&store, &graph, root, local_scope(root)).unwrap();
    let (plan, stage) = crate::context::local_overlay::local_overlay_key(root);
    let dirty = graph.load_overlay(&plan, &stage).unwrap().unwrap();
    assert!(layer_mentions(&dirty, "src.rs", "dirty_symbol"));
    assert!(!dirty.blob_index.contains_key("src.rs"));

    git_ok(root, &["restore", "src.rs"]);
    reconcile_source_graph(&store, &graph, root, local_scope(root)).unwrap();
    let resolved = graph
        .resolved(&head_sha(root), Some((&plan, &stage)))
        .unwrap();
    assert_eq!(
        resolved.files["src.rs"].content_hash,
        body_hash(b"fn main() {}\n")
    );
    assert!(resolved.files["src.rs"]
        .nodes
        .iter()
        .any(|node| node.signature.contains("main")));
    assert!(!resolved.files["src.rs"]
        .nodes
        .iter()
        .any(|node| node.signature.contains("dirty_symbol")));
    assert_eq!(
        resolved.files["docs/notes.txt"].content_hash,
        body_hash(b"dirty notes\n")
    );
}

#[cfg(feature = "source-graph")]
#[test]
fn restored_overlay_entry_cannot_contaminate_a_new_base() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph) = stores(&temp);
    reconcile_source_graph(&store, &graph, root, base_scope(root)).unwrap();
    std::fs::write(root.join("src.rs"), "fn dirty_symbol() {}\n").unwrap();
    reconcile_source_graph(&store, &graph, root, local_scope(root)).unwrap();
    git_ok(root, &["restore", "src.rs"]);
    std::fs::write(root.join("docs/notes.txt"), "unrelated commit\n").unwrap();
    git_ok(root, &["add", "docs/notes.txt"]);
    git_ok(root, &["commit", "-m", "unrelated"]);
    reconcile_source_graph(&store, &graph, root, base_scope(root)).unwrap();
    let base = graph.load_base(&head_sha(root)).unwrap().unwrap();
    assert_eq!(
        base.files["src.rs"].content_hash,
        body_hash(b"fn main() {}\n")
    );
    assert!(layer_mentions(&base, "src.rs", "main"));
    assert!(!layer_mentions(&base, "src.rs", "dirty_symbol"));
}

#[cfg(feature = "source-graph")]
#[test]
fn old_schema_overlay_with_ambiguous_oids_is_not_reused() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph) = stores(&temp);
    reconcile_source_graph(&store, &graph, root, base_scope(root)).unwrap();
    std::fs::write(root.join("src.rs"), "fn dirty_symbol() {}\n").unwrap();
    std::fs::write(root.join("docs/notes.txt"), "dirty notes\n").unwrap();
    reconcile_source_graph(&store, &graph, root, local_scope(root)).unwrap();
    let (plan, stage) = crate::context::local_overlay::local_overlay_key(root);
    let mut old = graph.load_overlay(&plan, &stage).unwrap().unwrap();
    old.schema_version = crate::context::source_graph::GRAPH_SCHEMA_VERSION - 1;
    old.blob_index = graph
        .load_base(&head_sha(root))
        .unwrap()
        .unwrap()
        .blob_index;
    graph.save_overlay(&plan, &stage, &old).unwrap();

    git_ok(root, &["restore", "src.rs"]);
    reconcile_source_graph(&store, &graph, root, local_scope(root)).unwrap();
    let resolved = graph
        .resolved(&head_sha(root), Some((&plan, &stage)))
        .unwrap();
    assert_eq!(
        resolved.files["src.rs"].content_hash,
        body_hash(b"fn main() {}\n")
    );
    assert!(!resolved.files["src.rs"]
        .nodes
        .iter()
        .any(|node| node.signature.contains("dirty_symbol")));
    assert_eq!(
        resolved.files["docs/notes.txt"].content_hash,
        body_hash(b"dirty notes\n")
    );
}
