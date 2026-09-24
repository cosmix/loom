//! Fixtures shared by the contract tests: one cargo-test contract, a stage
//! carrying it, and a real git repository with the stage worktree where loom
//! puts it.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::daemon::ContractRunReport;
use crate::models::stage::{Stage, StageStatus};
use crate::plan::schema::ContractSpec;
use crate::verify::contracts::store::{write_freeze, FreezeRecord, FREEZE_RECORD_VERSION};

pub(crate) const CONTRACT_ID: &str = "rejects-x";
pub(crate) const CONTRACT_FILE: &str = "tests/x_contract.rs";
pub(crate) const CONTRACT_CONTENT: &[u8] = b"#[test]\nfn rejects_x() { assert!(false); }\n";

pub(crate) fn contract() -> ContractSpec {
    ContractSpec {
        id: CONTRACT_ID.to_string(),
        file: CONTRACT_FILE.to_string(),
        test: "tests::rejects_x".to_string(),
        runner: Some("cargo-test".to_string()),
        scenario: "x arrives".to_string(),
        rejects: "accepting x".to_string(),
    }
}

/// An executing v2 stage owned by `session_id`, with [`contract`], whose
/// worktree is `.worktrees/<stage_id>`.
pub(crate) fn contract_stage(stage_id: &str, session_id: &str) -> Stage {
    Stage {
        id: stage_id.to_string(),
        status: StageStatus::Executing,
        plan_version: 2,
        session: Some(session_id.to_string()),
        worktree: Some(stage_id.to_string()),
        contracts: vec![contract()],
        ..Stage::default()
    }
}

/// The report of a contract session whose one contract failed.
pub(crate) fn red_reports() -> Vec<ContractRunReport> {
    vec![ContractRunReport {
        contract_id: CONTRACT_ID.to_string(),
        adapter: Some("cargo-test".to_string()),
        outcome: "failed".to_string(),
        exit_code: Some(101),
    }]
}

/// Leave the freeze record `loom stage contracts freeze` writes (DESIGN D8)
/// for `stage_id`, frozen by `session_id`, with no frozen files.
pub(crate) fn write_test_freeze(work_dir: &Path, stage_id: &str, session_id: &str) {
    let record = FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: stage_id.to_string(),
        session_id: session_id.to_string(),
        frozen_at: chrono::Utc::now(),
        base: "main".to_string(),
        files: Vec::new(),
        contracts: Vec::new(),
    };
    write_freeze(work_dir, &record, &[]).unwrap();
}

/// Make `repo_root` a repository on `main` with one commit, add the stage
/// worktree on `loom/<stage_id>`, and write the contract file into it,
/// untracked. Returns the worktree root.
pub(crate) fn contract_worktree(repo_root: &Path, stage_id: &str) -> PathBuf {
    std::fs::create_dir_all(repo_root).unwrap();
    git(repo_root, &["init", "-q", "-b", "main"]);
    std::fs::write(repo_root.join("README.md"), "base\n").unwrap();
    git(repo_root, &["add", "README.md"]);
    git(repo_root, &["commit", "-q", "-m", "base"]);
    let branch = format!("loom/{stage_id}");
    let relative = format!(".worktrees/{stage_id}");
    git(
        repo_root,
        &["worktree", "add", "-q", "-b", &branch, &relative],
    );
    let worktree = repo_root.join(relative);
    let contract_file = worktree.join(CONTRACT_FILE);
    std::fs::create_dir_all(contract_file.parent().unwrap()).unwrap();
    std::fs::write(contract_file, CONTRACT_CONTENT).unwrap();
    worktree
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}
