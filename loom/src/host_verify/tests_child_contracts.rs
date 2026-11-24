//! Tests for a check's contracts (`child_contracts`): `loom check
//! --contracts`, a replacement trial, and the contracts a full check runs.
//! Each contract is a shell command (no test-runner adapter), so its verdict
//! is its exit code. Every test starts loom's real confinement and skips
//! where it cannot start; every one is serial, because the spawn observer is
//! one process-wide slot.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use chrono::Utc;
use serial_test::serial;
use tempfile::TempDir;

use super::child::run_job;
use super::submit::REPLACEMENTS_FILE;
use super::tests_child::{confinement_unavailable, labels_and_verdicts};
use super::{digest, new_job_id, store, JobKind, JobRecord, JobResult, JobState};
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::FileReplacement;
use crate::models::stage::AcceptanceCriterion;
use crate::utils::sha256_hex;
use crate::verify::contracts::store::FREEZE_RECORD_VERSION;
use crate::verify::contracts::store::{write_freeze, FreezeRecord, FrozenFile};
use crate::verify::contracts::test_support::{
    contract, contract_stage, shell_contract_worktree, CONTRACT_CONTENT, CONTRACT_FILE,
};
use crate::verify::contracts::trial::{load_receipt, RECEIPT_FILE};
use crate::verify::transitions::save_stage;

pub(super) const STAGE: &str = "s1";
/// Passes only on a contract file holding `PASS`.
const NEEDS_PASS: &str = "grep -q PASS tests/x_contract.rs";

/// A frozen stage whose contracts are the shell commands `tests`, with the
/// acceptance criteria `acceptance`, and a plan the state root names.
pub(super) struct Fixture {
    _cache_env: crate::test_cache_env::CacheEnv,
    _tmp: TempDir,
    pub(super) work_dir: PathBuf,
    pub(super) worktree: PathBuf,
}

impl Fixture {
    pub(super) fn new(tests: &[(&str, &str)], acceptance: &[&str]) -> Self {
        let cache_env = crate::test_cache_env::CacheEnv::pin();
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().canonicalize().unwrap().join("repo");
        let worktree = shell_contract_worktree(&repo, STAGE);
        let workspace = WorkDir::new(&repo).unwrap();
        workspace.adopt_existing().unwrap();
        let work_dir = workspace.root().to_path_buf();
        let mut stage = contract_stage(STAGE, "session-a");
        stage.contracts = tests
            .iter()
            .map(|(id, test)| crate::plan::schema::ContractSpec {
                id: id.to_string(),
                runner: None,
                test: Some(test.to_string()),
                ..contract()
            })
            .collect();
        stage.acceptance = acceptance
            .iter()
            .map(|command| AcceptanceCriterion::Simple(command.to_string()))
            .collect();
        save_stage(&stage, &work_dir).unwrap();
        let fixture = Self {
            _cache_env: cache_env,
            _tmp: tmp,
            work_dir,
            worktree,
        };
        fixture.write_plan();
        fixture.freeze();
        fixture
    }

    fn write_plan(&self) {
        let plan = self.work_dir.join("PLAN-contracts.md");
        let yaml = format!(
            "loom:\n  version: 1\n  stages:\n    - id: {STAGE}\n      name: {STAGE}\n      working_dir: \".\"\n      acceptance:\n        - \"true\"\n"
        );
        let text = format!(
            "# Plan\n\n<!-- loom METADATA -->\n\n```yaml\n{yaml}```\n\n<!-- END loom METADATA -->\n"
        );
        fs::write(&plan, text).unwrap();
        let config = format!(
            "[plan]\nsource_path = \"{}\"\nplan_id = \"contracts\"\nplan_name = \"contracts\"\nbase_branch = \"main\"\n",
            plan.display()
        );
        fs::write(self.work_dir.join("config.toml"), config).unwrap();
    }

