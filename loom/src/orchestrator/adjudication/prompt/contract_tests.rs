//! Tests for the contract dispute briefing.

use super::*;
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::DisputeRequest;
use crate::models::stage::Stage;
use crate::orchestrator::adjudication::prompt::truncate::truncate_to_budget;
use crate::orchestrator::adjudication::prompt::{ExecutionSite, MAX_PROMPT_BYTES};
use crate::relay::sha256_hex;
use crate::verify::contracts::test_support::{contract, contract_worktree};
use chrono::Utc;
use std::path::{Path, PathBuf};

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
pub(super) struct ContractFixture {
    _tmp: tempfile::TempDir,
    work_dir: PathBuf,
    pub(super) worktree: PathBuf,
    pub(super) stage: Stage,
    request: DisputeRequest,
    site: ExecutionSite,
}

pub(super) fn contract_fixture() -> ContractFixture {
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

/// Writes the freeze of the `demo` stage holding exactly `frozen`.
fn freeze(work_dir: &Path, frozen: &[(&str, &str)]) {
    let record = store::FreezeRecord {
        version: store::FREEZE_RECORD_VERSION,
        stage_id: "demo".to_string(),
        session_id: "contract-session".to_string(),
        frozen_at: Utc::now(),
        base: "main".to_string(),
        files: frozen
            .iter()
            .map(|(path, content)| store::FrozenFile {
                path: path.to_string(),
                sha256: sha256_hex(content.as_bytes()),
            })
            .collect(),
        contracts: Vec::new(),
    };
    let copies: Vec<(String, Vec<u8>)> = frozen
        .iter()
        .map(|(path, content)| (path.to_string(), content.as_bytes().to_vec()))
        .collect();
    store::write_freeze(work_dir, &record, &copies).unwrap();
}

/// Freezes four files, then changes one in the worktree, leaves one unchanged
/// and deletes one.
pub(super) fn freeze_then_change_support_files(fx: &ContractFixture) {
    freeze(
        &fx.work_dir,
        &[
            ("tests/x_contract.rs", "assert!(false);\n"),
            (
                "tests/support/changed.rs",
                "pub fn ready() -> bool { false }\n",
            ),
            ("tests/support/same.rs", "pub fn same() {}\n"),
            ("tests/support/gone.rs", "pub fn gone() {}\n"),
        ],
    );
    let support = fx.site.path.join("tests/support");
    fs::create_dir_all(&support).unwrap();
    fs::write(
        support.join("changed.rs"),
        "pub fn ready() -> bool { true }\n",
    )
    .unwrap();
    fs::write(support.join("same.rs"), "pub fn same() {}\n").unwrap();
}

#[test]
fn the_briefing_shows_each_changed_and_deleted_frozen_file() {
    let fx = contract_fixture();
    freeze_then_change_support_files(&fx);
    let input = KindPromptInput {
        stage: &fx.stage,
        dispute_id: 4,
        request: &fx.request,
        site: &fx.site,
        worktree: Some(&fx.worktree),
        work_dir: &fx.work_dir,
    };

    let prompt = build(&input);

    let start = prompt
        .evidence
        .find(FROZEN_SET_HEADING)
        .expect("the section");
    let section = &prompt.evidence[start..];
    for expected in [
        "- `tests/support/changed.rs` changed:",
        "-pub fn ready() -> bool { false }",
        "+pub fn ready() -> bool { true }",
        "- `tests/support/gone.rs` is deleted from the worktree",
    ] {
        assert!(section.contains(expected), "missing {expected}:\n{section}");
    }
    assert!(!section.contains("same.rs"), "{section}");
    assert!(!section.contains("x_contract.rs"), "{section}");
    assert!(prompt.instructions.contains(FROZEN_SET_HEADING));
}

fn input_of(fx: &ContractFixture) -> KindPromptInput<'_> {
    KindPromptInput {
        stage: &fx.stage,
        dispute_id: 4,
        request: &fx.request,
        site: &fx.site,
        worktree: Some(&fx.worktree),
        work_dir: &fx.work_dir,
    }
}

/// The frozen-set section of a briefing built for `fx`.
pub(super) fn frozen_set_of(fx: &ContractFixture) -> String {
    let prompt = build(&input_of(fx));
    let start = prompt
        .evidence
        .find(FROZEN_SET_HEADING)
        .expect("the section");
    prompt.evidence[start..].to_string()
}

/// A contract file the cap lets the briefing quote three times must not push
/// the frozen-set section off the end of the prompt.
#[test]
fn a_large_contract_file_leaves_the_frozen_set_section_after_truncation() {
    let fx = contract_fixture();
    freeze_then_change_support_files(&fx);
    let frozen: String = (0..2500)
        .map(|n| format!("assert!(case_{n}());\n"))
        .collect();
    let current = frozen.replace("case_1250()", "other()");
    let file = &fx.stage.contracts[0].file;
    let frozen_copy = store::frozen_file_path(&fx.work_dir, "demo", file);
    fs::write(frozen_copy, &frozen).unwrap();
    fs::write(fx.site.path.join(file), &current).unwrap();
    let mut prompt = build(&input_of(&fx));
    assert!(
        prompt.total_len() > MAX_PROMPT_BYTES,
        "the fixture must overflow"
    );

    truncate_to_budget(&mut prompt);

    assert!(prompt.total_len() <= MAX_PROMPT_BYTES);
    let start = prompt
        .evidence
        .find(FROZEN_SET_HEADING)
        .expect("the section");
    let section = &prompt.evidence[start..];
    assert!(
        section.contains("- `tests/support/changed.rs` changed:"),
        "{section}"
    );
    assert!(
        section.contains("+pub fn ready() -> bool { true }"),
        "{section}"
    );
    assert!(
        section.contains("- `tests/support/gone.rs` is deleted"),
        "{section}"
    );
}

/// A git worktree for `demo` whose freeze holds the contract file and an
/// unchanged `tests/support/old.rs`; the stage's harness is `tests/support/**`.
pub(super) fn harness_fixture() -> ContractFixture {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, "demo");
    let workspace = WorkDir::new(&repo).unwrap();
    workspace.adopt_existing().unwrap();
    let work_dir = workspace.root().to_path_buf();
    freeze(
        &work_dir,
        &[
            ("tests/x_contract.rs", "assert!(false);\n"),
            ("tests/support/old.rs", "pub fn old() {}\n"),
        ],
    );
    fs::create_dir_all(worktree.join("tests/support")).unwrap();
    fs::write(worktree.join("tests/support/old.rs"), "pub fn old() {}\n").unwrap();
    let site = ExecutionSite {
        path: worktree.clone(),
        working_dir: ".".to_string(),
        worktree_present: true,
        root: worktree.clone(),
    };
    let stage = Stage {
        id: "demo".to_string(),
        harness: vec!["tests/support/**".to_string()],
        contracts: vec![contract()],
        ..Stage::default()
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
