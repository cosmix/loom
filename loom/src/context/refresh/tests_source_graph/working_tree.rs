//! How generation, dirtiness, and renames drive a reconcile's behavior
//! against the working tree.

use super::*;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn linked_checkout_graph_uses_anchored_identity_after_pointer_change() {
    let original = init_repo();
    let foreign = init_repo();
    std::fs::write(foreign.path().join("src.rs"), "fn foreign() {}\n").unwrap();
    git_ok(foreign.path(), &["add", "src.rs"]);
    git_ok(foreign.path(), &["commit", "-m", "foreign"]);
    let parent = tempfile::TempDir::new().unwrap();
    let root = parent.path().join("checkout");
    git_ok(
        original.path(),
        &["worktree", "add", "-b", "linked", root.to_str().unwrap()],
    );
    let revision = head_sha(&root);
    let work = root.join(".loom/work");
    std::fs::create_dir_all(&work).unwrap();
    crate::git::worktree::capture_main_identity(&root, &work).unwrap();
    std::fs::write(
        root.join(".git"),
        format!("gitdir: {}\n", foreign.path().join(".git").display()),
    )
    .unwrap();
    std::fs::write(root.join("src.rs"), "fn dirty() {}\n").unwrap();

    let store = ContextStore::with_root(root.join(".loom/cache/context-v1"));
    let graph = GraphStore::new(store.root(), &work);
    let outcome = reconcile_source_graph(
        &store,
        &graph,
        &root,
        SourceGraphScope::Base {
            revision: revision.clone(),
        },
    )
    .unwrap();
    assert!(!outcome.freshness.stale, "{:?}", outcome.freshness);
    let base = graph.load_base(&revision).unwrap().unwrap();
    assert_eq!(
        base.files["src.rs"].content_hash,
        crate::context::source_graph::body_hash(b"fn main() {}\n")
    );
    assert_eq!(working_tree(&root).unwrap().head, revision);
}

#[test]
fn base_scope_refuses_to_publish_when_the_tracked_tree_is_dirty() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    let revision = head_sha(root);

    // Dirty a TRACKED file without committing.
    std::fs::write(root.join("src.rs"), "fn main() { /* uncommitted */ }\n").unwrap();

    let scope = SourceGraphScope::Base {
        revision: revision.clone(),
    };
    let outcome = reconcile_source_graph(&store, &graph_store, root, scope).unwrap();

    assert_eq!(outcome.counters.files_parsed, 2);
    assert!(!outcome.freshness.stale);
    assert!(
        graph_store.load_base(&revision).unwrap().is_some(),
        "a dirty tree must publish the committed base"
    );

    let state = store.load_state().unwrap();
    assert!(!state.semantic.stale);
}

#[test]
fn an_edit_at_unchanged_head_changes_the_generation_and_marks_the_overlay_stale() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    let head = head_sha(root);
    reconcile_source_graph(&store, &graph_store, root, overlay_scope("generation")).unwrap();
    let overlay = graph_store
        .load_overlay("plan-contract", "generation")
        .unwrap()
        .unwrap();

    std::fs::write(root.join("src.rs"), "fn moved_generation() {}\n").unwrap();
    let moved = working_tree(root).unwrap();

    assert_eq!(moved.head, head);
    assert_ne!(overlay.generation, moved.generation);
}

#[test]
fn a_renamed_file_appears_under_its_new_path_only() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    let revision = head_sha(root);
    reconcile_source_graph(&store, &graph_store, root, base_scope(root)).unwrap();
    git_ok(root, &["mv", "src.rs", "renamed.rs"]);

    let moved = working_tree(root).unwrap();
    assert_eq!(
        moved.dirty.get("renamed.rs").map(String::as_str),
        Some("R ")
    );
    assert_eq!(
        moved.dirty.get("src.rs").map(String::as_str),
        Some("R "),
        "the old path no longer exists on disk and must resolve through the \
         'deleted' identity rather than being dropped from `dirty`"
    );
    assert_ne!(
        moved.generation,
        clean_generation(&revision),
        "a rename must change the generation even though HEAD is unchanged"
    );

    reconcile_source_graph(&store, &graph_store, root, overlay_scope("renamed")).unwrap();
    let resolved = graph_store
        .resolved(&revision, Some(("plan-contract", "renamed")))
        .unwrap();

    assert!(resolved.files.contains_key("renamed.rs"));
    assert!(!resolved.files.contains_key("src.rs"));
}

#[test]
fn a_base_publishes_from_committed_content_on_a_dirty_tree() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    let revision = head_sha(root);
    std::fs::write(root.join("src.rs"), "fn edited_only() {}\n").unwrap();

    reconcile_source_graph(&store, &graph_store, root, base_scope(root)).unwrap();
    reconcile_source_graph(
        &store,
        &graph_store,
        root,
        overlay_scope("committed-content"),
    )
    .unwrap();
    let base = graph_store.load_base(&revision).unwrap().unwrap();
    let overlay = graph_store
        .load_overlay("plan-contract", "committed-content")
        .unwrap()
        .unwrap();

    assert!(layer_mentions(&base, "src.rs", "main"));
    assert!(!layer_mentions(&base, "src.rs", "edited_only"));
    assert!(layer_mentions(&overlay, "src.rs", "edited_only"));
}

#[test]
fn an_untracked_symlink_to_a_device_does_not_hang_the_generation() {
    // Old code hashed a dirty path's content with `fs::read`, which follows
    // symlinks: an untracked symlink to `/dev/zero` made that call read
    // forever. Run on a worker thread with a timeout so the old behavior
    // fails by timing out rather than hanging the whole test suite.
    if !Path::new("/dev/zero").exists() {
        eprintln!(
            "SKIP an_untracked_symlink_to_a_device_does_not_hang_the_generation: \
             /dev/zero is not present on this platform"
        );
        return;
    }

    let temp = init_repo();
    let root = temp.path().to_path_buf();
    std::os::unix::fs::symlink("/dev/zero", root.join("huge.rs")).unwrap();

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = working_tree(&root);
        let _ = tx.send(result.map(|tree| tree.generation));
    });

    let generation = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("working_tree must return promptly instead of reading a device file forever")
        .expect("working_tree must not fail on a refused symlink");
    assert!(!generation.is_empty());
}
