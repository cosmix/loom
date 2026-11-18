//! Persisting, loading and pruning views through [`GraphStore`].

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, SystemTime};

use serial_test::serial;
use tempfile::TempDir;

use super::build::resolution_runs;
use super::*;
use crate::context::extract::{extract_file, registry};
use crate::context::graph_store::{FileEntry, GraphLayer, GraphStore};
use crate::context::refresh::{ensure_snapshot, SnapshotPolicy};
use crate::context::source_graph::GRAPH_SCHEMA_VERSION;
use crate::context::store::ContextStore;
use crate::test_git::{git, TestRepo};

pub(super) const A_RS: (&str, &str) = ("src/a.rs", "pub fn helper() {}\n");
pub(super) const B_RS: (&str, &str) = ("src/b.rs", "pub fn run() {\n    helper();\n}\n");
pub(super) const REV_OLD: &str = "aaaa1111";
pub(super) const REV_NEW: &str = "bbbb2222";

pub(super) fn layer_of(revision: &str, files: &[(&str, &str)]) -> GraphLayer {
    let extractors = registry();
    let mut layer = GraphLayer {
        revision: revision.to_string(),
        schema_version: GRAPH_SCHEMA_VERSION,
        ..GraphLayer::default()
    };
    for (path, source) in files {
        let extraction = extract_file(&extractors, Path::new(path), source.as_bytes());
        layer.files.insert(
            (*path).to_string(),
            FileEntry::from_extraction(source.as_bytes(), extraction),
        );
    }
    layer
}

fn node_count(view: &ResolvedView) -> usize {
    view.graph.files.values().map(|e| e.nodes.len()).sum()
}

/// A store whose project root is `root`, so `root/.loom/config.toml` is the
/// config its prune reads.
pub(super) fn store_in(root: &Path) -> GraphStore {
    GraphStore::new(
        &root.join(".loom/cache/context-v1"),
        &root.join(".loom/work"),
    )
}

pub(super) fn view_file_names(store: &GraphStore) -> Vec<String> {
    let Ok(entries) = fs::read_dir(store.view_dir()) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

fn age(path: &Path, seconds: u64) {
    let modified = SystemTime::now() - Duration::from_secs(seconds);
    fs::File::open(path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
}

#[test]
fn missing_base_view_is_never_persisted() {
    let temp = TempDir::new().unwrap();
    let store = store_in(temp.path());

    let empty = store.view(REV_OLD, None).unwrap();

    assert_eq!(node_count(&empty), 0);
    assert!(
        view_file_names(&store).is_empty(),
        "an empty view was persisted"
    );

    store
        .publish_base(REV_OLD, &layer_of(REV_OLD, &[A_RS, B_RS]))
        .unwrap();
    let published = store.view(REV_OLD, None).unwrap();

    assert!(
        node_count(&published) > 0,
        "the empty view shadowed the base"
    );
    assert_eq!(view_file_names(&store).len(), 1);
}

#[test]
fn an_unparseable_base_view_is_never_persisted() {
    let temp = TempDir::new().unwrap();
    let store = store_in(temp.path());
    let base = store.base_path(REV_OLD);
    fs::create_dir_all(store.base_dir()).unwrap();
    fs::write(&base, b"not json").unwrap();

    let view = store.view(REV_OLD, None).unwrap();

    assert_eq!(node_count(&view), 0);
    assert!(
        view_file_names(&store).is_empty(),
        "the view of an unparseable base was persisted"
    );
    assert_eq!(fs::read(&base).unwrap(), b"not json");
}

#[test]
fn prune_enforces_byte_budget_oldest_first() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".loom")).unwrap();
    fs::write(
        temp.path().join(".loom/config.toml"),
        "[retrieval]\ngraph_cache_budget_bytes = 1024\n",
    )
    .unwrap();
    let store = store_in(temp.path());
    store
        .publish_base(REV_OLD, &layer_of(REV_OLD, &[A_RS, B_RS]))
        .unwrap();
    store.view(REV_OLD, None).unwrap();
    age(&store.base_path(REV_OLD), 3600);
    assert_eq!(store.view_files(REV_OLD).len(), 1);

    store
        .publish_base(REV_NEW, &layer_of(REV_NEW, &[A_RS, B_RS]))
        .unwrap();

    assert!(
        !store.base_path(REV_OLD).exists(),
        "the older base survived"
    );
    assert!(
        store.view_files(REV_OLD).is_empty(),
        "the older view survived"
    );
    assert!(
        store.base_path(REV_NEW).is_file(),
        "the revision just published is protected even over budget"
    );
}

