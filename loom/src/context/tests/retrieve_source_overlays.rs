//! Retrieval over the layers a real reconcile leaves behind: a complete stage
//! overlay at an unpublished HEAD, a delta whose base was pruned, and a checkout
//! whose HEAD has neither an overlay nor a base.

use super::super::retrieve::project_with_knowledge;
use super::{distinctive_node, write_local_layer, DISTINCTIVE_SYMBOL};
use crate::commands::hook::reconcile_graph::wants_rebuild;
use crate::context::freshness::GraphState;
use crate::context::graph_store::GraphStore;
use crate::context::local_overlay::OverlayScope;
use crate::context::refresh::{ensure_snapshot, SnapshotAction, SnapshotPolicy};
use crate::context::retrieve::{retrieve_for_stage, StageQuery};
use crate::context::schema::ContextPack;
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;
use serial_test::serial;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const PLAN: &str = "plan";
const STAGE: &str = "stage";

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn commit_all(root: &Path, message: &str) -> String {
    git(root, &["add", "doc", "src"]);
    git(root, &["commit", "-qm", message]);
    git(root, &["rev-parse", "HEAD"])
}

/// A committed checkout with two source files and no graph layer.
fn repository() -> TempDir {
    let temp = project_with_knowledge();
    let root = temp.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "t@t.com"]);
    git(root, &["config", "user.name", "t"]);
    write_source(root, "src/kept.rs", "pub fn kept_zorble() {\n}\n");
    write_source(root, "src/edited.rs", "pub fn edited_zorble() {\n}\n");
    commit_all(root, "seed");
    crate::git::worktree::capture_main_identity(root, &root.join(".loom/work")).unwrap();
    temp
}

fn write_source(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn stores(root: &Path) -> (ContextStore, GraphStore) {
    let work = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work).unwrap();
    let graph = GraphStore::new(store.root(), work.root());
    (store, graph)
}

fn stage_policy() -> SnapshotPolicy {
    SnapshotPolicy::StageOverlay {
        plan: PLAN.to_string(),
        stage: STAGE.to_string(),
    }
}

fn snapshot(root: &Path, policy: SnapshotPolicy) -> SnapshotAction {
    let (store, graph) = stores(root);
    ensure_snapshot(&store, &graph, root, policy).action
}

fn stage_pack(root: &Path, text: &str) -> ContextPack {
    let mut query = StageQuery::new(root, text);
    query.overlay = OverlayScope::Stage {
        plan: PLAN.to_string(),
        stage: STAGE.to_string(),
    };
    retrieve_for_stage(&query, 4000).unwrap()
}

fn has_item(pack: &ContextPack, name: &str) -> bool {
    pack.items
        .iter()
        .any(|item| item.id.as_str().contains(name))
}

fn assert_current_without_rebuild(pack: &ContextPack) {
    assert_eq!(pack.degraded, None);
    assert_eq!(
        pack.semantic_freshness.state(),
        GraphState::Current,
        "{:?}",
        pack.semantic_freshness
    );
    assert!(
        !wants_rebuild(&pack.semantic_freshness, pack.degraded.as_deref()),
        "a complete overlay must not trip a background rebuild"
    );
}

// `ContextPack::degraded` is a live input to `reconcile_graph::spawn_if_needed`:
// reporting a complete overlay as degraded would start a background rebuild from
// every prompt in that checkout, each one reusing the same overlay and changing
// nothing. Only a delta needs the base it was cut from.

#[test]
fn retrieve_for_stage_is_not_degraded_when_an_overlay_covers_a_missing_base() {
    let temp = project_with_knowledge();
    let root = temp.path();
    let node = distinctive_node();
    write_local_layer(root, &node, true);
    let (store, _) = stores(root);
    store
        .update_state(|state| state.semantic.revision = "deadbeef00".to_string())
        .unwrap();

    let query = StageQuery::new(root, format!("Where is {DISTINCTIVE_SYMBOL} defined?"));
    let pack = retrieve_for_stage(&query, 500).unwrap();

    assert_eq!(
        pack.degraded, None,
        "an overlay that covers a missing base must never read as degraded"
    );
    assert!(
        has_item(&pack, &node.id),
        "the overlay's own content must still be packed: {:?}",
        pack.items
    );
}

