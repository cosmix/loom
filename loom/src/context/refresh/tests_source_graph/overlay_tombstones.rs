//! Which tombstones an overlay carries, and where the prune of one the base
//! lacks happens.

use super::*;

fn layer_with_tombstone(revision: &str, path: &str) -> GraphLayer {
    let mut layer = GraphLayer {
        revision: revision.to_string(),
        ..Default::default()
    };
    layer.files.insert(path.to_string(), FileEntry::tombstone());
    layer
}

#[test]
fn a_kept_ignored_file_removed_from_the_index_is_tombstoned() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    let revision = head_sha(root);
    reconcile_source_graph(&store, &graph_store, root, base_scope(root)).unwrap();
    std::fs::write(root.join(".gitignore"), "src.rs\n").unwrap();
    git_ok(root, &["rm", "--cached", "src.rs"]);
    assert!(root.join("src.rs").exists());

    reconcile_source_graph(&store, &graph_store, root, overlay_scope("ignored")).unwrap();

    let resolved = graph_store
        .resolved(&revision, Some(("plan-contract", "ignored")))
        .unwrap();
    let overlay = graph_store
        .load_overlay("plan-contract", "ignored")
        .unwrap()
        .unwrap();
    assert!(!resolved.files.contains_key("src.rs"));
    assert_eq!(
        overlay.files.get("src.rs").map(|entry| &entry.coverage),
        Some(&FileCoverage::Deleted)
    );
}

#[test]
fn persist_layer_drops_a_tombstone_the_base_lacks() {
    let temp = init_repo();
    let (_, graph_store) = stores(&temp);
    let mut base = GraphLayer {
        revision: "rev".to_string(),
        ..Default::default()
    };
    base.files.insert(
        "present.rs".to_string(),
        entry_with_parser_version(Path::new("present.rs"), "v"),
    );
    let mut layer = layer_with_tombstone("rev", "never-existed.rs");
    layer
        .files
        .insert("present.rs".to_string(), FileEntry::tombstone());

    persist_layer(
        &graph_store,
        &overlay_scope("prune"),
        &layer,
        None,
        Some(&base),
        BaseWrite::Publish,
    )
    .unwrap();

    let saved = graph_store
        .load_overlay("plan-contract", "prune")
        .unwrap()
        .unwrap();
    assert!(!saved.files.contains_key("never-existed.rs"));
    assert!(saved.files.contains_key("present.rs"));
}

#[test]
fn save_overlay_stores_the_layer_it_is_given() {
    let temp = init_repo();
    let (_, graph_store) = stores(&temp);
    let empty = GraphLayer {
        revision: "rev".to_string(),
        ..Default::default()
    };
    graph_store.publish_base("rev", &empty).unwrap();

    graph_store
        .save_overlay(
            "plan-contract",
            "as-given",
            &layer_with_tombstone("rev", "never-existed.rs"),
        )
        .unwrap();

    let saved = graph_store
        .load_overlay("plan-contract", "as-given")
        .unwrap()
        .unwrap();
    assert!(saved.files.contains_key("never-existed.rs"));
}
