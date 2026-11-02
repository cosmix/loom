use super::retrieve::project_with_knowledge;
use crate::context::extract::{extract_file, registry};
use crate::context::graph_store::{FileEntry, GraphLayer, GraphStore};
use crate::context::local_overlay::local_overlay_key;
use crate::context::retrieve::{retrieve_for_stage, StageQuery};
use crate::context::source_graph::GRAPH_SCHEMA_VERSION;
use crate::context::store::ContextStore;
use crate::context::view::{build_cold, ViewIdentity};
use crate::fs::work_dir::WorkDir;
use std::path::Path;
use std::process::Command;

fn entry(path: &str, source: &str) -> FileEntry {
    let parsed = extract_file(&registry(), Path::new(path), source.as_bytes());
    FileEntry::from_extraction(source.as_bytes(), parsed)
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn checkout_head_selects_its_base_without_an_overlay() {
    let temp = project_with_knowledge();
    let root = temp.path();
    git(root, &["init", "-q"]);
    crate::git::worktree::capture_main_identity(root, &root.join(".loom/work")).unwrap();
    git(root, &["config", "user.email", "test@example.invalid"]);
    git(root, &["config", "user.name", "Test"]);
    std::fs::write(root.join("source.rs"), "pub fn alpha_unique() {}\n").unwrap();
    git(root, &["add", "source.rs"]);
    git(root, &["commit", "-qm", "first"]);
    let first = git(root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("source.rs"), "pub fn beta_unique() {}\n").unwrap();
    git(root, &["commit", "-qam", "second"]);
    let second = git(root, &["rev-parse", "HEAD"]);
    git(root, &["checkout", "-q", "--detach", &first]);
    let work = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work).unwrap();
    let graph = GraphStore::new(store.root(), work.root());
    for (revision, name) in [(&first, "alpha_unique"), (&second, "beta_unique")] {
        let mut layer = GraphLayer {
            revision: revision.clone(),
            schema_version: GRAPH_SCHEMA_VERSION,
            ..Default::default()
        };
        layer.files.insert(
            "source.rs".into(),
            entry("source.rs", &format!("pub fn {name}() {{}}")),
        );
        graph.publish_base(revision, &layer).unwrap();
    }
    store
        .update_state(|state| state.semantic.revision = second.clone())
        .unwrap();
    let pack =
        retrieve_for_stage(&StageQuery::new(root, "alpha_unique beta_unique"), 2000).unwrap();
    assert_eq!(pack.semantic_freshness.revision, first);
    assert!(pack
        .items
        .iter()
        .any(|item| item.id.as_str().contains("alpha_unique")));
    assert!(!pack
        .items
        .iter()
        .any(|item| item.id.as_str().contains("beta_unique")));
}

#[test]
fn overlay_uses_its_own_base_for_cold_and_warm_retrieval() {
    let temp = project_with_knowledge();
    let work = WorkDir::new(temp.path()).unwrap();
    let store = ContextStore::open(&work).unwrap();
    let graph = GraphStore::new(store.root(), work.root());
    let (a, b) = distinct_bases();
    graph.publish_base(&a.revision, &a).unwrap();
    graph.publish_base(&b.revision, &b).unwrap();
    let mut overlay = GraphLayer {
        revision: a.revision.clone(),
        generation: "generation-a".into(),
        schema_version: GRAPH_SCHEMA_VERSION,
        ..Default::default()
    };
    overlay
        .files
        .insert("src/deleted.rs".into(), FileEntry::tombstone());
    let (plan, stage) = local_overlay_key(temp.path());
    graph.save_overlay(&plan, &stage, &overlay).unwrap();
    store
        .update_state(|state| state.semantic.revision = b.revision.clone())
        .unwrap();

    // Reproduce a persisted view created by the old B-base/A-overlay read.
    let wrong = build_cold(
        graph.resolved(&b.revision, None).unwrap(),
        ViewIdentity::current(&b.revision, &overlay.generation),
    );
    std::fs::write(
        graph.view_path(&wrong.identity, Some((&plan, &stage))),
        crate::context::store::canonical_json(&wrong).unwrap(),
    )
    .unwrap();

    for _ in 0..2 {
        assert_overlay_read(temp.path(), &graph, &plan, &stage, &a.revision, &b.revision);
    }
    std::fs::remove_file(graph.base_path(&a.revision)).unwrap();
    let pack = retrieve_for_stage(&StageQuery::new(temp.path(), "alpha_unique"), 4000).unwrap();
    assert!(pack.degraded.unwrap().contains("serving overlay only"));
    assert!(!pack
        .items
        .iter()
        .any(|item| item.id.as_str().contains("alpha_unique")));
}

