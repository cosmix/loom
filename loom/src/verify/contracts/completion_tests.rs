//! Tests for the completion contract check, over a runner that replays
//! captured `cargo test` output instead of running anything.

use super::*;
use crate::verify::contracts::store::{
    write_freeze, FrozenContract, FrozenFile, FREEZE_RECORD_VERSION,
};
use crate::verify::contracts::test_support::{
    contract, CONTRACT_CONTENT, CONTRACT_FILE, CONTRACT_ID,
};
use chrono::Utc;
use std::cell::RefCell;
use std::path::PathBuf;
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

/// A passing runner that remembers the directory it ran each probe from.
#[derive(Default)]
struct Recording(RefCell<Vec<PathBuf>>);

impl ProbeRunner for Recording {
    fn run(&self, command: &str, package_dir: &Path) -> Result<ProbeRun> {
        self.0.borrow_mut().push(package_dir.to_path_buf());
        Replay(ONE_PASS).run(command, package_dir)
    }
}

struct Fixture {
    _tmp: TempDir,
    work_dir: PathBuf,
    worktree: PathBuf,
    stage: Stage,
}

/// A frozen stage with one cargo-test contract whose file is still as frozen.
fn fixture() -> Fixture {
    fixture_with(Vec::new())
}

/// [`fixture`], its freeze record listing `frozen`.
fn fixture_with(frozen: Vec<FrozenContract>) -> Fixture {
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
        contracts: frozen,
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

/// The contract as the freeze recorded it, run from `cwd`.
fn frozen_at(cwd: &str) -> FrozenContract {
    FrozenContract {
        id: CONTRACT_ID.to_string(),
        adapter: Some("cargo-test".to_string()),
        outcome: "failed".to_string(),
        exit_code: Some(101),
        file: Some(CONTRACT_FILE.to_string()),
        cwd: Some(cwd.to_string()),
    }
}

/// Plant a `Cargo.toml` beside the contract file, nearer than the root.
fn plant_nearer_marker(fx: &Fixture) -> PathBuf {
    std::fs::write(fx.worktree.join("tests/Cargo.toml"), "").unwrap();
    fx.worktree.join("tests").canonicalize().unwrap()
}

/// The directories `fx`'s contract check runs its probes from.
fn run_dirs(fx: &Fixture) -> Vec<PathBuf> {
    let runner = Recording::default();
    run_check(fx, &runner).unwrap();
    runner.0.into_inner()
}

#[test]
fn a_recorded_run_dir_beats_a_runner_config_planted_since() {
    let legacy = FrozenContract {
        file: None,
        ..frozen_at(".")
    };
    for frozen in [frozen_at("."), legacy] {
        let fx = fixture_with(vec![frozen]);
        plant_nearer_marker(&fx);

        let root = fx.worktree.canonicalize().unwrap();
        assert_eq!(run_dirs(&fx), vec![root]);
    }
}

#[test]
fn an_explicit_plan_cwd_beats_the_recorded_one() {
    let mut fx = fixture_with(vec![frozen_at(".")]);
    fx.stage.contracts[0].cwd = Some("tests".to_string());

    let tests = fx.worktree.join("tests").canonicalize().unwrap();
    assert_eq!(run_dirs(&fx), vec![tests]);
}

#[test]
fn a_run_dir_recorded_for_another_file_is_ignored() {
    let fx = fixture_with(vec![FrozenContract {
        file: Some("tests/other_contract.rs".to_string()),
        ..frozen_at(".")
    }]);
    let nearer = plant_nearer_marker(&fx);

    assert_eq!(run_dirs(&fx), vec![nearer]);
}

#[test]
fn a_record_without_a_run_dir_falls_back_to_detection() {
    let legacy = FrozenContract {
        file: None,
        cwd: None,
        ..frozen_at(".")
    };
    for frozen in [Vec::new(), vec![legacy]] {
        let fx = fixture_with(frozen);
        let nearer = plant_nearer_marker(&fx);

        assert_eq!(run_dirs(&fx), vec![nearer]);
    }
}

#[test]
fn a_contract_that_cannot_be_resolved_is_reported_with_the_rest() {
    let fx = fixture_with(vec![frozen_at("gone")]);

    let error = run_check(&fx, &Replay(ONE_PASS)).unwrap_err().to_string();

    let expected = format!("contract '{CONTRACT_ID}' cannot be run:");
    assert!(error.contains(&expected), "{error}");
    assert!(
        error.contains("contract check failed for stage 's1'"),
        "{error}"
    );
}
