//! A relayed `commit` that signs, or fails to: the signed commit, the stage
//! block a stage session's signing failure raises, and the settle of a block
//! the stage refuses.

use crate::daemon::Response;
use crate::fs::inbox::LedgerOutcome;
use crate::git::signing::tests::{fake_signer, git_in};
use crate::models::stage::StageStatus;
use crate::verify::transitions::load_stage;

use super::super::commit::settle_signing_block;
use super::super::test_support::{fixture, STAGE};
use super::super::Settle;
use super::{
    add_file, reason_of, relay_commit, repository, stage_session, staged_payload, tip, BRANCH,
};

#[test]
fn a_signed_commit_lands_when_gpgsign_is_on() {
    let fx = fixture();
    let worktree = repository(&fx);
    let record = stage_session(&fx);
    fake_signer(&fx.repo_root, false);
    add_file(&worktree, "a.txt");

    let row = relay_commit(&fx, &mut fx.host(true), &record, staged_payload(&worktree));

    reason_of(&row, LedgerOutcome::Applied);
    let object = git_in(&worktree, &["cat-file", "commit", BRANCH]);
    assert!(
        object.lines().any(|line| line.starts_with("gpgsig ")),
        "{object}"
    );
}

#[test]
fn a_signing_failure_blocks_the_stage_and_leaves_the_ref() {
    let fx = fixture();
    let worktree = repository(&fx);
    let record = stage_session(&fx);
    let before = tip(&worktree, BRANCH);
    fake_signer(&fx.repo_root, true);
    add_file(&worktree, "a.txt");

    let row = relay_commit(&fx, &mut fx.host(true), &record, staged_payload(&worktree));

    let reason = reason_of(&row, LedgerOutcome::Refused);
    assert!(reason.contains("fake signer refused"), "{reason}");
    assert!(
        reason.contains("the stage is blocked for the operator"),
        "{reason}"
    );
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Blocked);
    let block_reason = stage.close_reason.unwrap_or_default();
    assert!(
        block_reason.contains("fake signer refused"),
        "{block_reason}"
    );
    assert!(
        block_reason.contains("loom stage retry s1"),
        "{block_reason}"
    );
    assert_eq!(tip(&worktree, BRANCH), before);
}

#[test]
fn a_refused_block_is_not_reported_as_a_block() {
    let refused = settle_signing_block(STAGE, "fake signer refused", "remedy", |_| {
        Ok(Response::Error {
            message: "cannot block stage 's1': already blocked".to_string(),
        })
    });
    let failed = settle_signing_block(STAGE, "fake signer refused", "remedy", |_| {
        Err(anyhow::anyhow!("failed to persist blocked state"))
    });

    for (settle, why) in [(refused, "already blocked"), (failed, "failed to persist")] {
        let Settle::Refused(reason) = settle else {
            panic!("a signing failure is never applied");
        };
        assert!(reason.contains("could not be blocked"), "{reason}");
        assert!(reason.contains(why), "{reason}");
        assert!(!reason.contains("is blocked for the operator"), "{reason}");
    }
}
