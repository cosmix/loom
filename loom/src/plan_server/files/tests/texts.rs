//! The text store: load, batches, and turn pins.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::plan_server::files::texts::{TextStore, TurnChange, TurnId};
use crate::planning::project::Project;
use crate::planning::sections::normalized_hash;
use crate::planning::watch::Batch;

const A: &str = "doc/product/a.md";
const B: &str = "doc/product/b.md";
const C: &str = "doc/product/c.md";
const SUMMARY: &str = "doc/loom/summaries/s.md";

fn write(root: &TempDir, rel: &str, text: &str) {
    let path = root.path().join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A root with `a.md`, `b.md` and a summary-store file, and its loaded store.
fn store() -> (TempDir, TextStore) {
    let root = tempfile::tempdir().unwrap();
    write(&root, A, "# A\n");
    write(&root, B, "# B\n");
    write(&root, SUMMARY, "# Summary\n");
    let project = Project::load(root.path()).unwrap();
    let store = TextStore::load(root.path(), &project).unwrap();
    (root, store)
}

fn hash_of(store: &TextStore, rel: &str) -> Option<String> {
    store.current(Path::new(rel)).map(|pinned| pinned.hash)
}

#[test]
fn load_holds_every_document_and_never_the_summary_store() {
    let (_root, store) = store();
    let a = store.current(Path::new(A)).expect("a.md is held");
    assert_eq!(&*a.text, "# A\n");
    assert_eq!(a.hash, normalized_hash("# A\n"));
    assert!(store.current(Path::new(SUMMARY)).is_none());
}

#[test]
fn apply_follows_a_batch() {
    let (root, store) = store();
    write(&root, A, "# A edited\n");
    write(&root, C, "# C\n");
    std::fs::remove_file(root.path().join(B)).unwrap();
    std::fs::rename(root.path().join(C), root.path().join("doc/product/d.md")).unwrap();
    write(&root, "doc/loom/summaries/t.md", "# T\n");
    let batch = Batch {
        added: vec![PathBuf::from("doc/loom/summaries/t.md")],
        changed: vec![PathBuf::from(A)],
        removed: vec![PathBuf::from(B)],
        renamed: Vec::new(),
        initial: false,
    };
    store.apply(root.path(), &batch);
    assert_eq!(hash_of(&store, A), Some(normalized_hash("# A edited\n")));
    assert_eq!(hash_of(&store, B), None);
    assert_eq!(hash_of(&store, "doc/loom/summaries/t.md"), None);

    let rename = Batch {
        added: vec![PathBuf::from("doc/product/d.md")],
        ..Batch::default()
    };
    store.apply(root.path(), &rename);
    let moved = Batch {
        renamed: vec![(
            PathBuf::from("doc/product/d.md"),
            PathBuf::from("doc/product/e.md"),
        )],
        ..Batch::default()
    };
    store.apply(root.path(), &moved);
    assert_eq!(hash_of(&store, "doc/product/d.md"), None);
    assert_eq!(
        hash_of(&store, "doc/product/e.md"),
        Some(normalized_hash("# C\n"))
    );
}

#[test]
fn a_turn_reports_an_edit_a_creation_and_a_deletion() {
    let (root, store) = store();
    let turn = TurnId("session/1".to_owned());
    store.begin_turn(&turn);
    write(&root, A, "# A after\n");
    write(&root, C, "# C\n");
    std::fs::remove_file(root.path().join(B)).unwrap();
    // The project predates the turn's writes: end_turn rediscovers and reads the disk itself.
    let project = Project::load(root.path()).unwrap();
    let changes = store.end_turn(&turn, root.path(), &project);
    let expected = vec![
        TurnChange {
            path: PathBuf::from(A),
            before_hash: Some(normalized_hash("# A\n")),
            after_hash: Some(normalized_hash("# A after\n")),
        },
        TurnChange {
            path: PathBuf::from(B),
            before_hash: Some(normalized_hash("# B\n")),
            after_hash: None,
        },
        TurnChange {
            path: PathBuf::from(C),
            before_hash: None,
            after_hash: Some(normalized_hash("# C\n")),
        },
    ];
    assert_eq!(changes, expected);
    let pin = store.turn_pin(&turn, Path::new(A)).unwrap();
    assert_eq!(&*pin.text, "# A\n");
    let end = store.turn_end(&turn, Path::new(A)).unwrap();
    assert_eq!(&*end.text, "# A after\n");
    assert!(store.turn_pin(&turn, Path::new(C)).is_none());
    assert!(store.turn_end(&turn, Path::new(B)).is_none());
}

#[test]
fn end_turn_reads_files_the_project_has_not_seen() {
    let (root, store) = store();
    let stale = Project::load(root.path()).unwrap();
    let turn = TurnId("session/2".to_owned());
    store.begin_turn(&turn);
    write(&root, C, "# C\n");
    let changes = store.end_turn(&turn, root.path(), &stale);
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(changes[0].path, PathBuf::from(C));
}

#[test]
fn a_turn_never_begun_has_no_changes_or_texts() {
    let (root, store) = store();
    let project = Project::load(root.path()).unwrap();
    let turn = TurnId("never".to_owned());
    write(&root, A, "# Changed\n");
    assert!(store.end_turn(&turn, root.path(), &project).is_empty());
    assert!(store.turn_pin(&turn, Path::new(A)).is_none());
    assert!(store.turn_end(&turn, Path::new(A)).is_none());
}
