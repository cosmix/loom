//! Tests for [`super::reconcile_source_graph`], [`super::mark_semantic_stale`],
//! and [`super::parser_version_matches`].

use super::*;
use crate::context::source_graph::{NodeLanguage, SourceNode, SourceNodeKind};
use crate::context::store::StoreState;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

/// A single-node cache entry stamped with `parser_version`, for exercising
/// [`super::parser_version_matches`] directly without a full extraction.
fn entry_with_parser_version(path: &Path, parser_version: &str) -> FileEntry {
    let node: SourceNode = extract::file_node(
        path,
        b"irrelevant to version matching",
        NodeLanguage::Other("test".to_string()),
        parser_version.to_string(),
        &FileCoverage::Full,
    );
    FileEntry {
        content_hash: "irrelevant".to_string(),
        nodes: vec![node],
        edges: Vec::new(),
        coverage: FileCoverage::Full,
    }
}

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

/// A `Freshness` carrying only a recognizable `revision`, for state.json
/// lost-update regression tests that need distinct per-field values.
fn freshness(revision: &str) -> Freshness {
    Freshness {
        revision: revision.to_string(),
        ..Default::default()
    }
}

/// A `StoreState` with distinct, recognizable revisions in each field, for
/// tests that assert one field survives a locked update to another.
fn seeded_state(structural: &str, semantic: &str, catalog_revision: &str) -> StoreState {
    StoreState {
        structural: freshness(structural),
        semantic: freshness(semantic),
        catalog_revision: catalog_revision.to_string(),
    }
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

    // Root (and some sandboxes - the DEFAULT in most CI containers) ignore
    // file permission bits entirely, in which case this environment cannot
    // exercise the unreadable-file path at all. Skip loudly rather than
    // silently `return`ing a green pass that asserted nothing, and make the
    // skip a hard failure when `LOOM_TEST_REQUIRE_UNREADABLE_FILE=1` is set,
    // mirroring `tests/e2e/tmux_backend.rs`'s `LOOM_E2E_REQUIRE_TMUX`.
    let still_readable = std::fs::read(&restricted).is_ok();

    let scope = SourceGraphScope::Overlay {
        plan: "plan-e".to_string(),
        stage: "stage-e".to_string(),
    };
    let outcome = reconcile_source_graph(&store, &graph_store, root, scope);

    std::fs::set_permissions(&restricted, original_perms).unwrap();

    if still_readable {
        if std::env::var("LOOM_TEST_REQUIRE_UNREADABLE_FILE").as_deref() == Ok("1") {
            panic!(
                "an_unreadable_file_survives_as_a_reported_lexical_only_entry: this \
                 environment does not enforce 0o000 file permissions (running as root, or a \
                 sandbox that ignores mode bits), so the unreadable-file path was never \
                 exercised (LOOM_TEST_REQUIRE_UNREADABLE_FILE=1 demands a real run)"
            );
        }
        eprintln!(
            "SKIP an_unreadable_file_survives_as_a_reported_lexical_only_entry: this \
             environment does not enforce 0o000 file permissions (running as root, or a \
             sandbox that ignores mode bits), so the unreadable-file path was never \
             exercised (set LOOM_TEST_REQUIRE_UNREADABLE_FILE=1 to fail instead)"
        );
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

#[test]
fn a_lexical_fallback_node_stays_current_when_no_extractor_claims_the_path() {
    // Regression test: before the fix, `current` was `None` for a path no
    // extractor supports, and `None == Some(&node.parser_version)` was always
    // false — so every lexically-extracted file (.md, .txt, ...) was judged
    // stale and re-extracted on every incremental build, forever.
    let path = Path::new("docs/notes.txt");
    let entry = entry_with_parser_version(path, extract::lexical::LEXICAL_PARSER_VERSION);
    let extractors = extract::registry();

    assert!(
        parser_version_matches(&entry, &extractors, path),
        "a node that already took the lexical fallback must stay current \
         when no extractor claims the path today"
    );
}

#[test]
fn a_node_from_a_now_missing_extractor_is_still_treated_as_stale() {
    // Guards the case the lexical-fallback fix must not break: a cached node
    // produced by a real extractor (e.g. tree-sitter Rust) that is no longer
    // registered today (e.g. built with `--no-default-features`) must still
    // invalidate, even though no extractor claims the path either.
    let path = Path::new("src.rs");
    let entry = entry_with_parser_version(path, "rust-grammar+deadbeefcafe+v1");
    let extractors: Vec<BoxedExtractor> = Vec::new();

    assert!(
        !parser_version_matches(&entry, &extractors, path),
        "nodes from an extractor that is no longer registered must not be reused"
    );
}

#[test]
fn a_matching_version_from_a_present_extractor_is_current() {
    let path = Path::new("src.rs");
    let extractors = extract::registry();
    let current_version = extractors
        .iter()
        .find(|extractor| extractor.supports(path))
        .expect("the rust extractor must claim a .rs path")
        .cache_identity()
        .to_parser_version();
    let entry = entry_with_parser_version(path, &current_version);

    assert!(parser_version_matches(&entry, &extractors, path));
}

#[test]
fn a_mismatched_version_from_a_present_extractor_is_stale() {
    let path = Path::new("src.rs");
    let extractors = extract::registry();
    let current_version = extractors
        .iter()
        .find(|extractor| extractor.supports(path))
        .expect("the rust extractor must claim a .rs path")
        .cache_identity()
        .to_parser_version();
    let entry = entry_with_parser_version(path, &format!("{current_version}-stale"));

    assert!(!parser_version_matches(&entry, &extractors, path));
}

#[test]
fn an_entry_with_no_nodes_is_always_current() {
    let entry = FileEntry::default();
    let extractors = extract::registry();

    assert!(parser_version_matches(
        &entry,
        &extractors,
        Path::new("whatever.rs")
    ));
}

#[test]
fn rebuild_and_persist_and_persist_semantic_freshness_do_not_revert_each_others_fields() {
    // Regression test for the state.json lost-update bug: `rebuild_and_persist`
    // owns `structural`/`catalog_revision` and `persist_semantic_freshness` owns
    // `semantic` — run both real update paths against one seeded state and
    // confirm neither's locked read-modify-write reverts the field the other
    // just wrote (the old unlocked load-then-save clobbered whichever field the
    // caller's stale in-memory snapshot did not carry forward).
    let temp = TempDir::new().unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));
    let knowledge_root = temp.path().join("knowledge");
    std::fs::create_dir_all(&knowledge_root).unwrap();
    store
        .save_state(&seeded_state("", "seed-semantic", "seed-catalog"))
        .unwrap();

    // A stale `semantic` snapshot, as `evaluate` would have captured it before
    // a concurrent semantic update landed — the old bug wrote this straight
    // back to disk, reverting whatever `semantic` actually held.
    let stale_semantic = freshness("stale-snapshot-from-evaluate");
    crate::context::refresh::rebuild_and_persist(&store, &knowledge_root, stale_semantic).unwrap();

    let after_rebuild = store.load_state().unwrap();
    assert_eq!(
        after_rebuild.semantic.revision, "seed-semantic",
        "rebuild_and_persist must not overwrite `semantic` with its stale snapshot"
    );
    assert_ne!(
        after_rebuild.catalog_revision, "seed-catalog",
        "rebuild_and_persist must still update the field it owns"
    );

    persist_semantic_freshness(&store, "fresh-semantic-revision".to_string()).unwrap();

    let after_semantic = store.load_state().unwrap();
    assert_eq!(
        after_semantic.catalog_revision, after_rebuild.catalog_revision,
        "persist_semantic_freshness must not revert catalog_revision"
    );
    assert_eq!(after_semantic.semantic.revision, "fresh-semantic-revision");
}

