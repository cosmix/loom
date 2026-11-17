//! An accepted contract dispute that carries a replacement file
//! (`apply_contract_replacement.rs`): what lands in the worktree and the
//! freeze, and what a conflict, a symlink, a replay or a non-accept verdict
//! leaves alone.

use serde_json::json;
use std::os::unix::fs::PermissionsExt;

use super::FAIL_AFTER_WRITE;
use crate::models::dispute::{DisputeKind, DisputeVerdict, PlanPatch};
use crate::models::stage::StageStatus;
use crate::orchestrator::adjudication::apply_contract_tests::{
    fixture, frozen_hash, replacing_contract, Fixture,
};
use crate::orchestrator::adjudication::tests::{reject_verdict, write_request, write_verdict};
use crate::orchestrator::adjudication::{feedback, AdjudicatorRegistry};
use crate::utils::sha256_hex;
use crate::verify::contracts::store as contract_store;
use crate::verify::contracts::test_support::{CONTRACT_CONTENT, CONTRACT_FILE, CONTRACT_ID};
use crate::verify::transitions::{load_stage, update_stage};

/// CRLF, trailing whitespace, a bare `---` line and no final newline: bytes
/// a text round trip would normalise.
const REPLACEMENT: &str = "#[test]\r\nfn rejects_x() { assert!(true); }   \r\n---\n\ttail";

fn base_hash() -> String {
    sha256_hex(CONTRACT_CONTENT)
}

fn replacement_hash() -> String {
    sha256_hex(REPLACEMENT.as_bytes())
}

/// File dispute 1 over [`CONTRACT_ID`] carrying [`REPLACEMENT`], filed against
/// `base`.
fn file_replacement(fx: &Fixture, base: Option<String>) {
    let kind = DisputeKind::Contract {
        contract_id: CONTRACT_ID.to_string(),
        replacement: Some(REPLACEMENT.to_string()),
        replacement_base_sha256: base,
    };
    write_request(&fx.work, "s1", 1, kind);
}

fn record_verdict(fx: &Fixture, verdict: DisputeVerdict) {
    write_verdict(&fx.work, "s1", 1, verdict, 1);
}

fn accept_verdict(plan_patch: serde_json::Value) -> DisputeVerdict {
    let DisputeVerdict::Reject {
        citations,
        reasoning,
    } = reject_verdict()
    else {
        unreachable!("reject_verdict builds a reject");
    };
    DisputeVerdict::Accept {
        plan_patch: PlanPatch { inner: plan_patch },
        citations,
        reasoning,
    }
}

/// Apply dispute 1 once; the verdict is recorded first.
fn apply(fx: &Fixture, verdict: DisputeVerdict) -> anyhow::Result<()> {
    record_verdict(fx, verdict);
    AdjudicatorRegistry::new().apply_verdict(&fx.work, "s1", 1)
}

fn worktree_bytes(fx: &Fixture, rel: &str) -> Vec<u8> {
    std::fs::read(fx.worktree.join(rel)).unwrap()
}

#[test]
fn accept_writes_the_replacement_byte_exact_and_refreezes_it() {
    let fx = fixture(Some("."));
    file_replacement(&fx, Some(base_hash()));

    apply(&fx, accept_verdict(json!({}))).unwrap();

    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), REPLACEMENT.as_bytes());
    assert_eq!(
        frozen_hash(&fx, CONTRACT_FILE),
        Some(replacement_hash()),
        "the freeze holds the bytes that were written"
    );
    let notice = feedback::read_feedback(&fx.work, "s1").unwrap().unwrap();
    assert!(
        notice.contains(&format!(
            "The daemon wrote the accepted replacement to `{CONTRACT_FILE}`. Stage it with \
             `git add {CONTRACT_FILE}` and commit it with your other files through \
             `loom stage commit`."
        )),
        "{notice}"
    );
}

