//! Tests for [`super::reconcile_source_graph`] and [`super::mark_semantic_stale`].

use super::*;
use crate::context::source_graph::SourceNodeKind;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

/// Run one git command with ambient global/system config neutralized, so a
/// developer's or CI runner's `~/.gitconfig` cannot change test behavior.
fn isolated_git(root: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

/// Run one git setup command and assert it succeeded.
fn git_ok(root: &Path, args: &[&str]) {
    let out = isolated_git(root, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A temp git repo with two committed files: a `.rs` (which a real extractor
/// may claim) and a `.txt` (which none does, so it stays file-level-only).
fn init_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git_ok(root, &["init", "-b", "main"]);
    git_ok(root, &["config", "user.email", "t@t.com"]);
    git_ok(root, &["config", "user.name", "t"]);

    std::fs::write(root.join("src.rs"), "fn main() {}\n").unwrap();
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("docs").join("notes.txt"), "hello\n").unwrap();
    git_ok(root, &["add", "src.rs", "docs/notes.txt"]);
    git_ok(root, &["commit", "-m", "seed"]);

    temp
}

fn head_sha(root: &Path) -> String {
    let out = isolated_git(root, &["rev-parse", "HEAD"]);
    assert!(out.status.success(), "rev-parse HEAD failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn stores(temp: &TempDir) -> (ContextStore, GraphStore) {
    let store = ContextStore::with_root(temp.path().join("cache"));
    let graph_store = GraphStore::new(store.root(), &temp.path().join("work"));
    (store, graph_store)
}

#[test]
fn overlay_scope_extracts_every_tracked_file_including_an_unsupported_language() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);

    let scope = SourceGraphScope::Overlay {
        plan: "plan-a".to_string(),
        stage: "stage-a".to_string(),
    };
    let outcome = reconcile_source_graph(&store, &graph_store, root, scope).unwrap();

    assert_eq!(outcome.files_extracted, 2);
    assert!(!outcome.freshness.stale);

    let overlay = graph_store
        .load_overlay("plan-a", "stage-a")
        .unwrap()
        .unwrap();
    assert!(overlay.files.contains_key("src.rs"));

    let txt_entry = overlay
        .files
        .get("docs/notes.txt")
        .expect("the .txt file must still be represented");
    assert!(
        txt_entry
            .nodes
            .iter()
            .any(|node| node.kind == SourceNodeKind::File),
        "a file with no extractor must still keep a file-level node"
    );
}

#[test]
fn an_incremental_overlay_rerun_with_unchanged_bytes_leaves_the_file_untouched() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    let scope = || SourceGraphScope::Overlay {
        plan: "plan-b".to_string(),
        stage: "stage-b".to_string(),
    };

    reconcile_source_graph(&store, &graph_store, root, scope()).unwrap();
    let overlay_path = graph_store.overlay_path("plan-b", "stage-b");
    let before = std::fs::read(&overlay_path).unwrap();

    reconcile_source_graph(&store, &graph_store, root, scope()).unwrap();
    let after = std::fs::read(&overlay_path).unwrap();

    assert_eq!(
        before, after,
        "an unchanged incremental run must not rewrite the overlay"
    );
}

#[test]
fn base_scope_publishes_once_and_a_republish_is_refused_without_erroring() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    let revision = head_sha(root);
    let scope = || SourceGraphScope::Base {
        revision: revision.clone(),
    };

    let first = reconcile_source_graph(&store, &graph_store, root, scope()).unwrap();
    assert_eq!(first.files_extracted, 2);

    // A second build for the same revision must not error, even though the
    // base layer is already published and therefore immutable.
    let second = reconcile_source_graph(&store, &graph_store, root, scope()).unwrap();
    assert_eq!(second.files_extracted, 2);

    let published = graph_store.load_base(&revision).unwrap().unwrap();
    assert_eq!(published.files.len(), 2);
}

#[test]
fn mark_semantic_stale_sets_stale_and_detail_and_survives_no_prior_state() {
    let temp = TempDir::new().unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));

    mark_semantic_stale(&store, "sibling merge invalidated the semantic layer").unwrap();

    let state = store.load_state().unwrap();
    assert!(state.semantic.stale);
    assert_eq!(
        state.semantic.detail.as_deref(),
        Some("sibling merge invalidated the semantic layer")
    );
}