#[test]
fn update_state_leaves_fields_the_closure_does_not_assign_untouched() {
    // Pins the `update_state` invariant directly, one level below the two
    // real call sites exercised above: a closure that assigns only one field
    // must not disturb any other field, because the read that seeds it is
    // fresh and inside the same lock as the write.
    let temp = TempDir::new().unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));

    let seeded = seeded_state("seed-structural", "seed-semantic", "seed-catalog");
    store.save_state(&seeded).unwrap();

    store
        .update_state(|state| state.catalog_revision = "updated-catalog".to_string())
        .unwrap();

    let after = store.load_state().unwrap();
    assert_eq!(after.catalog_revision, "updated-catalog");
    assert_eq!(
        after.structural, seeded.structural,
        "update_state must not disturb a field the closure did not assign"
    );
    assert_eq!(
        after.semantic, seeded.semantic,
        "update_state must not disturb a field the closure did not assign"
    );
}

/// [`init_repo`] plus the `doc/loom/knowledge/` tree `refresh` requires: it
/// derives the project root by walking three ancestors up from the knowledge
/// root and refuses to guess when that layout does not match.
fn init_repo_with_knowledge() -> TempDir {
    let temp = init_repo();
    let root = temp.path();
    let knowledge = root.join("doc").join("loom").join("knowledge");
    std::fs::create_dir_all(&knowledge).unwrap();
    std::fs::write(
        knowledge.join("architecture.md"),
        "# Architecture\n\nOne section, so the catalog has something to ingest.\n",
    )
    .unwrap();
    git_ok(root, &["add", "doc/loom/knowledge/architecture.md"]);
    git_ok(root, &["commit", "-m", "knowledge"]);
    // `refresh` resolves the graph store through `WorkDir::new(project_root)`,
    // which yields `<root>/.work` once that directory exists. Creating it here
    // pins the layer location instead of letting the upward search find some
    // ancestor's `.work`.
    std::fs::create_dir_all(root.join(".work")).unwrap();
    temp
}

/// The store and graph store `refresh` itself will construct for `root`, so a
/// test reads back the layer that the real call actually wrote.
fn refresh_stores(root: &Path) -> (ContextStore, GraphStore) {
    let store = ContextStore::with_root(root.join("cache"));
    store.ensure().unwrap();
    let graph_store = GraphStore::new(store.root(), &root.join(".work"));
    (store, graph_store)
}