#[test]
fn a_patch_that_re_points_the_file_gets_the_replacement_at_the_new_path() {
    let fx = fixture(Some("."));
    file_replacement(&fx, Some(base_hash()));
    let spec = "id: rejects-x\nfile: tests/y_contract.rs\ntest: tests::rejects_x\n\
                runner: cargo-test\nscenario: x arrives\nrejects: accepting x";

    apply(&fx, accept_verdict(replacing_contract(spec))).unwrap();

    assert_eq!(
        worktree_bytes(&fx, "tests/y_contract.rs"),
        REPLACEMENT.as_bytes()
    );
    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), CONTRACT_CONTENT);
    assert_eq!(
        frozen_hash(&fx, "tests/y_contract.rs"),
        Some(replacement_hash())
    );
}

/// A single-contract stage cannot delete its contract (the amendment would
/// leave a v2 standard stage with none), so the contract goes by a rename: the
/// disputed id no longer names a file.
#[test]
fn a_patch_that_removes_the_disputed_contract_writes_nothing() {
    let fx = fixture(Some("."));
    file_replacement(&fx, Some(base_hash()));
    let spec = "id: renamed-x\nfile: tests/y_contract.rs\ntest: tests::rejects_x\n\
                runner: cargo-test\nscenario: x arrives\nrejects: accepting x";

    let error = apply(&fx, accept_verdict(replacing_contract(spec))).unwrap_err();

    assert!(
        format!("{error:#}").contains("removed contract 'rejects-x'"),
        "{error:#}"
    );
    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), CONTRACT_CONTENT);
    assert!(!fx.worktree.join("tests/y_contract.rs").exists());
}

#[test]
fn a_symlinked_target_is_refused_and_its_destination_left_alone() {
    let fx = fixture(Some("."));
    file_replacement(&fx, Some(base_hash()));
    let outside = fx.work.join("outside.rs");
    std::fs::write(&outside, "outside\n").unwrap();
    let target = fx.worktree.join(CONTRACT_FILE);
    std::fs::remove_file(&target).unwrap();
    std::os::unix::fs::symlink(&outside, &target).unwrap();

    let error = apply(&fx, accept_verdict(json!({}))).unwrap_err();

    assert!(format!("{error:#}").contains("refusing to"), "{error:#}");
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "outside\n");
    assert!(std::fs::symlink_metadata(&target).unwrap().is_symlink());
}

#[test]
fn a_symlinked_parent_directory_is_refused_and_its_destination_left_alone() {
    let fx = fixture(Some("."));
    file_replacement(&fx, Some(base_hash()));
    let elsewhere = fx.work.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("x_contract.rs"), "outside\n").unwrap();
    let tests_dir = fx.worktree.join("tests");
    std::fs::remove_dir_all(&tests_dir).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &tests_dir).unwrap();

    assert!(apply(&fx, accept_verdict(json!({}))).is_err());

    assert_eq!(
        std::fs::read_to_string(elsewhere.join("x_contract.rs")).unwrap(),
        "outside\n"
    );
}

#[test]
fn a_working_dir_below_the_worktree_root_gets_the_file_beneath_it() {
    let fx = fixture(Some("."));
    update_stage("s1", &fx.work, |stage| {
        stage.working_dir = Some("loom".to_string());
        Ok(())
    })
    .unwrap();
    let nested = fx.worktree.join("loom");
    std::fs::create_dir_all(nested.join("tests")).unwrap();
    std::fs::write(nested.join(CONTRACT_FILE), CONTRACT_CONTENT).unwrap();
    std::fs::write(
        nested.join("Cargo.toml"),
        std::fs::read(fx.worktree.join("Cargo.toml")).unwrap(),
    )
    .unwrap();
    file_replacement(&fx, Some(base_hash()));

    apply(&fx, accept_verdict(json!({}))).unwrap();

    assert_eq!(
        worktree_bytes(&fx, &format!("loom/{CONTRACT_FILE}")),
        REPLACEMENT.as_bytes()
    );
    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), CONTRACT_CONTENT);
}

