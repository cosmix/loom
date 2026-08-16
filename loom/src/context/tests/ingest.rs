use crate::context::{fingerprint::*, refresh::*, store::*};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Write `contents` to `root/relative`, creating parent directories as needed.
fn write_file(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

#[test]
fn fingerprint_file_is_stable_across_two_calls() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_file(root, "architecture.md", "# Architecture\n\nBody text.\n");

    let first = fingerprint_file(root, Path::new("architecture.md")).unwrap();
    let second = fingerprint_file(root, Path::new("architecture.md")).unwrap();

    assert_eq!(first, second);
    assert!(first.content_hash.starts_with("sha256:"));
    assert_eq!(first.size, "# Architecture\n\nBody text.\n".len() as u64);
}

#[test]
fn fingerprint_tree_sorts_and_skips_dotfiles_and_dot_directories() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_file(root, "zebra.md", "z");
    write_file(root, "alpha.md", "a");
    write_file(root, "architecture/nested.md", "n");
    write_file(root, ".hidden.md", "hidden");
    write_file(root, ".git/ignored.md", "ignored");
    write_file(root, "not-markdown.txt", "irrelevant");

    let fingerprints = fingerprint_tree(root).unwrap();
    let paths: Vec<String> = fingerprints
        .iter()
        .map(|fp| fp.path.to_string_lossy().into_owned())
        .collect();

    assert_eq!(
        paths,
        vec![
            "alpha.md".to_string(),
            "architecture/nested.md".to_string(),
            "zebra.md".to_string(),
        ]
    );
}

#[test]
fn fingerprint_tree_missing_root_is_empty() {
    let temp = TempDir::new().unwrap();
    let missing = temp.path().join("does-not-exist");

    let fingerprints = fingerprint_tree(&missing).unwrap();

    assert!(fingerprints.is_empty());
}

#[test]
fn tree_revision_is_stable_on_reread_and_changes_on_one_byte() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_file(root, "a.md", "hello");
    write_file(root, "b.md", "world");

    let revision_a = tree_revision(&fingerprint_tree(root).unwrap());
    let revision_b = tree_revision(&fingerprint_tree(root).unwrap());
    assert_eq!(
        revision_a, revision_b,
        "re-reading unchanged files must not change the revision"
    );

    write_file(root, "a.md", "hellx"); // one byte different
    let revision_c = tree_revision(&fingerprint_tree(root).unwrap());
    assert_ne!(revision_a, revision_c);
}

#[test]
fn evaluate_reports_never_built_on_empty_store() {
    let temp = TempDir::new().unwrap();
    let knowledge_root = temp.path().join("knowledge");
    write_file(&knowledge_root, "architecture.md", "# Architecture\n");

    let store = ContextStore::with_root(temp.path().join("store"));
    let state = evaluate(&store, &knowledge_root).unwrap();

    assert!(state.structural.stale);
    assert!(state.structural.revision.is_empty());
    assert_eq!(
        state.structural.detail.as_deref(),
        Some("catalog has never been built")
    );
    assert!(state.semantic.stale);
    assert!(state.semantic.revision.is_empty());
    assert_eq!(
        state.semantic.detail.as_deref(),
        Some("source graph not built; see the source-graph stage")
    );
}

#[test]
fn refresh_rebuilds_once_then_reports_no_rebuild_on_second_call() {
    let temp = TempDir::new().unwrap();
    let knowledge_root = temp.path().join("knowledge");
    write_file(
        &knowledge_root,
        "architecture.md",
        "# Architecture\n\nBody.\n",
    );

    let store = ContextStore::with_root(temp.path().join("store"));

    let first = refresh(&store, &knowledge_root, true).unwrap();
    assert!(first.rebuilt);
    assert!(!first.structural.stale);
    assert!(!first.structural.revision.is_empty());
    assert!(first.report.is_some());

    let second = refresh(&store, &knowledge_root, true).unwrap();
    assert!(!second.rebuilt);
    assert!(second.report.is_none());
    assert_eq!(second.structural.revision, first.structural.revision);
}

#[test]
fn evaluate_reports_stale_when_catalog_file_is_missing() {
    let temp = TempDir::new().unwrap();
    let knowledge_root = temp.path().join("knowledge");
    write_file(
        &knowledge_root,
        "architecture.md",
        "# Architecture\n\nBody.\n",
    );

    let store = ContextStore::with_root(temp.path().join("store"));
    let first = refresh(&store, &knowledge_root, true).unwrap();
    assert!(first.rebuilt);

    fs::remove_file(store.catalog_path()).unwrap();

    let state = evaluate(&store, &knowledge_root).unwrap();
    assert!(
        state.structural.stale,
        "a missing catalog must be reported stale even though state.json still claims it is current"
    );

    let second = refresh(&store, &knowledge_root, true).unwrap();
    assert!(
        second.rebuilt,
        "refresh must rebuild when the catalog file is gone"
    );
}

#[test]
fn evaluate_reports_stale_when_catalog_file_does_not_match_recorded_revision() {
    let temp = TempDir::new().unwrap();
    let knowledge_root = temp.path().join("knowledge");
    write_file(
        &knowledge_root,
        "architecture.md",
        "# Architecture\n\nBody.\n",
    );

    let store = ContextStore::with_root(temp.path().join("store"));
    let first = refresh(&store, &knowledge_root, true).unwrap();
    assert!(first.rebuilt);
    let stored_after_first = store.load_state().unwrap();

    // Build a different, independently-valid catalog (different content, so a
    // different revision) and write it over the one `refresh` just saved —
    // simulating catalog.json being replaced out from under state.json.
    let other_root = temp.path().join("other-knowledge");
    write_file(
        &other_root,
        "patterns.md",
        "# Patterns\n\nDifferent body.\n",
    );
    let other_catalog = crate::fs::knowledge::catalog::build(&other_root).unwrap();
    assert_ne!(other_catalog.revision, stored_after_first.catalog_revision);
    store.save_catalog(&other_catalog).unwrap();

    let state = evaluate(&store, &knowledge_root).unwrap();
    assert!(state.structural.stale);
}

#[test]
fn refresh_does_not_modify_knowledge_files() {
    let temp = TempDir::new().unwrap();
    let knowledge_root = temp.path().join("knowledge");
    write_file(
        &knowledge_root,
        "architecture.md",
        "# Architecture\n\nBody.\n",
    );
    write_file(
        &knowledge_root,
        "patterns/example.md",
        "# Example\n\nDetail.\n",
    );

    let before: Vec<(std::path::PathBuf, Vec<u8>)> = fingerprint_tree(&knowledge_root)
        .unwrap()
        .into_iter()
        .map(|fp| {
            let bytes = fs::read(knowledge_root.join(&fp.path)).unwrap();
            (fp.path, bytes)
        })
        .collect();
    assert!(!before.is_empty());

    let store = ContextStore::with_root(temp.path().join("store"));
    refresh(&store, &knowledge_root, true).unwrap();

    for (path, original_bytes) in &before {
        let after_bytes = fs::read(knowledge_root.join(path)).unwrap();
        assert_eq!(
            &after_bytes,
            original_bytes,
            "refresh modified knowledge file {}",
            path.display()
        );
    }
}
