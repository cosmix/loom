//! Contract: the structural revision a refresh stores describes the bytes the
//! catalog build indexed, so a file edited after the freshness pre-pass and
//! reverted after the build reads as stale.

use tempfile::TempDir;

use super::{evaluate, refresh, BEFORE_BUILD};
use crate::context::store::ContextStore;

const OLD: &str = "# Topic\n\n## Section\n\nOLD body.\n";
const NEW: &str = "# Topic\n\n## Section\n\nNEW body.\n";

#[test]
fn an_edit_and_revert_during_the_build_leaves_the_revision_stale() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("knowledge");
    std::fs::create_dir_all(&root).unwrap();
    let topic = root.join("topic.md");
    std::fs::write(&topic, OLD).unwrap();
    let store = ContextStore::with_root(temp.path().join("cache"));

    let edited = topic.clone();
    BEFORE_BUILD.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            std::fs::write(&edited, NEW).unwrap();
        }));
    });
    let first = refresh(&store, &root, true).unwrap();
    assert!(first.rebuilt, "the first refresh builds the catalog");
    std::fs::write(&topic, OLD).unwrap();

    let state = evaluate(&store, &root).unwrap();
    assert!(
        state.structural.stale,
        "the stored revision must describe the NEW bytes the build indexed, so the \
         restored OLD tree reads as stale: {:?}",
        state.structural
    );
    let second = refresh(&store, &root, true).unwrap();
    assert!(
        second.rebuilt,
        "a refresh after the revert must rebuild the catalog"
    );
}
