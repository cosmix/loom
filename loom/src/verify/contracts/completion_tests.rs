//! Tests for the completion contract check, over a runner that replays
//! captured `cargo test` output instead of running anything.

use super::*;
use crate::verify::contracts::frozen_set::read_frozen_set;
use crate::verify::contracts::store::{
    write_freeze, FrozenContract, FrozenFile, FREEZE_RECORD_VERSION,
};
use crate::verify::contracts::test_support::{
    contract, write_files, CONTRACT_CONTENT, CONTRACT_FILE, CONTRACT_ID, PACKAGE_MANIFEST,
};
use chrono::Utc;
use std::cell::RefCell;
use std::path::PathBuf;
use tempfile::TempDir;

pub(super) const ONE_PASS: &str = include_str!("../../testrun/fixtures/cargo-test/one-pass.stdout");
const NO_MATCH: &str = include_str!("../../testrun/fixtures/cargo-test/no-match.stdout");

pub(super) struct Replay(pub(super) &'static str);

impl ProbeRunner for Replay {
    fn run(&self, _command: &str, _package_dir: &Path) -> Result<ProbeRun> {
        Ok(ProbeRun {
            stdout: self.0.to_string(),
            stderr: String::new(),
            exit_code: Some(0),
            timed_out: false,
            cached: None,
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

pub(super) struct Fixture {
    _tmp: TempDir,
    work_dir: PathBuf,
    pub(super) worktree: PathBuf,
    pub(super) stage: Stage,
}

/// A frozen stage with one cargo-test contract whose file is still as frozen.
fn fixture() -> Fixture {
    fixture_with(Vec::new())
}

/// [`fixture`], its freeze record listing `frozen`.
pub(super) fn fixture_with(frozen: Vec<FrozenContract>) -> Fixture {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join("work");
    let worktree = tmp.path().join("worktree");
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::create_dir_all(worktree.join("tests")).unwrap();
    std::fs::write(worktree.join(CONTRACT_FILE), CONTRACT_CONTENT).unwrap();
    write_files(
        &worktree,
        &[("Cargo.toml", PACKAGE_MANIFEST), ("src/lib.rs", "")],
    );
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

pub(super) fn run_check(fx: &Fixture, runner: &dyn ProbeRunner) -> Result<()> {
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

/// A certified cache hit whose kept tail holds only `running 0 tests` blocks.
struct CachedTail(Option<u64>);

impl ProbeRunner for CachedTail {
    fn run(&self, command: &str, package_dir: &Path) -> Result<ProbeRun> {
        let mut run = Replay(NO_MATCH).run(command, package_dir)?;
        run.cached = Some(CachedRun {
            tests_executed: self.0,
        });
        Ok(run)
    }
}

#[test]
fn a_cache_hit_is_judged_by_its_recorded_test_count_not_its_tail() {
    let fx = fixture();

    run_check(&fx, &CachedTail(Some(1))).unwrap();
    let error = run_check(&fx, &CachedTail(Some(0)))
        .unwrap_err()
        .to_string();

    assert!(error.contains("not selected"), "{error}");
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
pub(super) fn frozen_at(cwd: &str) -> FrozenContract {
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
fn plant_nearer_marker(fx: &Fixture, manifest: &str) -> PathBuf {
    std::fs::write(fx.worktree.join("tests/Cargo.toml"), manifest).unwrap();
    fx.worktree.join("tests").canonicalize().unwrap()
}

/// A package of its own at `tests/`, whose one test target is the contract.
const TESTS_PACKAGE: &str = "[package]\nname = \"inner\"\nversion = \"0.1.0\"\n\
     [[test]]\nname = \"x\"\npath = \"x_contract.rs\"\n";

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
        plant_nearer_marker(&fx, "[workspace]\n");

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
    let nearer = plant_nearer_marker(&fx, TESTS_PACKAGE);

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
        let nearer = plant_nearer_marker(&fx, TESTS_PACKAGE);

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

/// Re-freeze `fx`'s stage the way an accepted contract dispute does: its
/// contract files, the harness matches `harness` and every recorded path.
fn refreeze_as_accept(fx: &Fixture, harness: &[String]) -> Vec<String> {
    let record = store::load_freeze(&fx.work_dir, "s1").unwrap().unwrap();
    let recorded: Vec<String> = record.files.iter().map(|f| f.path.clone()).collect();
    let files = read_frozen_set(&fx.worktree, &fx.stage.contracts, harness, &recorded).unwrap();
    store::refreeze_set(&fx.work_dir, "s1", &files, &[]).unwrap()
}

#[test]
fn after_an_accepted_refreeze_no_frozen_file_is_changed() {
    let fx = fixture();
    let support = fx.worktree.join("tests/support");
    std::fs::create_dir_all(&support).unwrap();
    std::fs::write(
        support.join("helper.rs"),
        "pub fn ready() -> bool { false }\n",
    )
    .unwrap();
    std::fs::write(support.join("gone.rs"), "pub fn gone() {}\n").unwrap();
    let harness = ["tests/support/gone.rs", "tests/support/helper.rs"].map(String::from);
    refreeze_as_accept(&fx, &harness);
    std::fs::write(
        support.join("helper.rs"),
        "pub fn ready() -> bool { true }\n",
    )
    .unwrap();
    std::fs::remove_file(support.join("gone.rs")).unwrap();
    std::fs::write(
        fx.worktree.join(CONTRACT_FILE),
        b"#[test]\nfn rejects_x() {}\n",
    )
    .unwrap();
    let record = store::load_freeze(&fx.work_dir, "s1").unwrap().unwrap();
    let before = changed_frozen_files(&record, &fx.worktree, &fx.worktree).unwrap();
    assert_eq!(before.len(), 3, "{before:?}");

    let dropped = refreeze_as_accept(&fx, &harness[1..]);

    assert_eq!(dropped, vec!["tests/support/gone.rs".to_string()]);
    let record = store::load_freeze(&fx.work_dir, "s1").unwrap().unwrap();
    let after = changed_frozen_files(&record, &fx.worktree, &fx.worktree).unwrap();
    assert!(after.is_empty(), "{after:?}");
}

/// `fx`'s one contract re-pointed at `file`/`test`, with `files` written.
fn collect_fixture(file: &str, test: &str, files: &[(&str, &str)]) -> Fixture {
    let mut fx = fixture();
    write_files(&fx.worktree, files);
    fx.stage.contracts[0].file = file.to_string();
    fx.stage.contracts[0].test = test.to_string();
    fx
}

fn assert_only_collection_fails(error: &str, named: &str) {
    assert!(error.contains(named), "{error}");
    assert!(!error.contains("fails:"), "{error}");
    assert!(!error.contains("not selected"), "{error}");
}

#[test]
fn a_path_redirected_contract_module_fails_though_the_run_passes() {
    let fx = collect_fixture(
        "src/contract_tests.rs",
        "contract_tests::rejects_x",
        &[
            (
                "src/lib.rs",
                "#[path = \"elsewhere.rs\"]\nmod contract_tests;\n",
            ),
            ("src/contract_tests.rs", "#[test]\nfn rejects_x() {}\n"),
        ],
    );

    let error = run_check(&fx, &Replay(ONE_PASS)).unwrap_err().to_string();

    assert_only_collection_fails(&error, "#[path]");
    assert!(error.contains("contract 'rejects-x': "), "{error}");
}

#[test]
fn a_cfg_removed_module_root_fails_though_the_run_passes() {
    let fx = collect_fixture(
        "src/m/contract_tests.rs",
        "m::contract_tests::rejects_x",
        &[
            ("src/lib.rs", "mod m;\n"),
            ("src/m/mod.rs", "#![cfg(not(test))]\nmod contract_tests;\n"),
            ("src/m/contract_tests.rs", "#[test]\nfn rejects_x() {}\n"),
        ],
    );

    let error = run_check(&fx, &Replay(ONE_PASS)).unwrap_err().to_string();

    assert_only_collection_fails(&error, "#![cfg");
}

#[test]
fn a_workspace_root_run_dir_that_skips_the_package_fails() {
    let mut fx = collect_fixture(
        "a/tests/x.rs",
        "x::rejects_x",
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"a\", \"b\"]\ndefault-members = [\"b\"]\n",
            ),
            (
                "a/Cargo.toml",
                "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            (
                "b/Cargo.toml",
                "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("a/tests/x.rs", "#[test]\nfn rejects_x() {}\n"),
        ],
    );
    fx.stage.contracts[0].cwd = Some(".".to_string());

    let error = run_check(&fx, &Replay(ONE_PASS)).unwrap_err().to_string();

    assert!(error.contains("does not select package"), "{error}");
    assert!(!error.contains("fails:"), "{error}");
}
