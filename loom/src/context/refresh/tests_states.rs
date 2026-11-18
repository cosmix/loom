//! The four graph states a `SnapshotOutcome` reports, the stale base a failed
//! build serves, and the read-only-cache marker.

use super::tests_snapshot::{
    git_ok, init_repo, lock_base_dir_read_only, restore_after_persist_probe, stores,
};
use super::*;
use crate::context::freshness::GraphState;
use crate::context::graph_store::GraphStore;
use serial_test::serial;
use tempfile::TempDir;

#[test]
fn returned_catalog_survives_another_tree_publishing() {
    let temp = TempDir::new().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    std::fs::write(
        first.join("first.md"),
        "# First\n\n## Alpha\n\nFirst tree.\n",
    )
    .unwrap();
    std::fs::write(
        second.join("second.md"),
        "# Second\n\n## Beta\n\nSecond tree.\n",
    )
    .unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));
    let (first_outcome, first_catalog) = refresh_snapshot(&store, &first, true).unwrap();
    let (second_outcome, second_catalog) = refresh_snapshot(&store, &second, true).unwrap();
    assert_ne!(
        first_outcome.structural.revision,
        second_outcome.structural.revision
    );
    assert_ne!(first_catalog.revision, second_catalog.revision);
    assert_eq!(store.load_catalog().unwrap().unwrap(), second_catalog);
    assert!(first_catalog
        .chunks
        .iter()
        .any(|chunk| chunk.id.as_str().contains("first")));
}

#[test]
fn cold_snapshot_does_not_reload_catalog_and_warm_snapshot_reads_once() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("knowledge");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("topic.md"), "# Topic\n\n## Section\n\nContent.\n").unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));
    crate::context::store::CATALOG_READS.with(|count| count.set(0));
    refresh_snapshot(&store, &root, true).unwrap();
    assert_eq!(
        crate::context::store::CATALOG_READS.with(|count| count.replace(0)),
        0
    );
    refresh_snapshot(&store, &root, true).unwrap();
    assert_eq!(
        crate::context::store::CATALOG_READS.with(|count| count.replace(0)),
        1
    );
}

#[test]
fn publication_failure_keeps_the_single_fingerprint_snapshot() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("knowledge");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("topic.md"), "# Topic\n\n## Section\n\nContent.\n").unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));
    store.ensure().unwrap();
    std::fs::create_dir(store.catalog_path()).unwrap();
    take_fingerprint_pass_count();
    let (outcome, catalog) = refresh_snapshot(&store, &root, true).unwrap();
    assert_eq!(take_fingerprint_pass_count(), 1);
    assert!(!catalog.chunks.is_empty());
    assert!(outcome
        .structural
        .detail
        .unwrap()
        .contains("cache not updated"));
}

#[test]
fn explicit_refresh_repairs_corrupt_artifacts() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("knowledge");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("topic.md"), "# Topic\n\n## Section\n\nContent.\n").unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));
    refresh(&store, &root, true).unwrap();
    for path in [store.state_path(), store.catalog_path()] {
        std::fs::write(&path, "{broken").unwrap();
        assert!(refresh(&store, &root, true).unwrap().rebuilt);
        store.load_state().unwrap();
        store.load_catalog().unwrap().unwrap();
    }
}

#[test]
fn concurrent_publications_return_their_own_snapshots() {
    use std::sync::mpsc;
    let temp = TempDir::new().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    write_test_tree(&first, "alpha");
    write_test_tree(&second, "beta");
    let store = ContextStore::with_root(temp.path().join("cache"));
    let first_store = store.clone();
    let first_root = first.clone();
    let (published, wait_published) = mpsc::channel();
    let (resume, wait_resume) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        AFTER_PUBLISH.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                published.send(()).unwrap();
                wait_resume.recv().unwrap();
            }))
        });
        refresh_snapshot(&first_store, &first_root, true).unwrap()
    });
    wait_published.recv().unwrap();
    store
        .update_state(|state| state.semantic.revision = "concurrent-semantic".into())
        .unwrap();
    let (second_outcome, second_catalog) = refresh_snapshot(&store, &second, true).unwrap();
    resume.send(()).unwrap();
    let (first_outcome, first_catalog) = worker.join().unwrap();
    for (root, name, outcome, catalog) in [
        (&first, "alpha", first_outcome, first_catalog),
        (&second, "beta", second_outcome, second_catalog),
    ] {
        assert_eq!(
            outcome.structural.revision,
            tree_revision(&fingerprint_tree(root).unwrap())
        );
        assert_eq!(outcome.report.unwrap().revision, catalog.revision);
        assert!(catalog
            .chunks
            .iter()
            .any(|chunk| chunk.id.as_str().contains(name)));
    }
    let (state, catalog) = store.load_snapshot().unwrap();
    assert_eq!(state.semantic.revision, "concurrent-semantic");
    assert_eq!(state.catalog_revision, catalog.unwrap().unwrap().revision);
}

fn write_test_tree(root: &Path, name: &str) {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join(format!("{name}.md")),
        format!("# {name}\n\n## Topic\n\n{name} content.\n"),
    )
    .unwrap();
}

fn stage_policy() -> SnapshotPolicy {
    SnapshotPolicy::StageOverlay {
        plan: "plan".to_string(),
        stage: "stage".to_string(),
    }
}