#[test]
fn prune_keeps_a_base_and_its_views_together_within_budget() {
    let temp = TempDir::new().unwrap();
    let store = store_in(temp.path());
    store
        .publish_base(REV_OLD, &layer_of(REV_OLD, &[A_RS, B_RS]))
        .unwrap();
    store.view(REV_OLD, None).unwrap();

    store.prune_to_budget(usize::MAX, &[]).unwrap();
    assert!(store.base_path(REV_OLD).is_file());
    assert_eq!(store.view_files(REV_OLD).len(), 1);

    store.prune_to_budget(1, &[REV_OLD]).unwrap();
    assert!(
        store.base_path(REV_OLD).is_file(),
        "a protected base was evicted"
    );

    store.prune_to_budget(1, &[]).unwrap();
    assert!(!store.base_path(REV_OLD).exists());
    assert!(store.view_files(REV_OLD).is_empty());
}

/// A committed repo with a cross-file call and its stores; the second store
/// stands in for another process, so its in-process view cache is empty.
fn snapshot_repo() -> (TempDir, GraphStore, String) {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    TestRepo::builder()
        .identity("t", "t@t")
        .no_seed()
        .init_in(root);
    for (path, source) in [A_RS, B_RS] {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(path), source).unwrap();
    }
    git(root, &["add", "src"]);
    git(root, &["commit", "-m", "seed"]);
    let context = ContextStore::with_root(root.join(".loom/cache/context-v1"));
    let graph_store = store_in(root);
    let head = ensure_snapshot(&context, &graph_store, root, SnapshotPolicy::BaseOnly).revision;
    assert!(!head.is_empty());
    (temp, graph_store, head)
}

#[test]
#[serial]
fn warm_view_load_runs_no_resolution() {
    let (temp, _first, head) = snapshot_repo();
    let second = store_in(temp.path());

    let before = resolution_runs();
    let view = second.view(&head, None).unwrap();

    assert_eq!(
        resolution_runs(),
        before,
        "a persisted view was re-resolved"
    );
    assert_eq!(view.origin, ViewOrigin::Materialized);
}

#[test]
#[serial]
fn view_round_trips_through_disk_with_provenance_counts() {
    let (temp, first, head) = snapshot_repo();
    // Taken from the process that materialized it: never parsed from disk.
    let materialized = first.view(&head, None).unwrap();
    let from_disk = store_in(temp.path()).view(&head, None).unwrap();

    assert_eq!(from_disk.origin, ViewOrigin::Materialized);
    assert!(
        !from_disk.stats.by_provenance.is_empty(),
        "the cross-file call left no provenance counts: {:?}",
        from_disk.stats
    );
    assert_eq!(
        from_disk.stats.by_provenance,
        materialized.stats.by_provenance
    );
    assert_eq!(
        canonical_bytes(&from_disk).unwrap(),
        canonical_bytes(&materialized).unwrap()
    );
}

#[test]
#[serial]
fn an_identity_mismatch_is_rebuilt_and_rewritten() {
    let (temp, first, head) = snapshot_repo();
    let identity = ViewIdentity::current(&head, "");
    let path = first.view_path(&identity, None);
    let mut stale: ResolvedView = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    stale.identity.extractor_digest = "sha256:retired".to_string();
    fs::write(&path, canonical_bytes(&stale).unwrap()).unwrap();
    let reader = store_in(temp.path());
    assert!(reader.load_view(&identity, None).is_none());

    let before = resolution_runs();
    let rebuilt = reader.view(&head, None).unwrap();

    assert_eq!(rebuilt.identity, identity);
    assert!(resolution_runs() > before, "the stale view was served");
    let rewritten: ResolvedView = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(rewritten.identity, identity);
}