    fn freeze(&self) {
        let record = FreezeRecord {
            version: FREEZE_RECORD_VERSION,
            stage_id: STAGE.to_string(),
            session_id: "session-c".to_string(),
            frozen_at: Utc::now(),
            base: "main".to_string(),
            files: vec![FrozenFile {
                path: CONTRACT_FILE.to_string(),
                sha256: sha256_hex(CONTRACT_CONTENT),
            }],
            contracts: Vec::new(),
        };
        let files = [(CONTRACT_FILE.to_string(), CONTRACT_CONTENT.to_vec())];
        write_freeze(&self.work_dir, &record, &files).unwrap();
    }

    /// A running check of the worktree as it stands, a contracts check when
    /// `contracts`, with `replacements` saved as the daemon saves them.
    pub(super) fn job(&self, contracts: bool, replacements: &[(&str, &str)]) -> JobRecord {
        let record = JobRecord {
            id: new_job_id(),
            submission_order: 1,
            stage_id: STAGE.to_string(),
            session_id: "session-a".to_string(),
            kind: JobKind::Check,
            head: digest::head(&self.worktree).unwrap(),
            tree_digest: digest::tree_digest(&self.worktree).unwrap(),
            no_cache: false,
            criteria: None,
            contracts,
            state: JobState::Running,
            requests: vec!["r1".to_string()],
            queued_at: Utc::now(),
            started_at: Some(Utc::now()),
            finished_at: None,
            pid: None,
            detail: None,
            at_base: false,
        };
        store::save_current(&self.work_dir, &record).unwrap();
        if !replacements.is_empty() {
            let files: Vec<FileReplacement> = replacements
                .iter()
                .map(|(file, content)| FileReplacement {
                    file: file.to_string(),
                    content: content.to_string(),
                    base_sha256: None,
                })
                .collect();
            let bytes = serde_json::to_vec(&files).unwrap();
            store::save_job_file(&self.work_dir, STAGE, &record.id, REPLACEMENTS_FILE, &bytes)
                .unwrap();
        }
        record
    }

    fn run(&self, record: &JobRecord) -> JobResult {
        run_job(&self.work_dir, STAGE, &record.id).unwrap()
    }

    /// The contract file's bytes and permission bits.
    pub(super) fn contract_file(&self) -> (Vec<u8>, u32) {
        let path = self.worktree.join(CONTRACT_FILE);
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        (fs::read(&path).unwrap(), mode)
    }

    fn receipt_saved(&self, record: &JobRecord) -> bool {
        store::load_job_file(&self.work_dir, STAGE, &record.id, RECEIPT_FILE)
            .unwrap()
            .is_some()
    }
}

