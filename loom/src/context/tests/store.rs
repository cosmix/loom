use crate::context::schema::{Freshness, KnowledgeChunk, LifecycleState};
use crate::context::store::{canonical_json, ContextStore, StoreState, CACHE_RELATIVE_DIR};
use crate::fs::knowledge::catalog::{Catalog, CatalogIssue};
use crate::fs::work_dir::WorkDir;
use std::fs;
use tempfile::TempDir;

fn sample_catalog() -> Catalog {
    Catalog {
        revision: "catalog-revision".to_string(),
        chunks: vec![KnowledgeChunk {
            id: "architecture.md#context-cache#1".to_string(),
            file: "architecture.md".into(),
            anchor: "context-cache".to_string(),
            heading: "Context cache".to_string(),
            body: "## Context cache\nDerived artifacts live in the shared cache.\n".to_string(),
            content_hash: "sha256:abc123".to_string(),
            estimated_tokens: 14,
            aliases: vec!["cache".to_string()],
            category: Some("architecture".to_string()),
            source_paths: vec!["loom/src/context/store.rs".to_string()],
            symbols: vec!["ContextStore".to_string()],
            links: vec![("Store state".to_string(), "state.md".to_string())],
            state: LifecycleState::Active,
        }],
        issues: vec![CatalogIssue::GenericBlurb {
            file: "architecture.md".into(),
            blurb: "Add more concrete details.".to_string(),
        }],
    }
}

#[cfg(unix)]
#[test]
fn open_resolves_cache_at_main_project_root_from_linked_worktree() {
    let temp = TempDir::new().unwrap();
    let main_repo = temp.path().join("main-repo");
    let worktree = main_repo.join(".worktrees").join("stage");

    fs::create_dir_all(main_repo.join(".work")).unwrap();
    fs::create_dir_all(&worktree).unwrap();
    std::os::unix::fs::symlink("../../.work", worktree.join(".work")).unwrap();

    let work_dir = WorkDir::new(&worktree).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();

    assert_eq!(store.root(), main_repo.join(CACHE_RELATIVE_DIR));
}

#[test]
fn ensure_is_idempotent() {
    let temp = TempDir::new().unwrap();
    let store = ContextStore::with_root(temp.path().join("nested").join("cache"));

    store.ensure().unwrap();
    store.ensure().unwrap();

    assert!(store.root().is_dir());
}

#[test]
fn canonical_json_is_deterministic() {
    let catalog = sample_catalog();

    let first = canonical_json(&catalog).unwrap();
    let second = canonical_json(&catalog).unwrap();

    assert_eq!(first, second);
    assert!(first.ends_with('\n'));
}

#[test]
fn save_and_load_catalog_round_trips() {
    let temp = TempDir::new().unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));
    let catalog = sample_catalog();

    store.save_catalog(&catalog).unwrap();

    assert_eq!(store.load_catalog().unwrap(), Some(catalog));
}

#[test]
fn load_catalog_returns_none_when_never_written() {
    let temp = TempDir::new().unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));

    assert_eq!(store.load_catalog().unwrap(), None);
}

#[test]
fn load_state_defaults_when_never_written() {
    let temp = TempDir::new().unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));

    let state = store.load_state().unwrap();

    assert_eq!(state.structural.revision, "");
    assert!(!state.structural.stale);
}

#[test]
fn save_and_load_state_round_trips() {
    let temp = TempDir::new().unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));
    let state = StoreState {
        structural: Freshness {
            revision: "structural-revision".to_string(),
            computed_at: None,
            stale: false,
            detail: None,
        },
        semantic: Freshness::never_built("semantic data has not been built"),
        catalog_revision: "catalog-revision".to_string(),
    };

    store.save_state(&state).unwrap();

    assert_eq!(store.load_state().unwrap(), state);
}