#[test]
fn retrieve_for_stage_is_not_degraded_when_an_overlay_backs_an_empty_semantic_revision() {
    let temp = project_with_knowledge();
    let root = temp.path();
    write_local_layer(root, &distinctive_node(), true);
    let query = StageQuery::new(root, format!("Where is {DISTINCTIVE_SYMBOL} defined?"));

    let pack = retrieve_for_stage(&query, 500).unwrap();

    assert_eq!(pack.degraded, None, "{:?}", pack.degraded);
}

#[test]
#[serial]
fn complete_stage_overlay_at_an_unpublished_head_reads_current_with_windows() {
    let temp = repository();
    let root = temp.path();
    let head = git(root, &["rev-parse", "HEAD"]);
    assert_eq!(snapshot(root, stage_policy()), SnapshotAction::Rebuilt);
    assert!(stores(root).1.load_base(&head).unwrap().is_none());

    let pack = stage_pack(root, "kept_zorble");

    assert_current_without_rebuild(&pack);
    assert_eq!(pack.semantic_freshness.revision, head);
    assert!(
        pack.items.iter().any(|item| item.window.is_some()),
        "a current graph vouches for its windows: {:?}",
        pack.items
    );
    assert_eq!(snapshot(root, stage_policy()), SnapshotAction::Reused);
    assert!(
        stores(root)
            .1
            .overlay_dir(PLAN, STAGE)
            .join("view.json")
            .is_file(),
        "a complete overlay's view is persisted, not rebuilt on every read"
    );
}

#[test]
#[serial]
fn complete_stage_overlay_reads_current_when_another_checkout_owns_the_pointer() {
    let temp = repository();
    let root = temp.path();
    snapshot(root, stage_policy());
    let (store, _) = stores(root);
    store
        .update_state(|state| state.semantic.revision = "f".repeat(40))
        .unwrap();

    let pack = stage_pack(root, "kept_zorble");

    assert_current_without_rebuild(&pack);
    assert!(pack.items.iter().any(|item| item.window.is_some()));
}

/// A complete overlay drops its tombstones for want of a base, so a base
/// published later must not be unioned under it.
#[test]
#[serial]
fn base_published_after_a_complete_overlay_does_not_resurrect_a_deleted_file() {
    let temp = repository();
    let root = temp.path();
    std::fs::remove_file(root.join("src/edited.rs")).unwrap();
    snapshot(root, stage_policy());
    snapshot(root, SnapshotPolicy::BaseOnly);
    assert_eq!(snapshot(root, stage_policy()), SnapshotAction::Reused);

    let pack = stage_pack(root, "kept_zorble edited_zorble");

    assert!(has_item(&pack, "kept_zorble"));
    assert!(!has_item(&pack, "edited_zorble"), "{:?}", pack.items);
}

/// Publish a base, leave a delta overlay over it, then prune the base.
fn delta_overlay_with_a_pruned_base(root: &Path) {
    snapshot(root, SnapshotPolicy::BaseOnly);
    write_source(root, "src/edited.rs", "pub fn edited_later_zorble() {\n}\n");
    snapshot(root, stage_policy());
    let (_, graph) = stores(root);
    graph.prune_base_graphs(0, &[]).unwrap();
}

#[test]
#[serial]
fn pruned_base_of_a_delta_overlay_is_reported_then_rebuilt_complete() {
    let temp = repository();
    let root = temp.path();
    delta_overlay_with_a_pruned_base(root);

    let collapsed = stage_pack(root, "kept_zorble edited_later_zorble");
    assert!(collapsed
        .degraded
        .as_deref()
        .is_some_and(|reason| reason.contains("serving overlay only")));
    assert_eq!(collapsed.semantic_freshness.state(), GraphState::Stale);
    assert!(!has_item(&collapsed, "kept_zorble"));

    assert_ne!(
        snapshot(root, stage_policy()),
        SnapshotAction::Reused,
        "a delta whose base is gone is not current at its generation"
    );

    let rebuilt = stage_pack(root, "kept_zorble edited_later_zorble");
    assert_current_without_rebuild(&rebuilt);
    assert!(has_item(&rebuilt, "kept_zorble"));
    assert!(has_item(&rebuilt, "edited_later_zorble"));
    assert_eq!(snapshot(root, stage_policy()), SnapshotAction::Reused);
}

