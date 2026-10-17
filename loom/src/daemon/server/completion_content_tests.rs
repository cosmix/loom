use super::{scratch_git, trusted_checkpoint_fixture};
use crate::handoff::completion::content::VerificationSnapshot;
use crate::models::{
    session::SessionType,
    stage::{StageStatus, StageType},
};
use crate::verify::transitions::{load_stage, update_stage};
const EVIDENCE: &str = "11111111111111111111111111111111";
const COMPLETION: &str = "22222222222222222222222222222222";

#[test]
fn completion_refuses_unstaged_fix_and_untracked_required_input() {
    for tracked in [true, false] {
        let fixture =
            trusted_checkpoint_fixture("api", StageType::Standard, SessionType::Stage, EVIDENCE);
        let checkout = fixture.repo().join(".worktrees/api");
        let fresh_evidence = "33333333333333333333333333333333";
        if tracked {
            commit_input(&checkout);
            // Bind a fresh checkpoint to the committed broken input.
            super::record_trusted_test_checkpoint(
                fixture.repo(),
                &fixture.work,
                &fixture.stage,
                &fixture.session,
                fresh_evidence,
            );
        }
        std::fs::write(checkout.join("input"), "fixed").unwrap();
        let error = fixture
            .complete(COMPLETION, if tracked { fresh_evidence } else { EVIDENCE })
            .unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains(if tracked {
                "worktree differs"
            } else {
                "untracked completion input"
            }),
            "{message}"
        );
        assert_eq!(
            load_stage("api", &fixture.work).unwrap().status,
            StageStatus::Executing
        );
    }
}

fn commit_input(checkout: &std::path::Path) {
    std::fs::write(checkout.join("input"), "broken").unwrap();
    scratch_git(checkout, &["add", "input"]);
    scratch_git(
        checkout,
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "broken",
        ],
    );
}

#[test]
fn content_mutation_after_verification_is_refused() {
    let fixture =
        trusted_checkpoint_fixture("api", StageType::Standard, SessionType::Stage, EVIDENCE);
    let snapshot = VerificationSnapshot::capture(&fixture.stage, &fixture.work).unwrap();
    let checkout = fixture.repo().join(".worktrees/api");
    std::fs::write(checkout.join("new-input"), "changed").unwrap();
    assert!(snapshot.validate(&fixture.stage, &fixture.work).is_err());
    assert!(fixture.complete(COMPLETION, EVIDENCE).is_err());
}

#[test]
fn completion_refuses_untracked_input_inside_scaffold_directory() {
    let fixture =
        trusted_checkpoint_fixture("api", StageType::Standard, SessionType::Stage, EVIDENCE);
    let checkout = fixture.repo().join(".worktrees/api");
    std::fs::create_dir_all(checkout.join(".claude")).unwrap();
    std::fs::write(checkout.join(".claude/required-input"), "uncommitted").unwrap();
    let error = fixture.complete(COMPLETION, EVIDENCE).unwrap_err();
    assert!(format!("{error:#}").contains("untracked completion input"));
    assert_eq!(
        load_stage("api", &fixture.work).unwrap().status,
        StageStatus::Executing
    );
}

#[test]
fn contracts_only_amendment_invalidates_checkpoint_with_unchanged_head() {
    let fixture =
        trusted_checkpoint_fixture("api", StageType::Standard, SessionType::Stage, EVIDENCE);
    let before = scratch_git(fixture.repo(), &["rev-parse", "loom/api"]);
    update_stage("api", &fixture.work, |stage| {
        stage
            .contracts
            .push(serde_json::from_value(serde_json::json!({
                "id": "contract", "file": "tests/api.rs", "test": "rejects_invalid",
                "scenario": "invalid input", "rejects": "accepting invalid input"
            }))?);
        Ok(())
    })
    .unwrap();
    let error = fixture.complete(COMPLETION, EVIDENCE).unwrap_err();
    assert!(format!("{error:#}").contains("check-definition"));
    assert_eq!(
        before,
        scratch_git(fixture.repo(), &["rev-parse", "loom/api"])
    );
}

#[test]
fn completion_refuses_ignored_required_input() {
    for path in [
        "required.env",
        "apps/api/src/input.py",
        "packages/ui/package.json",
        "deps/tool.py",
        "node_modules_backup/input.js",
    ] {
        ignored_input_is_refused(path);
    }
}

fn ignored_input_is_refused(path: &str) {
    let fixture =
        trusted_checkpoint_fixture("api", StageType::Standard, SessionType::Stage, EVIDENCE);
    let checkout = fixture.repo().join(".worktrees/api");
    std::fs::write(fixture.repo().join(".git/info/exclude"), "*\n").unwrap();
    std::fs::create_dir_all(checkout.join(path).parent().unwrap()).unwrap();
    std::fs::write(checkout.join(path), "uncommitted fix").unwrap();
    let error = fixture.complete(COMPLETION, EVIDENCE).unwrap_err();
    assert!(format!("{error:#}").contains("untracked completion input"));
    assert_eq!(
        load_stage("api", &fixture.work).unwrap().status,
        StageStatus::Executing
    );
}

