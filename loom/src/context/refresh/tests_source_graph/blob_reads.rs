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
