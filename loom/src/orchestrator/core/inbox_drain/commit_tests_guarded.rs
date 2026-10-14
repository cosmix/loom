//! A relayed merge commit and the plan's write denies: the resolver's sandbox
//! is released from them, so the daemon refuses a merge commit that authors
//! content under one, or that it cannot check because the plan snapshot is
//! unreadable.

use crate::fs::inbox::LedgerOutcome;
use crate::fs::work_dir::write_plan_sandbox;
use crate::models::stage::StageStatus;
use crate::plan::schema::{FilesystemConfig, SandboxConfig};

use super::super::test_support::{fixture, Fixture};
use super::{
    add_file, merge_session, merging_repository, reason_of, relay_commit, stage_in, staged_payload,
    tip, BRANCH,
};

/// A plan that denies writes under `protected/`.
fn plan_protecting_a_directory(fx: &Fixture) {
    let sandbox = SandboxConfig {
        filesystem: FilesystemConfig {
            deny_write: vec!["protected/**".to_string()],
            ..FilesystemConfig::default()
        },
        ..SandboxConfig::default()
    };
    write_plan_sandbox(&fx.work_dir, &sandbox).unwrap();
}

#[test]
fn a_merge_commit_authoring_a_protected_path_is_refused_with_the_ref_unmoved() {
    let fx = fixture();
    plan_protecting_a_directory(&fx);
    let worktree = merging_repository(&fx);
    let record = merge_session(&fx);
    stage_in(&fx, StageStatus::MergeConflict, None);
    add_file(&worktree, "protected/new.txt");
    let before = tip(&worktree, BRANCH);

    let row = relay_commit(&fx, &mut fx.host(true), &record, staged_payload(&worktree));

    let reason = reason_of(&row, LedgerOutcome::Refused);
    assert!(reason.contains("'protected/new.txt'"), "{reason}");
    assert!(
        reason.contains("git --literal-pathspecs restore --source="),
        "{reason}"
    );
    assert_eq!(tip(&worktree, BRANCH), before);
}

#[test]
fn a_merge_commit_is_refused_when_the_plan_snapshot_cannot_be_read() {
    let fx = fixture();
    // Valid TOML whose `deny_write` is not a list: the section cannot be read.
    std::fs::write(
        fx.work_dir.join("config.toml"),
        "[plan_sandbox.filesystem]\ndeny_write = \"protected/**\"\n",
    )
    .unwrap();
    let worktree = merging_repository(&fx);
    let record = merge_session(&fx);
    stage_in(&fx, StageStatus::MergeConflict, None);
    add_file(&worktree, "elsewhere/new.txt");
    let before = tip(&worktree, BRANCH);

    let row = relay_commit(&fx, &mut fx.host(true), &record, staged_payload(&worktree));

    let reason = reason_of(&row, LedgerOutcome::Refused);
    assert!(
        reason.contains("cannot be checked against the plan's write denies"),
        "{reason}"
    );
    assert!(
        reason.contains("the plan's sandbox config could not be read"),
        "{reason}"
    );
    assert_eq!(tip(&worktree, BRANCH), before);
}

#[test]
fn a_merge_commit_authoring_an_unprotected_path_is_applied() {
    let fx = fixture();
    plan_protecting_a_directory(&fx);
    let worktree = merging_repository(&fx);
    let record = merge_session(&fx);
    stage_in(&fx, StageStatus::MergeConflict, None);
    add_file(&worktree, "elsewhere/new.txt");
    let before = tip(&worktree, BRANCH);

    let row = relay_commit(&fx, &mut fx.host(true), &record, staged_payload(&worktree));

    reason_of(&row, LedgerOutcome::Applied);
    assert_ne!(tip(&worktree, BRANCH), before);
}
