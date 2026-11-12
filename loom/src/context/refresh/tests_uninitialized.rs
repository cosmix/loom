//! The source graph in a checkout `loom init` never ran in, and in one it ran
//! in whose checkout identity has since disappeared.

use super::tests_snapshot::{init_repo, repo_without_state, stores};
use super::*;
use serial_test::serial;

/// What the prompt and context hooks leave in any repository.
fn write_hook_state(root: &Path) {
    let work = root.join(".loom").join("work");
    for dir in ["context", "memory", "telemetry"] {
        std::fs::create_dir_all(work.join(dir)).unwrap();
    }
    std::fs::write(work.join("memory/ad-hoc.md"), "note\n").unwrap();
    std::fs::write(work.join("telemetry/events.jsonl"), "{}\n").unwrap();
}

#[test]
#[serial]
fn hook_created_state_without_init_still_builds_the_snapshot() {
    let temp = repo_without_state();
    write_hook_state(temp.path());
    let (store, graph_store) = stores(&temp);

    let outcome = ensure_snapshot(&store, &graph_store, temp.path(), SnapshotPolicy::BaseOnly);

    assert_eq!(
        outcome.action,
        SnapshotAction::Rebuilt,
        "{}",
        outcome.reason
    );
}

#[test]
#[serial]
fn an_initialized_checkout_without_its_identity_is_unavailable() {
    let temp = init_repo();
    let work = temp.path().join(".loom").join("work");
    std::fs::write(work.join("config.toml"), "[plan]\nplan_id = \"p\"\n").unwrap();
    std::fs::remove_file(work.join("main-checkout-identity.json")).unwrap();
    let (store, graph_store) = stores(&temp);

    let outcome = ensure_snapshot(&store, &graph_store, temp.path(), SnapshotPolicy::BaseOnly);

    assert_eq!(outcome.action, SnapshotAction::Unavailable);
    assert!(
        outcome.reason.contains("checkout identity missing"),
        "{}",
        outcome.reason
    );
}
