//! Tests for [`super::evaluate`]'s semantic-freshness comparison against
//! `git rev-parse HEAD` (`semantic_freshness_against_head`, `short_revision`).

use super::*;
use crate::context::freshness::GraphState;
use crate::context::store::StoreState;
use crate::test_git::git as git_ok;
use crate::test_git::TestRepo;
use std::path::PathBuf;
use tempfile::TempDir;

fn head_sha(root: &Path) -> String {
    git_ok(root, &["rev-parse", "HEAD"])
}

/// A git repo rooted at `temp.path()`, with `doc/loom/knowledge/` present so
/// `derive_project_root` recognizes the layout, and one committed file.
/// Returns `(repo, knowledge_root, first commit sha)`.
fn init_repo_with_knowledge_root() -> (TempDir, PathBuf, String) {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
    TestRepo::builder()
        .identity("t", "t@t.com")
        .no_seed()
        .init_in(&root);

    let knowledge_root = root.join("doc/loom/knowledge");
    std::fs::create_dir_all(&knowledge_root).unwrap();
    std::fs::write(knowledge_root.join("architecture.md"), "# Architecture\n").unwrap();
    git_ok(&root, &["add", "doc"]);
    git_ok(&root, &["commit", "-m", "seed"]);

    let first = head_sha(&root);
    (temp, knowledge_root, first)
}

/// A `ContextStore` under `temp` with `state.json` pre-seeded to carry
/// `semantic_revision` and nothing else — structural stays default (empty,
/// never built), which is irrelevant to these tests.
fn seeded_store(temp: &TempDir, semantic_revision: &str) -> ContextStore {
    let store = ContextStore::with_root(temp.path().join("cache"));
    store
        .save_state(&StoreState {
            semantic: Freshness {
                revision: semantic_revision.to_string(),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    store
}

#[test]
fn semantic_is_reported_stale_when_head_moves_past_the_stored_revision() {
    let (temp, knowledge_root, first) = init_repo_with_knowledge_root();
    let root = temp.path();

    std::fs::write(knowledge_root.join("patterns.md"), "# Patterns\n").unwrap();
    git_ok(root, &["add", "doc"]);
    git_ok(root, &["commit", "-m", "second"]);
    let second = head_sha(root);
    assert_ne!(first, second);

    let store = seeded_store(&temp, &first);

    let state = evaluate(&store, &knowledge_root).unwrap();

    assert!(state.semantic.stale);
    assert_eq!(
        state.semantic.revision, first,
        "revision must stay the STORED one — it names the base layer on disk"
    );
    let detail = state
        .semantic
        .detail
        .expect("a newly-stale semantic freshness needs a detail");
    assert!(
        detail.contains(&first[..8]),
        "detail must name the old short revision: {detail}"
    );
    assert!(
        detail.contains(&second[..8]),
        "detail must name the new short revision: {detail}"
    );
}

#[test]
fn semantic_stays_fresh_when_head_matches_the_stored_revision() {
    let (temp, knowledge_root, first) = init_repo_with_knowledge_root();
    let store = seeded_store(&temp, &first);

    let state = evaluate(&store, &knowledge_root).unwrap();

    assert!(!state.semantic.stale);
    assert_eq!(state.semantic.revision, first);
    assert!(state.semantic.detail.is_none());
}

#[test]
fn a_non_git_directory_reports_the_stored_semantic_layer_unavailable() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let knowledge_root = root.join("doc/loom/knowledge");
    std::fs::create_dir_all(&knowledge_root).unwrap();
    std::fs::write(knowledge_root.join("architecture.md"), "# Architecture\n").unwrap();

    let store = ContextStore::with_root(root.join("cache"));
    let stored = Freshness {
        revision: "deadbeef".to_string(),
        stale: true,
        detail: Some("stale for an unrelated, pre-existing reason".to_string()),
        ..Default::default()
    };
    store
        .save_state(&StoreState {
            semantic: stored.clone(),
            ..Default::default()
        })
        .unwrap();

    let state = evaluate(&store, &knowledge_root).unwrap();

    assert_eq!(state.semantic.state(), GraphState::Unavailable);
    assert_eq!(
        state.semantic.revision, stored.revision,
        "the stored revision names the base on disk and is kept"
    );
    assert!(
        state.semantic.detail.is_some(),
        "an unavailable verdict needs a detail"
    );
}

#[test]
fn an_empty_stored_semantic_revision_still_reports_never_built() {
    let (temp, knowledge_root, _first) = init_repo_with_knowledge_root();
    let store = ContextStore::with_root(temp.path().join("cache")); // no state.json written at all

    let state = evaluate(&store, &knowledge_root).unwrap();

    assert!(state.semantic.stale);
    assert!(state.semantic.revision.is_empty());
    assert_eq!(
        state.semantic.detail.as_deref(),
        Some("source graph not built; see the source-graph stage")
    );
}

#[test]
fn a_stored_revision_shorter_than_eight_characters_does_not_panic() {
    let (temp, knowledge_root, _first) = init_repo_with_knowledge_root();
    let store = seeded_store(&temp, "abc");

    let state = evaluate(&store, &knowledge_root).unwrap();

    assert!(state.semantic.stale);
    assert_eq!(state.semantic.revision, "abc");
    let detail = state
        .semantic
        .detail
        .expect("a newly-stale semantic freshness needs a detail");
    assert!(
        detail.contains("abc"),
        "a revision shorter than 8 chars must render whole, not panic: {detail}"
    );
}

#[test]
fn refresh_fingerprints_the_tree_once_per_call() {
    let (temp, knowledge_root, _first) = init_repo_with_knowledge_root();
    let store = ContextStore::with_root(temp.path().join("cache"));
    take_fingerprint_pass_count();

    refresh(&store, &knowledge_root, true).unwrap();

    assert_eq!(take_fingerprint_pass_count(), 1);
}

#[test]
fn rebuilt_revision_matches_the_pre_pass_revision_with_index_and_prose() {
    let (temp, knowledge_root, _first) = init_repo_with_knowledge_root();
    crate::fs::knowledge::index::write_index(&knowledge_root).unwrap();
    std::fs::write(
        temp.path().join("doc/design.md"),
        "# Design\n\nA prose design note.\n",
    )
    .unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));

    let outcome = refresh(&store, &knowledge_root, true).unwrap();

    assert!(outcome.rebuilt);
    let pre_pass = tree_revision(&fingerprint_tree(&knowledge_root).unwrap());
    assert_eq!(outcome.structural.revision, pre_pass);
    let state = evaluate(&store, &knowledge_root).unwrap();
    assert!(!state.structural.stale, "{:?}", state.structural);
}