fn distinct_bases() -> (GraphLayer, GraphLayer) {
    let mut a = GraphLayer {
        revision: "revision-a".into(),
        schema_version: GRAPH_SCHEMA_VERSION,
        ..Default::default()
    };
    a.files.insert(
        "src/common.rs".into(),
        entry("src/common.rs", "pub fn alpha_unique() {}"),
    );
    a.files.insert(
        "src/deleted.rs".into(),
        entry("src/deleted.rs", "pub fn removed_unique() {}"),
    );
    let mut b = GraphLayer {
        revision: "revision-b".into(),
        schema_version: GRAPH_SCHEMA_VERSION,
        ..Default::default()
    };
    b.files.insert(
        "src/common.rs".into(),
        entry("src/common.rs", "pub fn beta_unique() {}"),
    );
    b.files.insert(
        "src/added.rs".into(),
        entry("src/added.rs", "pub fn added_unique() {}"),
    );
    (a, b)
}

fn assert_overlay_read(root: &Path, graph: &GraphStore, plan: &str, stage: &str, a: &str, b: &str) {
    let view = graph.view(b, Some((plan, stage))).unwrap();
    assert_eq!(view.graph.base_revision, a);
    assert!(view.graph.files.contains_key("src/common.rs"));
    assert!(!view.graph.files.contains_key("src/added.rs"));
    assert!(!view.graph.files.contains_key("src/deleted.rs"));
    let pack = retrieve_for_stage(
        &StageQuery::new(root, "alpha_unique beta_unique added_unique removed_unique"),
        4000,
    )
    .unwrap();
    assert_eq!(pack.semantic_freshness.revision, a);
    assert!(pack
        .items
        .iter()
        .any(|item| item.id.as_str().contains("alpha_unique")));
    assert!(!pack
        .items
        .iter()
        .any(|item| ["beta_unique", "added_unique", "removed_unique"]
            .iter()
            .any(|name| item.id.as_str().contains(name))));
}

#[test]
fn legacy_overlay_never_inherits_a_requested_base() {
    let temp = project_with_knowledge();
    let work = WorkDir::new(temp.path()).unwrap();
    let store = ContextStore::open(&work).unwrap();
    let graph = GraphStore::new(store.root(), work.root());
    let mut base = GraphLayer {
        revision: "revision-b".into(),
        ..Default::default()
    };
    base.files.insert(
        "src/base.rs".into(),
        entry("src/base.rs", "pub fn base_unique() {}"),
    );
    graph.publish_base(&base.revision, &base).unwrap();
    let mut overlay = GraphLayer::default();
    overlay.files.insert(
        "src/local.rs".into(),
        entry("src/local.rs", "pub fn local_unique() {}"),
    );
    let (plan, stage) = local_overlay_key(temp.path());
    graph.save_overlay(&plan, &stage, &overlay).unwrap();
    let resolved = graph
        .resolved(&base.revision, Some((&plan, &stage)))
        .unwrap();
    assert!(!resolved.files.contains_key("src/base.rs"));
    assert!(resolved.files.contains_key("src/local.rs"));
    let pack = retrieve_for_stage(&StageQuery::new(temp.path(), "local_unique"), 2000).unwrap();
    assert!(pack.degraded.unwrap().contains("no recorded base revision"));
}

#[test]
fn a_loaded_overlay_is_used_for_both_view_identity_and_content() {
    let temp = project_with_knowledge();
    let work = WorkDir::new(temp.path()).unwrap();
    let store = ContextStore::open(&work).unwrap();
    let graph = GraphStore::new(store.root(), work.root());
    let base = GraphLayer {
        revision: "revision-a".into(),
        schema_version: GRAPH_SCHEMA_VERSION,
        ..Default::default()
    };
    graph.publish_base(&base.revision, &base).unwrap();
    let mut overlay = GraphLayer {
        revision: base.revision.clone(),
        generation: "first".into(),
        schema_version: GRAPH_SCHEMA_VERSION,
        ..Default::default()
    };
    overlay.files.insert(
        "src/first.rs".into(),
        entry("src/first.rs", "pub fn first_unique() {}"),
    );
    graph.save_overlay("plan", "stage", &overlay).unwrap();
    let read = graph
        .graph_read("revision-b", Some(("plan", "stage")))
        .unwrap();
    overlay.generation = "second".into();
    overlay.files.clear();
    overlay.files.insert(
        "src/second.rs".into(),
        entry("src/second.rs", "pub fn second_unique() {}"),
    );
    graph.save_overlay("plan", "stage", &overlay).unwrap();
    let view = graph.view_read(&read).unwrap();
    assert_eq!(view.identity.overlay_generation, "first");
    assert!(view.graph.files.contains_key("src/first.rs"));
    assert!(!view.graph.files.contains_key("src/second.rs"));
}