#[test]
fn a_failure_between_the_write_and_the_refreeze_is_repaired_by_a_replay() {
    let fx = fixture(Some("."));
    file_replacement(&fx, Some(base_hash()));
    FAIL_AFTER_WRITE.with(|fail| fail.set(true));

    assert!(apply(&fx, accept_verdict(json!({}))).is_err());
    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), REPLACEMENT.as_bytes());
    assert_eq!(
        frozen_hash(&fx, CONTRACT_FILE),
        Some(base_hash()),
        "the freeze still names the old file"
    );

    AdjudicatorRegistry::new()
        .apply_verdict(&fx.work, "s1", 1)
        .unwrap();

    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), REPLACEMENT.as_bytes());
    assert_eq!(frozen_hash(&fx, CONTRACT_FILE), Some(replacement_hash()));
}

#[test]
fn a_replay_after_a_completed_refreeze_changes_nothing() {
    let fx = fixture(Some("."));
    let mut stage = load_stage("s1", &fx.work).unwrap();
    let kind = DisputeKind::Contract {
        contract_id: CONTRACT_ID.to_string(),
        replacement: Some(REPLACEMENT.to_string()),
        replacement_base_sha256: Some(base_hash()),
    };
    let patch = PlanPatch { inner: json!({}) };
    for _ in 0..2 {
        super::super::accept(&fx.work, &mut stage, 1, &kind, &patch)
            .unwrap()
            .expect("an accept notice");
        assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), REPLACEMENT.as_bytes());
        assert_eq!(frozen_hash(&fx, CONTRACT_FILE), Some(replacement_hash()));
    }
}

#[test]
fn a_frozen_file_that_changed_after_filing_writes_nothing_and_escalates() {
    let fx = fixture(Some("."));
    file_replacement(&fx, Some(base_hash()));
    let moved = vec![(CONTRACT_FILE.to_string(), b"moved on\n".to_vec())];
    contract_store::refreeze_set(&fx.work, "s1", &moved, &[]).unwrap();

    apply(&fx, accept_verdict(json!({}))).unwrap();

    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), CONTRACT_CONTENT);
    let stage = load_stage("s1", &fx.work).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    let reason = stage.review_reason.unwrap_or_default();
    assert!(
        reason.contains(
            "the frozen `tests/x_contract.rs` changed after dispute 1 was filed; its \
             replacement was not written"
        ),
        "{reason}"
    );
}

#[test]
fn the_executable_bit_of_the_target_is_kept() {
    let fx = fixture(Some("."));
    file_replacement(&fx, Some(base_hash()));
    let target = fx.worktree.join(CONTRACT_FILE);
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();

    apply(&fx, accept_verdict(json!({}))).unwrap();

    let mode = std::fs::metadata(&target).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), REPLACEMENT.as_bytes());
}

#[test]
fn a_reject_leaves_the_worktree_alone_and_says_the_replacement_was_discarded() {
    let fx = fixture(Some("."));
    file_replacement(&fx, Some(base_hash()));

    apply(&fx, reject_verdict()).unwrap();

    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), CONTRACT_CONTENT);
    assert_eq!(frozen_hash(&fx, CONTRACT_FILE), Some(base_hash()));
    let notice = feedback::read_feedback(&fx.work, "s1").unwrap().unwrap();
    assert!(
        notice.contains("Your replacement was discarded; the worktree was not changed."),
        "{notice}"
    );
    assert!(!notice.contains("contracts restore"), "{notice}");
}

#[test]
fn needs_more_evidence_counts_a_round_and_never_asks_for_an_edit() {
    let fx = fixture(Some("."));
    file_replacement(&fx, Some(base_hash()));
    let ask = DisputeVerdict::NeedsMoreEvidence {
        questions: vec!["Which scenario does the new assertion cover?".to_string()],
    };

    apply(&fx, ask).unwrap();

    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), CONTRACT_CONTENT);
    assert_eq!(load_stage("s1", &fx.work).unwrap().tally.evidence_rounds, 1);
    let notice = feedback::read_feedback(&fx.work, "s1").unwrap().unwrap();
    assert!(notice.contains("Never edit a frozen file."), "{notice}");
    assert!(
        notice.contains(
            "loom stage dispute-contract s1 --contract rejects-x --replacement <path> \
             --reason \"...\""
        ),
        "{notice}"
    );
    assert!(
        !notice.contains("commit it, and file a follow-up"),
        "{notice}"
    );
}
