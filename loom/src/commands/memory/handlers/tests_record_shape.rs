//! Record-shape tests: confirm `note` accepts the `mistake:`/`stale-knowledge:`
//! conventions end to end, split out of `tests.rs` to keep it under the
//! maintainability line limit.

use super::note;
use super::tests::{init_git_repo, EnvGuard};
use crate::fs::memory::read_journal;
use serial_test::serial;
use std::env;

#[test]
#[serial]
fn mistake_note_with_prevention_is_recorded() {
    let _guard = EnvGuard::new();
    let repo = init_git_repo();
    env::set_current_dir(repo.path()).unwrap();
    env::set_var("LOOM_STAGE_ID", "shape-stage");

    note(
        "mistake: tried X because Y. Failed because Z. Prevention: check W. Fix: did V".to_string(),
        Vec::new(),
        None,
    )
    .unwrap();

    let work_dir = repo.path().join(".loom/work");
    let journal = read_journal(&work_dir, "shape-stage").unwrap();
    assert_eq!(journal.entries.len(), 1);
}

#[test]
#[serial]
fn stale_knowledge_note_with_correction_is_recorded() {
    let _guard = EnvGuard::new();
    let repo = init_git_repo();
    env::set_current_dir(repo.path()).unwrap();
    env::set_var("LOOM_STAGE_ID", "shape-stage");

    note(
        "stale-knowledge: doc/loom/knowledge/patterns/foo.md#Some Heading claims X; the tree does Y (foo.rs:1). Correction: use Y"
            .to_string(),
        Vec::new(),
        None,
    )
    .unwrap();

    let work_dir = repo.path().join(".loom/work");
    let journal = read_journal(&work_dir, "shape-stage").unwrap();
    assert_eq!(journal.entries.len(), 1);
}