#[test]
fn test_clean_tree_publishes_base() {
    let temp = init_repo_with_knowledge();
    let root = temp.path();
    let (store, graph_store) = refresh_stores(root);

    let outcome = crate::context::refresh::refresh(
        &store,
        &root.join("doc").join("loom").join("knowledge"),
        false,
    )
    .unwrap();

    let head = head_sha(root);
    match &outcome.semantic.layer {
        crate::context::refresh::SemanticLayer::Base { revision } => assert_eq!(*revision, head),
        other => panic!("a clean tree must publish a base layer, got {other:?}"),
    }
    assert!(
        outcome.semantic.files_extracted > 0 && outcome.semantic.nodes > 0,
        "a base publish must report the layer it actually built, got {:?}",
        outcome.semantic
    );
    assert!(
        graph_store.load_base(&head).unwrap().is_some(),
        "the base layer must be readable back at the revision it was published for"
    );
    assert!(
        std::fs::read_dir(graph_store.base_dir())
            .unwrap()
            .next()
            .is_some(),
        "a base publish must leave a layer file under graph/base/"
    );
}

#[test]
fn test_dirty_tree_falls_back_to_local_overlay() {
    let temp = init_repo_with_knowledge();
    let root = temp.path();
    let (store, _graph_store) = refresh_stores(root);

    // Modify a TRACKED file: `dirty_tree_reason` runs with
    // `--untracked-files=no`, so an untracked scratch file would not refuse.
    std::fs::write(root.join("src.rs"), "fn main() { let x = 1; }\n").unwrap();

    let outcome = crate::context::refresh::refresh(
        &store,
        &root.join("doc").join("loom").join("knowledge"),
        false,
    )
    .unwrap();

    let (expected_plan, expected_stage) = crate::context::local_overlay::local_overlay_key(root);
    match &outcome.semantic.layer {
        crate::context::refresh::SemanticLayer::LocalOverlay {
            plan,
            stage,
            refusal,
        } => {
            assert_eq!(*plan, expected_plan);
            assert_eq!(*stage, expected_stage);
            assert!(
                !refusal.is_empty(),
                "the fallback must name why the base was refused"
            );
        }
        other => panic!("a dirty tree must fall back to the working-tree overlay, got {other:?}"),
    }
    assert!(
        outcome.semantic.files_extracted > 0 && outcome.semantic.nodes > 0,
        "the fallback must report the overlay it really built, not a silent zero-count \
         degraded outcome: {:?}",
        outcome.semantic
    );
}

#[test]
fn test_dirty_tree_overlay_is_readable_through_local_scope() {
    // THE REGRESSION GUARD FOR THE WHOLE FALLBACK. Writing an overlay nobody
    // can read is indistinguishable from writing nothing, so this resolves the
    // graph the way retrieval does - through `local_overlay_key` - and proves
    // the address the writer used is the address the reader looks at.
    let temp = init_repo_with_knowledge();
    let root = temp.path();
    let (store, graph_store) = refresh_stores(root);

    std::fs::write(root.join("src.rs"), "fn main() { let x = 1; }\n").unwrap();

    let outcome = crate::context::refresh::refresh(
        &store,
        &root.join("doc").join("loom").join("knowledge"),
        false,
    )
    .unwrap();

    assert!(
        graph_store.load_base(&head_sha(root)).unwrap().is_none(),
        "no base was ever published here, so anything readable below comes from the overlay"
    );

    let (plan, stage) = crate::context::local_overlay::local_overlay_key(root);
    let resolved = graph_store
        .resolved(
            &outcome.semantic.freshness.revision,
            Some((plan.as_str(), stage.as_str())),
        )
        .unwrap();

    assert!(
        !resolved.files.is_empty(),
        "the overlay the dirty-tree fallback wrote must be readable through the local scope"
    );
    assert!(
        resolved.nodes().count() > 0,
        "a readable overlay with no nodes would still leave retrieval with nothing"
    );
}

#[test]
fn test_structural_only_skips_semantic_layer() {
    let temp = init_repo_with_knowledge();
    let root = temp.path();
    let (store, graph_store) = refresh_stores(root);

    let outcome = crate::context::refresh::refresh(
        &store,
        &root.join("doc").join("loom").join("knowledge"),
        true,
    )
    .unwrap();

    match &outcome.semantic.layer {
        crate::context::refresh::SemanticLayer::Skipped { reason } => assert!(
            reason.contains("structural-only"),
            "the skip must name the flag that caused it, got {reason:?}"
        ),
        other => panic!("--structural-only must not touch the semantic layer, got {other:?}"),
    }
    assert_eq!(outcome.semantic.files_extracted, 0);
    assert_eq!(outcome.semantic.nodes, 0);

    let (plan, stage) = crate::context::local_overlay::local_overlay_key(root);
    assert!(
        graph_store.load_base(&head_sha(root)).unwrap().is_none(),
        "--structural-only must publish no base layer"
    );
    assert!(
        graph_store.load_overlay(&plan, &stage).unwrap().is_none(),
        "--structural-only must write no overlay either"
    );
}