#[test]
#[serial]
fn a_contracts_check_reports_each_contract() {
    if confinement_unavailable("a_contracts_check_reports_each_contract") {
        return;
    }
    let fx = Fixture::new(&[("passes", "true"), ("fails", "false")], &["true"]);
    let result = fx.run(&fx.job(true, &[]));
    assert_eq!(
        labels_and_verdicts(&result),
        [("contract:passes", true), ("contract:fails", false)]
    );
    assert!(!result.passed, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
}

#[test]
#[serial]
fn a_contracts_check_runs_no_acceptance_criterion() {
    if confinement_unavailable("a_contracts_check_runs_no_acceptance_criterion") {
        return;
    }
    let fx = Fixture::new(&[("passes", "true")], &["false"]);
    let result = fx.run(&fx.job(true, &[]));
    assert_eq!(labels_and_verdicts(&result), [("contract:passes", true)]);
    assert!(result.passed, "{result:?}");
}

#[test]
#[serial]
fn a_full_check_runs_the_frozen_contracts_after_its_criteria() {
    if confinement_unavailable("a_full_check_runs_the_frozen_contracts_after_its_criteria") {
        return;
    }
    let fx = Fixture::new(&[("fails", "false")], &["true"]);
    let result = fx.run(&fx.job(false, &[]));
    // A plan v2 standard stage also runs the test-integrity gate last.
    assert_eq!(
        labels_and_verdicts(&result),
        [
            ("acceptance-0", true),
            ("contract:fails", false),
            ("test-integrity", true)
        ]
    );
    assert!(!result.passed, "{result:?}");
}

#[test]
#[serial]
fn a_failing_trial_leaves_the_worktree_as_it_was_and_records_no_receipt() {
    if confinement_unavailable(
        "a_failing_trial_leaves_the_worktree_as_it_was_and_records_no_receipt",
    ) {
        return;
    }
    let fx = Fixture::new(&[("needs-pass", NEEDS_PASS)], &[]);
    let before = fx.contract_file();
    let record = fx.job(true, &[(CONTRACT_FILE, "FAIL\n")]);
    let result = fx.run(&record);
    assert!(!result.passed, "{result:?}");
    assert!(
        result.error.is_none(),
        "the tree must be unchanged: {result:?}"
    );
    assert!(labels_and_verdicts(&result).contains(&("contract:needs-pass", false)));
    assert_eq!(
        fx.contract_file(),
        before,
        "the frozen file must be restored"
    );
    assert!(!fx.receipt_saved(&record));
    assert_eq!(load_receipt(&fx.work_dir, STAGE).unwrap(), None);
}

#[test]
#[serial]
fn a_passing_trial_records_its_receipt() {
    if confinement_unavailable("a_passing_trial_records_its_receipt") {
        return;
    }
    let fx = Fixture::new(&[("needs-pass", NEEDS_PASS)], &[]);
    let before = fx.contract_file();
    let replacement = "PASS\n";
    let record = fx.job(true, &[(CONTRACT_FILE, replacement)]);
    let result = fx.run(&record);
    assert!(result.passed, "{result:?}");
    assert_eq!(
        labels_and_verdicts(&result),
        [("contract:needs-pass", true), ("format", true)]
    );
    assert_eq!(
        fx.contract_file(),
        before,
        "the frozen file must be restored"
    );
    assert_eq!(
        load_receipt(&fx.work_dir, STAGE).unwrap(),
        None,
        "a trial the runner has not settled is no receipt"
    );
    let mut done = record.clone();
    done.state = JobState::Passed;
    store::save_current(&fx.work_dir, &done).unwrap();
    let receipt = load_receipt(&fx.work_dir, STAGE)
        .unwrap()
        .expect("a receipt");
    assert_eq!(receipt.head, record.head);
    assert_eq!(receipt.tree_digest, record.tree_digest);
    assert_eq!(receipt.files.len(), 1);
    assert_eq!(receipt.files[0].path, CONTRACT_FILE);
    assert_eq!(receipt.files[0].sha256, sha256_hex(replacement.as_bytes()));
}

#[test]
#[serial]
fn a_trial_of_a_file_outside_the_freeze_is_refused() {
    if confinement_unavailable("a_trial_of_a_file_outside_the_freeze_is_refused") {
        return;
    }
    let fx = Fixture::new(&[("needs-pass", NEEDS_PASS)], &[]);
    let record = fx.job(true, &[("README.md", "PASS\n")]);
    let result = fx.run(&record);
    assert!(!result.passed, "{result:?}");
    let error = result.error.as_deref().expect("a refusal");
    assert!(error.contains("not a frozen file"), "{error}");
    assert!(result.criteria.is_empty(), "nothing ran: {result:?}");
    assert!(!fx.receipt_saved(&record));
}

#[test]
#[serial]
fn a_trial_whose_overlay_changed_during_the_runs_records_no_receipt() {
    if confinement_unavailable("a_trial_whose_overlay_changed_during_the_runs_records_no_receipt") {
        return;
    }
    // The contract passes while overwriting the overlaid file with other bytes.
    let rewrite = "printf 'PASS, other bytes\\n' > tests/x_contract.rs";
    let fx = Fixture::new(&[("rewrites", rewrite)], &[]);
    let before = fx.contract_file();
    let record = fx.job(true, &[(CONTRACT_FILE, "PASS\n")]);
    let result = fx.run(&record);
    assert!(!result.passed, "{result:?}");
    assert!(
        labels_and_verdicts(&result).contains(&("frozen-files", false)),
        "{result:?}"
    );
    // The restore leaves a file holding bytes written during the run alone;
    // the drift check reports it.
    assert_eq!(
        fx.contract_file(),
        (b"PASS, other bytes\n".to_vec(), before.1),
        "a file written during the run is left as written"
    );
    assert!(!fx.receipt_saved(&record));
    assert_eq!(load_receipt(&fx.work_dir, STAGE).unwrap(), None);
}
