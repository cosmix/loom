//! Tests for the contract dispute briefing.

use super::*;
use crate::models::dispute::DisputeRequest;
use crate::models::stage::Stage;
use crate::orchestrator::adjudication::prompt::ExecutionSite;
use crate::verify::contracts::test_support::contract;
use chrono::Utc;
use std::path::PathBuf;

fn request() -> DisputeRequest {
    DisputeRequest {
        id: 4,
        stage_id: "demo".to_string(),
        kind: DisputeKind::Contract {
            contract_id: "rejects-x".to_string(),
        },
        reason: "the new assertion still rejects x".to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 0,
        created_at: Utc::now(),
    }
}

/// Fixture for [`full_briefing_quotes_each_source_and_the_diff`]: a
/// stage with one contract, its frozen file and current worktree file
/// written to disk, ready for [`KindPromptInput`] to borrow from.
struct ContractFixture {
    _tmp: tempfile::TempDir,
    work_dir: PathBuf,
    worktree: PathBuf,
    stage: Stage,
    request: DisputeRequest,
    site: ExecutionSite,
}

fn contract_fixture() -> ContractFixture {
    let tmp = tempfile::tempdir().unwrap();
    let work_dir = tmp.path().join(".loom/work");
    let worktree = tmp.path().join(".worktrees/demo");
    let execution = worktree.join("loom");
    let stage = Stage {
        id: "demo".to_string(),
        working_dir: Some("loom".to_string()),
        contracts: vec![contract()],
        ..Stage::default()
    };
    let frozen = store::frozen_file_path(&work_dir, &stage.id, &stage.contracts[0].file);
    let current = execution.join(&stage.contracts[0].file);
    fs::create_dir_all(frozen.parent().unwrap()).unwrap();
    fs::create_dir_all(current.parent().unwrap()).unwrap();
    fs::write(&frozen, "assert!(false);\n").unwrap();
    fs::write(&current, "assert!(rejects_x());\n").unwrap();
    let site = ExecutionSite {
        path: execution,
        working_dir: "loom".to_string(),
        worktree_present: true,
        root: worktree.clone(),
    };
    ContractFixture {
        _tmp: tmp,
        work_dir,
        worktree,
        stage,
        request: request(),
        site,
    }
}

#[test]
fn full_briefing_quotes_each_source_and_the_diff() {
    let fx = contract_fixture();
    let input = KindPromptInput {
        stage: &fx.stage,
        dispute_id: 4,
        request: &fx.request,
        site: &fx.site,
        worktree: Some(&fx.worktree),
        work_dir: &fx.work_dir,
    };

    let prompt = build(&input);

    for expected in [
        "## Contract spec",
        "id: rejects-x",
        "file: tests/x_contract.rs",
        "test: tests::rejects_x",
        "cwd: (not set; detected)",
        "recorded cwd: (none recorded)",
        "scenario: x arrives",
        "rejects: accepting x",
        "## Frozen content",
        "assert!(false);",
        "## Current content",
        "assert!(rejects_x());",
        "## Agent's reason",
        "the new assertion still rejects x",
        "## Diff (frozen vs current)",
        "-assert!(false);",
        "+assert!(rejects_x());",
    ] {
        assert!(prompt.evidence.contains(expected), "missing {expected}");
    }
    assert!(prompt
        .instructions
        .contains("loom stage adjudicate --stage demo"));
    assert!(prompt
        .instructions
        .contains("run directory (`loom` in the stage's worktree)"));
}

#[test]
fn missing_worktree_leaves_a_readable_briefing() {
    let tmp = tempfile::tempdir().unwrap();
    let work_dir = tmp.path().join(".loom/work");
    let stage = Stage {
        id: "demo".to_string(),
        contracts: vec![contract()],
        ..Stage::default()
    };
    let site = ExecutionSite {
        path: tmp.path().to_path_buf(),
        working_dir: ".".to_string(),
        worktree_present: false,
        root: tmp.path().to_path_buf(),
    };
    let request = request();
    let input = KindPromptInput {
        stage: &stage,
        dispute_id: 4,
        request: &request,
        site: &site,
        worktree: None,
        work_dir: &work_dir,
    };

    let prompt = build(&input);

    assert!(prompt
        .evidence
        .contains("(current file unavailable: stage worktree missing)"));
    assert!(prompt.evidence.contains("(diff unavailable"));
    assert!(prompt.instructions.contains("stage worktree unavailable"));
}

#[test]
fn the_spec_block_shows_the_plan_and_recorded_cwd() {
    let mut fx = contract_fixture();
    fx.stage.contracts[0].cwd = Some("loom".to_string());
    let record = store::FreezeRecord {
        version: store::FREEZE_RECORD_VERSION,
        stage_id: "demo".to_string(),
        session_id: "contract-session".to_string(),
        frozen_at: Utc::now(),
        base: "main".to_string(),
        files: Vec::new(),
        contracts: vec![store::FrozenContract {
            id: "rejects-x".to_string(),
            adapter: Some("cargo-test".to_string()),
            outcome: "failed".to_string(),
            exit_code: Some(101),
            file: Some("tests/x_contract.rs".to_string()),
            cwd: Some(".".to_string()),
        }],
    };
    store::write_freeze(&fx.work_dir, &record, &[]).unwrap();
    let input = KindPromptInput {
        stage: &fx.stage,
        dispute_id: 4,
        request: &fx.request,
        site: &fx.site,
        worktree: Some(&fx.worktree),
        work_dir: &fx.work_dir,
    };

    let prompt = build(&input);

    assert!(
        prompt.evidence.contains("\ncwd: loom\n"),
        "{}",
        prompt.evidence
    );
    assert!(
        prompt.evidence.contains("\nrecorded cwd: .\n"),
        "{}",
        prompt.evidence
    );
    assert!(prompt
        .instructions
        .contains("run directory (`loom` in the stage's worktree)"));
    assert!(prompt
        .instructions
        .contains("`cwd` is the optional directory"));
}