#[test]
fn a_changed_file_is_re_extracted_and_the_new_content_lands_in_the_layer() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    let scope = || SourceGraphScope::Overlay {
        plan: "plan-c".to_string(),
        stage: "stage-c".to_string(),
    };

    let first = reconcile_source_graph(&store, &graph_store, root, scope()).unwrap();
    assert_eq!(first.files_extracted, 2);

    let new_bytes = b"a much longer body that is definitely not the seed content\n";
    std::fs::write(root.join("docs").join("notes.txt"), new_bytes).unwrap();

    let second = reconcile_source_graph(&store, &graph_store, root, scope()).unwrap();
    assert_eq!(second.files_extracted, 2);

    let overlay = graph_store
        .load_overlay("plan-c", "stage-c")
        .unwrap()
        .unwrap();
    let entry = overlay.files.get("docs/notes.txt").unwrap();
    assert_eq!(
        entry.content_hash,
        body_hash(new_bytes),
        "a changed file must be re-extracted, not reused from the stale cache entry"
    );
}

#[test]
fn overlay_delta_prunes_files_identical_to_the_base_and_keeps_changed_ones() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);
    let revision = head_sha(root);

    // Publish a base at the current (clean) revision.
    let base_scope = SourceGraphScope::Base {
        revision: revision.clone(),
    };
    reconcile_source_graph(&store, &graph_store, root, base_scope).unwrap();

    // Change one tracked file in the working tree without committing.
    std::fs::write(root.join("docs").join("notes.txt"), "changed\n").unwrap();

    let overlay_scope = SourceGraphScope::Overlay {
        plan: "plan-d".to_string(),
        stage: "stage-d".to_string(),
    };
    reconcile_source_graph(&store, &graph_store, root, overlay_scope).unwrap();

    let overlay = graph_store
        .load_overlay("plan-d", "stage-d")
        .unwrap()
        .unwrap();
    assert!(
        !overlay.files.contains_key("src.rs"),
        "an unchanged file must be pruned from the overlay delta"
    );
    assert!(
        overlay.files.contains_key("docs/notes.txt"),
        "a changed file must remain in the overlay delta"
    );
}

#[test]
fn an_unreadable_file_survives_as_a_reported_lexical_only_entry() {
    let temp = init_repo();
    let root = temp.path();
    let (store, graph_store) = stores(&temp);

    let restricted = root.join("src.rs");
    let original_perms = std::fs::metadata(&restricted).unwrap().permissions();
    std::fs::set_permissions(&restricted, std::fs::Permissions::from_mode(0o000)).unwrap();

    // Root (and some sandboxes) ignore file permission bits entirely; skip
    // the assertion in that case rather than asserting a false failure.
    let still_readable = std::fs::read(&restricted).is_ok();

    let scope = SourceGraphScope::Overlay {
        plan: "plan-e".to_string(),
        stage: "stage-e".to_string(),
    };
    let outcome = reconcile_source_graph(&store, &graph_store, root, scope);

    std::fs::set_permissions(&restricted, original_perms).unwrap();

    if still_readable {
        return;
    }

    let outcome = outcome.unwrap();
    assert_eq!(
        outcome.files_extracted, 2,
        "the unreadable file must still be represented"
    );

    let overlay = graph_store
        .load_overlay("plan-e", "stage-e")
        .unwrap()
        .unwrap();
    let entry = overlay
        .files
        .get("src.rs")
        .expect("an unreadable file must not vanish from the layer");
    match &entry.coverage {
        FileCoverage::LexicalOnly { detail } => assert!(detail.contains("unreadable")),
        other => panic!("expected LexicalOnly coverage naming the failure, got {other:?}"),
    }
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

    assert_eq!(outcome.files_extracted, 0);
    assert!(outcome.freshness.stale);
    assert!(
        graph_store.load_base(&revision).unwrap().is_none(),
        "a dirty tree must not publish a base"
    );

    let state = store.load_state().unwrap();
    assert!(state.semantic.stale, "the refusal must be persisted");
}