#[test]
#[serial]
fn garbage_bytes_are_rebuilt() {
    let (temp, first, head) = snapshot_repo();
    let path = first.view_path(&ViewIdentity::current(&head, ""), None);
    fs::write(&path, b"not json").unwrap();

    let view = store_in(temp.path()).view(&head, None).unwrap();

    assert_eq!(view.origin, ViewOrigin::Built);
    assert!(node_count(&view) > 0);
    assert!(serde_json::from_slice::<ResolvedView>(&fs::read(&path).unwrap()).is_ok());
}

#[test]
#[serial]
fn writing_a_view_removes_views_of_other_identities() {
    let (temp, first, head) = snapshot_repo();
    let identity = ViewIdentity::current(&head, "");
    let path = first.view_path(&identity, None);
    let orphan = first.view_dir().join(format!("{head}-000000000000.json"));
    fs::write(&orphan, b"{}").unwrap();
    fs::remove_file(&path).unwrap();

    store_in(temp.path()).view(&head, None).unwrap();

    assert!(path.is_file());
    assert!(!orphan.exists(), "a superseded view was left behind");
}

#[test]
#[serial]
fn an_overlay_view_is_persisted_beside_its_layer_and_equals_the_cold_build() {
    let temp = TempDir::new().unwrap();
    let store = store_in(temp.path());
    store
        .publish_base(REV_OLD, &layer_of(REV_OLD, &[A_RS]))
        .unwrap();
    let mut overlay = layer_of(REV_OLD, &[B_RS]);
    overlay.generation = "gen-1".to_string();
    store.save_overlay("plan", "stage", &overlay).unwrap();
    let overlaid = Some(("plan", "stage"));

    let view = store.view(REV_OLD, overlaid).unwrap();

    let identity = ViewIdentity::current(REV_OLD, "gen-1");
    assert_eq!(view.identity, identity);
    assert!(store.view_path(&identity, overlaid).is_file());
    let cold = build_cold(store.resolved(REV_OLD, overlaid).unwrap(), identity.clone());
    assert_eq!(
        canonical_bytes(&view).unwrap(),
        canonical_bytes(&cold).unwrap()
    );

    store.discard_overlay("plan", "stage").unwrap();
    assert!(!store.view_path(&identity, overlaid).exists());
}

#[test]
#[serial]
fn the_memory_fallback_serves_a_view_when_writes_are_denied() {
    let temp = TempDir::new().unwrap();
    let store = store_in(temp.path());
    store
        .publish_base(REV_OLD, &layer_of(REV_OLD, &[A_RS, B_RS]))
        .unwrap();
    let view_dir = store.view_dir();
    fs::create_dir_all(&view_dir).unwrap();
    fs::set_permissions(&view_dir, fs::Permissions::from_mode(0o555)).unwrap();
    let enforced = fs::write(view_dir.join(".probe"), b"x").is_err();

    let built = store.view(REV_OLD, None);
    let before = resolution_runs();
    let served = store.view(REV_OLD, None);
    let resolved_again = resolution_runs() != before;
    let listing = view_file_names(&store);
    fs::set_permissions(&view_dir, fs::Permissions::from_mode(0o755)).unwrap();

    if !enforced {
        eprintln!("SKIP: this environment does not enforce 0o555 directory permissions");
        return;
    }
    assert!(node_count(&built.unwrap()) > 0);
    assert!(node_count(&served.unwrap()) > 0);
    assert!(
        !resolved_again,
        "the fallback view was resolved a second time"
    );
    assert!(
        listing.is_empty(),
        "a denied write reached the disk: {listing:?}"
    );
    assert!(store.fell_back());
}
