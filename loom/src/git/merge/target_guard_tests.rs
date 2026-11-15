//! `merge_stage` consults the target guard: a held target blocks every gate
//! mode, and loom's own merge keeps the accepted tip current.

use super::test_support::{commit_count, init_repo, lock_dir, rev};
use super::*;
use crate::git::target_guard::{accepted_tip, check, RECORD_FILE};
use crate::test_git::{commit_file, git};

/// `loom/s1` with `b.txt` from `main`, `main` checked out again.
fn stage_branch(root: &Path) {
    super::test_support::stage_branch(root, "s1", &[("b.txt", "branch")]);
}

fn record(root: &Path, work: &Path) -> String {
    let state = check(root, work, "main").unwrap().unwrap();
    let main = rev(root, "main");
    assert_eq!(
        state,
        GuardState::Clear {
            accepted: main.clone()
        }
    );
    main
}

/// After the record, `main` gains a control-path commit outside loom.
fn assert_held_merge_is_blocked(gate: MergeGate) {
    let repo = init_repo();
    let root = repo.path();
    let work = lock_dir();
    stage_branch(root);
    let accepted = record(root, work.path());
    std::fs::create_dir_all(root.join(".claude")).unwrap();
    commit_file(root, ".claude/settings.json", "{}", "settings");
    let observed = rev(root, "main");
    let commits = commit_count(root);

    let result = merge_stage("s1", "main", root, work.path(), gate).unwrap();

    match result {
        MergeResult::Blocked(MergeBlock::TargetHeld {
            target,
            accepted: a,
            observed: o,
        }) => assert_eq!(
            (target.as_str(), a, o),
            ("main", accepted, observed.clone())
        ),
        other => panic!("expected TargetHeld, got {other:?}"),
    }
    assert_eq!(rev(root, "main"), observed);
    assert_eq!(commit_count(root), commits, "a held merge writes no commit");
}

#[test]
fn merge_stage_blocks_a_held_target_and_writes_no_commit() {
    assert_held_merge_is_blocked(MergeGate::Enforce);
}

#[test]
fn the_gate_bypass_does_not_bypass_a_held_target() {
    assert_held_merge_is_blocked(MergeGate::Bypass);
}

#[test]
fn a_merge_through_update_ref_moves_the_accepted_tip() {
    let repo = init_repo();
    let root = repo.path();
    let work = lock_dir();
    stage_branch(root);
    record(root, work.path());
    git(root, &["checkout", "-b", "elsewhere"]);

    let result = merge_stage("s1", "main", root, work.path(), MergeGate::Enforce).unwrap();

    assert!(matches!(result, MergeResult::Success { .. }), "{result:?}");
    let merged = rev(root, "main");
    assert_eq!(accepted_tip(work.path(), "main").unwrap(), Some(merged));
}

#[test]
fn a_merge_through_the_checkout_moves_the_accepted_tip() {
    let repo = init_repo();
    let root = repo.path();
    let work = lock_dir();
    stage_branch(root);
    record(root, work.path());

    let result = merge_stage(
        "s1",
        "refs/heads/main",
        root,
        work.path(),
        MergeGate::Enforce,
    );

    assert!(
        matches!(result, Ok(MergeResult::Success { .. })),
        "{result:?}"
    );
    let merged = rev(root, "main");
    assert_eq!(accepted_tip(work.path(), "main").unwrap(), Some(merged));
}

#[test]
fn a_merge_with_no_record_entry_records_the_target_and_advances_it() {
    let repo = init_repo();
    let root = repo.path();
    let work = lock_dir();
    stage_branch(root);
    assert_eq!(accepted_tip(work.path(), "main").unwrap(), None);

    let result = merge_stage("s1", "main", root, work.path(), MergeGate::Enforce).unwrap();

    assert!(matches!(result, MergeResult::Success { .. }), "{result:?}");
    let merged = rev(root, "main");
    assert_eq!(accepted_tip(work.path(), "main").unwrap(), Some(merged));
}

#[test]
fn a_merge_with_an_unreadable_record_is_held_and_says_so() {
    let repo = init_repo();
    let root = repo.path();
    let work = lock_dir();
    stage_branch(root);
    let record = work.path().join(RECORD_FILE);
    std::fs::write(&record, "{not json").unwrap();
    let main = rev(root, "main");
    let commits = commit_count(root);

    let result = merge_stage("s1", "main", root, work.path(), MergeGate::Bypass).unwrap();

    let block = match result {
        MergeResult::Blocked(block) => block,
        other => panic!("expected a block, got {other:?}"),
    };
    let expected = MergeBlock::TargetHeld {
        target: "main".to_string(),
        accepted: String::new(),
        observed: main.clone(),
    };
    assert_eq!(block, expected);
    let text = block.to_string();
    assert!(text.contains("record could not be read"), "{text}");
    assert!(!text.contains("moved outside loom"), "{text}");
    assert_eq!(rev(root, "main"), main);
    assert_eq!(commit_count(root), commits, "a held merge writes no commit");
    assert_eq!(std::fs::read_to_string(&record).unwrap(), "{not json");
}