/// Make the stage overlay unreadable: a directory where the layer file
/// belongs fails the read with a non-NotFound error, so the build fails
/// before it touches any base.
fn break_stage_overlay(graph_store: &GraphStore) {
    std::fs::create_dir_all(graph_store.overlay_path("plan", "stage")).unwrap();
}

fn commit_second_revision(root: &Path) {
    std::fs::write(root.join("src.rs"), "fn second() {}\n").unwrap();
    git_ok(root, &["add", "src.rs"]);
    git_ok(root, &["commit", "-m", "second"]);
}

#[test]
#[serial]
fn failed_build_serves_newest_older_base_as_stale() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    let first = ensure_snapshot(&store, &graph_store, root, SnapshotPolicy::BaseOnly);
    assert_eq!(first.state(), GraphState::Current, "{first:?}");
    assert!(first.persisted);
    commit_second_revision(root);
    break_stage_overlay(&graph_store);

    let outcome = ensure_snapshot(&store, &graph_store, root, stage_policy());

    let head = working_tree(root).unwrap().head;
    assert_ne!(head, first.revision, "the failed build's HEAD is newer");
    assert_eq!(outcome.action, SnapshotAction::Unavailable, "{outcome:?}");
    assert_eq!(outcome.revision, first.revision);
    assert_eq!(outcome.serving, Some(first.revision.clone()));
    assert_eq!(outcome.state(), GraphState::Stale);
    let suffix = format!("; serving stale base {}", short_revision(&first.revision));
    assert!(
        outcome.describe().ends_with(&suffix),
        "{}",
        outcome.describe()
    );
}

#[test]
#[serial]
fn failed_build_without_a_base_is_never_built() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    break_stage_overlay(&graph_store);

    let outcome = ensure_snapshot(&store, &graph_store, root, stage_policy());

    assert_eq!(outcome.action, SnapshotAction::Unavailable, "{outcome:?}");
    assert_eq!(outcome.serving, None);
    assert_eq!(outcome.state(), GraphState::NeverBuilt);
}

#[test]
#[serial]
fn a_directory_without_git_reports_the_unavailable_state() {
    let temp = TempDir::new().unwrap();
    let (store, graph_store) = stores(&temp);

    let outcome = ensure_snapshot(
        &store,
        &graph_store,
        temp.path(),
        SnapshotPolicy::LocalCurrent,
    );

    assert_eq!(outcome.state(), GraphState::Unavailable);
    assert_eq!(outcome.serving, None);
}

/// Lock `dir` to 0o555; `true` when this environment enforces it (see
/// `tests_snapshot::lock_base_dir_read_only`).
fn lock_read_only(dir: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(dir).unwrap();
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let probe = dir.join(".write-probe");
    let enforced = std::fs::write(&probe, b"x").is_err();
    if !enforced {
        let _ = std::fs::remove_file(&probe);
    }
    enforced
}

fn assert_current_but_not_persisted(outcome: &SnapshotOutcome) {
    assert_eq!(outcome.action, SnapshotAction::Rebuilt, "{outcome:#?}");
    assert_eq!(outcome.state(), GraphState::Current);
    assert!(!outcome.persisted);
    let line = outcome.describe();
    assert!(
        line.ends_with("; not persisted (cache read-only)"),
        "{line}"
    );
}

/// The in-stage `loom map` case: the whole shared cache is read-only, so the
/// layer, the view and `state.json` are all refused.
#[test]
#[serial]
fn a_read_only_cache_serves_the_built_layer_as_current() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    let cache = store.root().to_path_buf();
    let enforced = lock_read_only(&cache);

    let outcome = ensure_snapshot(&store, &graph_store, root, SnapshotPolicy::BaseOnly);

    let test = "a_read_only_cache_serves_the_built_layer_as_current";
    if restore_after_persist_probe(&cache, !enforced, test) {
        return;
    }
    assert_current_but_not_persisted(&outcome);
    assert!(!store.state_path().exists());
    let resolved = graph_store.resolved(&outcome.revision, None).unwrap();
    assert!(resolved.files.contains_key("src.rs"), "{resolved:?}");
}

/// Only `state.json` is refused: the layer and its view reach the disk.
#[test]
#[serial]
fn a_refused_state_write_alone_reports_not_persisted() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    std::fs::create_dir_all(graph_store.base_dir()).unwrap();
    std::fs::create_dir_all(graph_store.view_dir()).unwrap();
    let cache = store.root().to_path_buf();
    let enforced = lock_read_only(&cache);

    let outcome = ensure_snapshot(&store, &graph_store, root, SnapshotPolicy::BaseOnly);

    let test = "a_refused_state_write_alone_reports_not_persisted";
    if restore_after_persist_probe(&cache, !enforced, test) {
        return;
    }
    assert_current_but_not_persisted(&outcome);
    assert!(!graph_store.fell_back());
    assert!(graph_store.base_path(&outcome.revision).is_file());
}

#[test]
#[serial]
fn read_only_cache_reports_not_persisted() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    let (base_dir, still_writable) = lock_base_dir_read_only(&graph_store);

    let outcome = ensure_snapshot(&store, &graph_store, root, SnapshotPolicy::BaseOnly);

    let test = "read_only_cache_reports_not_persisted";
    if restore_after_persist_probe(&base_dir, still_writable, test) {
        return;
    }
    assert_eq!(outcome.state(), GraphState::Current, "{outcome:?}");
    assert!(!outcome.persisted);
    let line = outcome.describe();
    assert!(
        line.ends_with("; not persisted (cache read-only)"),
        "{line}"
    );
}
