use super::*;
use tempfile::TempDir;

fn entry(hash: &str) -> FileEntry {
    FileEntry {
        content_hash: hash.to_string(),
        coverage: FileCoverage::Full,
        ..FileEntry::default()
    }
}

fn store(temp: &TempDir) -> GraphStore {
    GraphStore::new(&temp.path().join("cache"), &temp.path().join("work"))
}

#[test]
fn an_overlay_entry_shadows_the_base_entry_for_the_same_path() {
    let temp = TempDir::new().unwrap();
    let store = store(&temp);

    let mut base = GraphLayer {
        revision: "rev1".to_string(),
        ..GraphLayer::default()
    };
    base.files.insert("src/a.rs".to_string(), entry("base-a"));
    base.files.insert("src/b.rs".to_string(), entry("base-b"));
    assert!(store.publish_base("rev1", &base).unwrap());

    let mut overlay = GraphLayer::default();
    overlay
        .files
        .insert("src/a.rs".to_string(), entry("overlay-a"));
    store.save_overlay("plan", "stage", &overlay).unwrap();

    let resolved = store.resolved("rev1", Some(("plan", "stage"))).unwrap();
    assert_eq!(resolved.files["src/a.rs"].content_hash, "overlay-a");
    assert_eq!(resolved.files["src/b.rs"].content_hash, "base-b");
    assert_eq!(resolved.overlaid.len(), 1);
    assert!(resolved.overlaid.contains("src/a.rs"));
}

#[test]
fn reading_without_a_stage_sees_only_the_base() {
    let temp = TempDir::new().unwrap();
    let store = store(&temp);

    let mut base = GraphLayer::default();
    base.files.insert("src/a.rs".to_string(), entry("base-a"));
    store.publish_base("rev1", &base).unwrap();

    let mut overlay = GraphLayer::default();
    overlay
        .files
        .insert("src/a.rs".to_string(), entry("overlay-a"));
    store.save_overlay("plan", "stage", &overlay).unwrap();

    let resolved = store.resolved("rev1", None).unwrap();
    assert_eq!(resolved.files["src/a.rs"].content_hash, "base-a");
    assert!(resolved.overlaid.is_empty());
}

#[test]
fn a_published_base_layer_is_immutable() {
    let temp = TempDir::new().unwrap();
    let store = store(&temp);

    let mut first = GraphLayer::default();
    first.files.insert("src/a.rs".to_string(), entry("first"));
    assert!(store.publish_base("rev1", &first).unwrap());

    let mut second = GraphLayer::default();
    second.files.insert("src/a.rs".to_string(), entry("second"));
    assert!(
        !store.publish_base("rev1", &second).unwrap(),
        "re-publishing the same revision must be refused"
    );

    let loaded = store.load_base("rev1").unwrap().unwrap();
    assert_eq!(loaded.files["src/a.rs"].content_hash, "first");
}

#[test]
fn two_stages_overlays_never_see_each_other() {
    let temp = TempDir::new().unwrap();
    let store = store(&temp);

    let mut one = GraphLayer::default();
    one.files.insert("src/a.rs".to_string(), entry("stage-one"));
    store.save_overlay("plan", "one", &one).unwrap();

    let mut two = GraphLayer::default();
    two.files.insert("src/a.rs".to_string(), entry("stage-two"));
    store.save_overlay("plan", "two", &two).unwrap();

    let view_one = store.resolved("rev1", Some(("plan", "one"))).unwrap();
    let view_two = store.resolved("rev1", Some(("plan", "two"))).unwrap();
    assert_eq!(view_one.files["src/a.rs"].content_hash, "stage-one");
    assert_eq!(view_two.files["src/a.rs"].content_hash, "stage-two");
}

#[test]
fn discarding_an_absent_overlay_succeeds() {
    let temp = TempDir::new().unwrap();
    let store = store(&temp);
    store.discard_overlay("plan", "never-existed").unwrap();
}

#[test]
fn discarding_an_overlay_leaves_sibling_records_in_the_shared_directory() {
    let temp = TempDir::new().unwrap();
    let store = store(&temp);

    let overlay = GraphLayer::default();
    store.save_overlay("plan", "stage", &overlay).unwrap();

    let dir = store.overlay_dir("plan", "stage");
    let session_retrieval_dir = dir.join("session-retrieval");
    std::fs::create_dir_all(&session_retrieval_dir).unwrap();
    let dirty_paths = dir.join("dirty-paths.json");
    std::fs::write(&dirty_paths, "{}").unwrap();
    let delivery_record = session_retrieval_dir.join("some-stage.json");
    std::fs::write(&delivery_record, "{}").unwrap();

    store.discard_overlay("plan", "stage").unwrap();

    assert!(
        !store.overlay_path("plan", "stage").exists(),
        "the graph layer file must be removed"
    );
    assert!(
        dirty_paths.exists(),
        "the edit recorder's file must survive discard_overlay"
    );
    assert!(
        delivery_record.exists(),
        "the delivery record must survive discard_overlay"
    );
}

#[test]
fn a_missing_base_resolves_to_an_overlay_only_view() {
    let temp = TempDir::new().unwrap();
    let store = store(&temp);

    let mut overlay = GraphLayer::default();
    overlay.files.insert("src/a.rs".to_string(), entry("only"));
    store.save_overlay("plan", "stage", &overlay).unwrap();

    let resolved = store
        .resolved("never-built", Some(("plan", "stage")))
        .unwrap();
    assert_eq!(resolved.files.len(), 1);
    assert_eq!(resolved.base_revision, "");
}
