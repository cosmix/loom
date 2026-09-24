//! Tests for the completion contract check, over a runner that replays
//! captured `cargo test` output instead of running anything.

use super::*;
use crate::verify::contracts::store::{write_freeze, FrozenFile, FREEZE_RECORD_VERSION};
use crate::verify::contracts::test_support::{contract, CONTRACT_CONTENT, CONTRACT_FILE};
use chrono::Utc;
use tempfile::TempDir;

const ONE_PASS: &str = include_str!("../../testrun/fixtures/cargo-test/one-pass.stdout");
const NO_MATCH: &str = include_str!("../../testrun/fixtures/cargo-test/no-match.stdout");

struct Replay(&'static str);

impl ProbeRunner for Replay {
    fn run(&self, _command: &str, _package_dir: &Path) -> Result<ProbeRun> {
        Ok(ProbeRun {
            stdout: self.0.to_string(),
            stderr: String::new(),
            exit_code: Some(0),
            timed_out: false,
        })
    }
}

struct Fixture {
    _tmp: TempDir,
    work_dir: std::path::PathBuf,
    worktree: std::path::PathBuf,
    stage: Stage,
}

/// A frozen stage with one cargo-test contract whose file is still as frozen.
fn fixture() -> Fixture {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join("work");
    let worktree = tmp.path().join("worktree");
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::create_dir_all(worktree.join("tests")).unwrap();
    std::fs::write(worktree.join(CONTRACT_FILE), CONTRACT_CONTENT).unwrap();
    let stage = Stage {
        id: "s1".to_string(),
        plan_version: 2,
        contracts: vec![contract()],
        ..Stage::default()
    };
    let record = FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: "s1".to_string(),
        session_id: "contract-session".to_string(),
        frozen_at: Utc::now(),
        base: "a".repeat(40),
        files: vec![FrozenFile {
            path: CONTRACT_FILE.to_string(),
            sha256: sha256_hex(CONTRACT_CONTENT),
        }],
        contracts: Vec::new(),
    };
    let copies = vec![(CONTRACT_FILE.to_string(), CONTRACT_CONTENT.to_vec())];
    write_freeze(&work_dir, &record, &copies).unwrap();
    Fixture {
        _tmp: tmp,
        work_dir,
        worktree,
        stage,
    }
}

fn run_check(fx: &Fixture, runner: &dyn ProbeRunner) -> Result<()> {
    check_with(&fx.stage, &fx.work_dir, &fx.worktree, &fx.worktree, runner)
}

#[test]
fn completion_passes_when_frozen_files_are_unchanged_and_contracts_pass() {
    let fx = fixture();
    run_check(&fx, &Replay(ONE_PASS)).unwrap();
}

#[test]
fn completion_fails_when_frozen_contract_changed() {
    let fx = fixture();
    std::fs::write(
        fx.worktree.join(CONTRACT_FILE),
        b"#[test]\nfn rejects_x() {}\n",
    )
    .unwrap();

    let error = run_check(&fx, &Replay(ONE_PASS)).unwrap_err().to_string();

    assert!(error.contains(CONTRACT_FILE), "{error}");
    assert!(error.contains("loom stage contracts restore s1"), "{error}");
    assert!(
        error.contains("loom stage dispute-contract s1 --contract <id> --reason ..."),
        "{error}"
    );
}

#[test]
fn completion_fails_when_contract_not_selected() {
    let fx = fixture();

    let error = run_check(&fx, &Replay(NO_MATCH)).unwrap_err().to_string();

    assert!(error.contains("not selected"), "{error}");
    assert!(error.contains("rejects-x"), "{error}");
}

#[test]
fn completion_fails_without_a_freeze() {
    let fx = fixture();
    let unfrozen = Stage {
        id: "s2".to_string(),
        ..fx.stage.clone()
    };

    let error = check_with(
        &unfrozen,
        &fx.work_dir,
        &fx.worktree,
        &fx.worktree,
        &Replay(ONE_PASS),
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("never frozen"), "{error}");
}