/// A delta that already holds every file gets the same files when rebuilt, and
/// must still be rewritten as complete or it stays not-current forever.
#[test]
#[serial]
fn rebuilding_a_delta_that_holds_every_file_records_it_complete() {
    let temp = repository();
    let root = temp.path();
    git(root, &["rm", "-rq", "doc"]);
    git(root, &["commit", "-qm", "only source files remain"]);
    snapshot(root, SnapshotPolicy::BaseOnly);
    write_source(root, "src/kept.rs", "pub fn kept_again_zorble() {\n}\n");
    write_source(root, "src/edited.rs", "pub fn edited_again_zorble() {\n}\n");
    snapshot(root, stage_policy());
    stores(root).1.prune_base_graphs(0, &[]).unwrap();

    assert_ne!(snapshot(root, stage_policy()), SnapshotAction::Reused);

    assert_eq!(snapshot(root, stage_policy()), SnapshotAction::Reused);
}

/// Commit past a published base without any reconcile.
fn head_past_its_published_base(root: &Path) -> (String, String) {
    let older = git(root, &["rev-parse", "HEAD"]);
    snapshot(root, SnapshotPolicy::BaseOnly);
    write_source(root, "src/added.rs", "pub fn added_zorble() {\n}\n");
    let head = commit_all(root, "second");
    (older, head)
}

fn assert_labelled_older_base(pack: &ContextPack, older: &str, head: &str) {
    assert!(has_item(pack, "kept_zorble"), "{:?}", pack.items);
    assert!(!has_item(pack, "added_zorble"));
    assert_eq!(pack.semantic_freshness.revision, older);
    assert_eq!(pack.semantic_freshness.state(), GraphState::Stale);
    let detail = pack.semantic_freshness.detail.as_deref().unwrap();
    assert!(detail.contains(&older[..8]), "{detail}");
    assert!(detail.contains(&head[..8]), "{detail}");
    assert!(
        pack.items.iter().all(|item| item.window.is_none()),
        "an older base vouches for no line of this checkout"
    );
    assert!(wants_rebuild(
        &pack.semantic_freshness,
        pack.degraded.as_deref()
    ));
}

#[test]
#[serial]
fn head_without_overlay_or_base_serves_the_newest_base_labelled_stale() {
    let temp = repository();
    let root = temp.path();
    let (older, head) = head_past_its_published_base(root);

    let pack = retrieve_for_stage(&StageQuery::new(root, "kept_zorble added_zorble"), 4000);

    assert_labelled_older_base(&pack.unwrap(), &older, &head);
}

/// The shared pointer already naming the served base must not make it read as
/// current for a checkout at another HEAD.
#[test]
#[serial]
fn older_base_stays_labelled_when_the_pointer_names_it() {
    let temp = repository();
    let root = temp.path();
    let (older, head) = head_past_its_published_base(root);
    let (store, _) = stores(root);
    store
        .update_state(|state| {
            state.semantic.revision.clone_from(&older);
            state.semantic.stale = false;
            state.semantic.detail = None;
        })
        .unwrap();

    let pack = stage_pack(root, "kept_zorble added_zorble");

    assert_labelled_older_base(&pack, &older, &head);
}

#[test]
#[serial]
fn delta_overlay_never_falls_back_to_another_revisions_base() {
    let temp = repository();
    let root = temp.path();
    delta_overlay_with_a_pruned_base(root);
    write_source(root, "src/other.rs", "pub fn other_zorble() {\n}\n");
    let newest = commit_all(root, "second");
    snapshot(root, SnapshotPolicy::BaseOnly);
    let (_, graph) = stores(root);
    assert!(graph.load_base(&newest).unwrap().is_some());

    let pack = stage_pack(root, "other_zorble");

    assert!(!has_item(&pack, "other_zorble"), "{:?}", pack.items);
}