#[test]
fn a_nested_index_md_leaves_the_refreshed_revision_equal_to_the_evaluated_one() {
    let (temp, knowledge_root, _first) = init_repo_with_knowledge_root();
    std::fs::create_dir_all(knowledge_root.join("architecture")).unwrap();
    std::fs::write(
        knowledge_root.join("architecture/INDEX.md"),
        "# nested index\n",
    )
    .unwrap();
    std::fs::write(knowledge_root.join("architecture/topic.md"), "# Topic\n").unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));

    let outcome = refresh(&store, &knowledge_root, true).unwrap();

    let fingerprints = fingerprint_tree(&knowledge_root).unwrap();
    assert!(
        !fingerprints
            .iter()
            .any(|print| print.path == Path::new("architecture/INDEX.md")),
        "a nested INDEX.md is not part of the fingerprinted set"
    );
    assert_eq!(outcome.structural.revision, tree_revision(&fingerprints));
    let state = evaluate(&store, &knowledge_root).unwrap();
    assert_eq!(state.structural.revision, outcome.structural.revision);
    assert!(!state.structural.stale, "{:?}", state.structural);
}

#[cfg(unix)]
#[test]
fn a_symlinked_root_index_md_is_skipped_and_does_not_fail_refresh() {
    let (temp, knowledge_root, _first) = init_repo_with_knowledge_root();
    let target = temp.path().join("elsewhere.md");
    std::fs::write(&target, "# not an index\n").unwrap();
    std::os::unix::fs::symlink(&target, knowledge_root.join("INDEX.md")).unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));

    let outcome = refresh(&store, &knowledge_root, true).unwrap();

    let fingerprints = fingerprint_tree(&knowledge_root).unwrap();
    assert!(!fingerprints
        .iter()
        .any(|print| print.path == Path::new("INDEX.md")));
    assert_eq!(outcome.structural.revision, tree_revision(&fingerprints));
    let state = evaluate(&store, &knowledge_root).unwrap();
    assert!(!state.structural.stale, "{:?}", state.structural);
}