#[test]
fn completion_accepts_generated_outputs_with_unchanged_proof() {
    let fixture =
        trusted_checkpoint_fixture("api", StageType::Standard, SessionType::Stage, EVIDENCE);
    let checkout = fixture.repo().join(".worktrees/api");
    let snapshot = VerificationSnapshot::capture(&fixture.stage, &fixture.work).unwrap();
    for path in [
        "target/debug/generated",
        "node_modules/.bun/pkg@1/node_modules/pkg/index.js",
        "apps/web/node_modules/pkg/index.js",
        "packages/api/.venv/pyvenv.cfg",
        ".ruff_cache/.gitignore",
        ".pytest_cache/v/cache/nodeids",
        "deps/tool/__pycache__/tool.cpython-314.pyc",
    ] {
        let output = checkout.join(path);
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::write(output, "output").unwrap();
    }
    for name in crate::verify::tool_artifacts::NAMES {
        std::fs::write(checkout.join(name), "").unwrap();
    }
    snapshot.validate(&fixture.stage, &fixture.work).unwrap();
    fixture.complete(COMPLETION, EVIDENCE).unwrap();
    let stage = load_stage("api", &fixture.work).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
    assert_eq!(
        stage.completed_commit.unwrap(),
        scratch_git(&checkout, &["rev-parse", "HEAD"])
    );
}

#[test]
fn completion_refuses_declared_generated_input_with_current_proof() {
    let fixture =
        trusted_checkpoint_fixture("api", StageType::Standard, SessionType::Stage, EVIDENCE);
    let checkout = fixture.repo().join(".worktrees/api");
    let declared = update_stage("api", &fixture.work, |stage| {
        stage.working_dir = Some("apps/api".into());
        stage.files.push(".venv/**".into());
        Ok(())
    })
    .unwrap();
    let fresh_evidence = "33333333333333333333333333333333";
    super::record_trusted_test_checkpoint(
        fixture.repo(),
        &fixture.work,
        &declared,
        &fixture.session,
        fresh_evidence,
    );
    std::fs::create_dir_all(checkout.join("apps/api/.venv")).unwrap();
    std::fs::write(checkout.join("apps/api/.venv/required.py"), "input").unwrap();
    let error = fixture.complete(COMPLETION, fresh_evidence).unwrap_err();
    assert!(
        format!("{error:#}").contains("untracked completion input"),
        "{error:#}"
    );
    assert_eq!(
        load_stage("api", &fixture.work).unwrap().status,
        StageStatus::Executing
    );
}

#[test]
fn policy_change_and_reversion_cannot_resurrect_checkpoint() {
    let fixture =
        trusted_checkpoint_fixture("api", StageType::Standard, SessionType::Stage, EVIDENCE);
    let snapshot = VerificationSnapshot::capture(&fixture.stage, &fixture.work).unwrap();
    let changed = update_stage("api", &fixture.work, |stage| {
        stage.acceptance.clear();
        Ok(())
    })
    .unwrap();
    assert!(snapshot.validate(&changed, &fixture.work).is_err());
    update_stage("api", &fixture.work, |stage| {
        stage.acceptance = fixture.stage.acceptance.clone();
        Ok(())
    })
    .unwrap();
    std::fs::create_dir_all(fixture.work.join("plan_versions")).unwrap();
    std::fs::write(
        fixture.work.join("plan_versions/audit.md"),
        "| 1 | api | acceptance | replace | 0 | 2026-10-16T00:00:00Z | | first |\n\
         | 2 | api | acceptance | replace | 0 | 2026-10-16T00:01:00Z | | revert |\n",
    )
    .unwrap();
    assert!(snapshot.validate(&fixture.stage, &fixture.work).is_err());
    let error = fixture.complete(COMPLETION, EVIDENCE).unwrap_err();
    assert!(format!("{error:#}").contains("check-definition"));
}

#[test]
fn verification_snapshot_validates_inside_stage_update_lock() {
    let fixture =
        trusted_checkpoint_fixture("api", StageType::Standard, SessionType::Stage, EVIDENCE);
    let snapshot = VerificationSnapshot::capture(&fixture.stage, &fixture.work).unwrap();
    update_stage("api", &fixture.work, |stage| {
        snapshot.validate_locked(stage, &fixture.work)
    })
    .unwrap();
}

#[test]
fn committed_revision_change_after_snapshot_is_refused() {
    let fixture =
        trusted_checkpoint_fixture("api", StageType::Standard, SessionType::Stage, EVIDENCE);
    let snapshot = VerificationSnapshot::capture(&fixture.stage, &fixture.work).unwrap();
    commit_input(&fixture.repo().join(".worktrees/api"));
    let error = snapshot
        .validate(&fixture.stage, &fixture.work)
        .unwrap_err();
    assert!(error.to_string().contains("content changed"));
    assert!(fixture.complete(COMPLETION, EVIDENCE).is_err());
    assert_eq!(
        load_stage("api", &fixture.work).unwrap().status,
        StageStatus::Executing
    );
}

#[test]
fn effective_gate_files_invalidate_snapshot_identity() {
    for relative in ["config.toml", "contracts/api/freeze.json"] {
        let fixture =
            trusted_checkpoint_fixture("api", StageType::Standard, SessionType::Stage, EVIDENCE);
        let snapshot = VerificationSnapshot::capture(&fixture.stage, &fixture.work).unwrap();
        let path = fixture.work.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
        let error = snapshot
            .validate(&fixture.stage, &fixture.work)
            .unwrap_err();
        assert!(error.to_string().contains("policy changed"), "{error:#}");
        assert!(fixture.complete(COMPLETION, EVIDENCE).is_err());
    }
}
