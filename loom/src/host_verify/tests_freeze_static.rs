//! A freeze job's refusal returned to the waiting writer: a static failure
//! (only the `format` outcome red), an interrupted run and a run the daemon
//! stopped are counted in `refused.json` (`returned`), and the third of a
//! session, like every other failed job, is recorded for the tick to file.
//! Uses the `tests_freeze` fixture.

use serial_test::serial;

use super::super::digest;
use super::super::freeze_memo::run_memoized;
use super::super::freeze_prepare::prepare_planned;
use super::super::runner::DAEMON_STOPPED;
use super::super::tests_freeze::{refused_with, Fixture, STAGE};
use super::super::{store, CriterionOutcome, JobRecord, JobResult, JobState};
use super::{apply_freeze_result, PreparedContract, REPORTS_FILE};
use crate::daemon::ContractRunReport;
use crate::plan::schema::{ContractChoice, ContractChoices};
use crate::relay::sha256_hex;
use crate::stage_service::contracts::{locate, read_files};
use crate::stage_service::{handle_freeze_contracts, Response};
use crate::verify::contracts::choices::{load_choices, load_effective_stage, save_choices};
use crate::verify::contracts::freeze::JudgedRun;
use crate::verify::contracts::refusal::{daemon_refusal, DaemonRefusal, INFRASTRUCTURE_PREFIX};
use crate::verify::contracts::test_support::{red_reports, CONTRACT_ID, PACKAGE_MANIFEST};

const RETURNED: &str = "returned to the contract writer (";
const FILED: &str = "filed as a freeze-refusal dispute: ";

fn outcome(label: &str, passed: bool, summary: &str) -> CriterionOutcome {
    CriterionOutcome {
        label: label.to_string(),
        passed,
        summary: summary.to_string(),
        log: None,
    }
}

/// A red contract and a red static gate naming the writer's file.
fn format_only() -> Vec<CriterionOutcome> {
    vec![
        outcome("contract:rejects-x", true, "rejects-x failed as required"),
        outcome(
            "format",
            false,
            "tests/x_contract.rs: line 2 is not formatted",
        ),
    ]
}

/// The `order`th freeze job of the fixture's writer, ended in `state`, with
/// `criteria` and `error` as its saved result when `criteria` is `Some`.
fn job(
    fx: &Fixture,
    order: u64,
    state: JobState,
    criteria: Option<Vec<CriterionOutcome>>,
    error: Option<&str>,
) -> JobRecord {
    let mut record = fx.job(state);
    record.submission_order = order;
    if let Some(criteria) = criteria {
        let result = JobResult {
            job_id: record.id.clone(),
            head: record.head.clone(),
            passed: false,
            criteria,
            error: error.map(str::to_string),
            policy_hash: None,
        };
        store::save_result(&fx.work_dir, STAGE, &result).unwrap();
    }
    record
}

fn on_record(fx: &Fixture) -> DaemonRefusal {
    daemon_refusal(&fx.work_dir, STAGE).unwrap().unwrap()
}

#[test]
#[serial]
fn a_static_failure_is_returned_to_the_writer() {
    let fx = Fixture::new("exit 1");
    let record = job(&fx, 1, JobState::Failed, Some(format_only()), None);
    let message = refused_with(fx.apply(&record));
    assert!(
        message.starts_with(&format!(
            "{RETURNED}1 of 3): fix what the output above names and run loom stage contracts \
             freeze {STAGE} again"
        )),
        "{message}"
    );
    assert!(message.contains("tests/x_contract.rs: line 2"), "{message}");
    let refusal = on_record(&fx);
    assert_eq!(refusal.returned, 1);
    assert_eq!(refusal.session_id, fx.session_id);
    assert!(refusal.problems[0].starts_with("format: "), "{refusal:?}");
}

#[test]
#[serial]
fn other_failed_jobs_are_filed_at_once() {
    let red_contract = vec![
        outcome("contract:rejects-x", false, "rejects-x passed"),
        outcome("format", false, "tests/x_contract.rs: line 2"),
    ];
    let cases = [
        ("a failed contract outcome", Some(red_contract), None),
        (
            "a result with an error",
            Some(format_only()),
            Some("the run stopped"),
        ),
        ("a missing result", None, None),
    ];
    for (label, criteria, error) in cases {
        let fx = Fixture::new("exit 1");
        let record = job(&fx, 1, JobState::Failed, criteria, error);
        let message = refused_with(fx.apply(&record));
        assert!(message.starts_with(FILED), "{label}: {message}");
        assert_eq!(on_record(&fx).returned, 0, "{label} is a standing refusal");
    }
}

#[test]
#[serial]
fn stopped_and_interrupted_runs_count_with_the_static_refusals() {
    let fx = Fixture::new("exit 1");
    let interrupted = job(&fx, 1, JobState::Interrupted, None, None);
    let message = refused_with(fx.apply(&interrupted));
    assert!(
        message.starts_with(&format!("{RETURNED}1 of 3)")),
        "{message}"
    );
    let refusal = on_record(&fx);
    assert_eq!(refusal.returned, 1);
    assert!(
        refusal.problems[0].starts_with(INFRASTRUCTURE_PREFIX),
        "{refusal:?}"
    );

    let mut stopped = job(&fx, 2, JobState::Cancelled, None, None);
    stopped.detail = Some(DAEMON_STOPPED.to_string());
    let message = refused_with(fx.apply(&stopped));
    assert!(
        message.starts_with(&format!("{RETURNED}2 of 3)")),
        "{message}"
    );
    let refusal = on_record(&fx);
    assert_eq!(refusal.returned, 2);
    assert!(
        refusal.problems[0].starts_with(INFRASTRUCTURE_PREFIX),
        "{refusal:?}"
    );

    let third = job(&fx, 3, JobState::Failed, Some(format_only()), None);
    let message = refused_with(fx.apply(&third));
    assert!(message.starts_with(FILED), "the third is filed: {message}");
    assert_eq!(on_record(&fx).returned, 0);
}

#[test]
#[serial]
fn the_same_job_applied_twice_counts_once() {
    let fx = Fixture::new("exit 1");
    let record = job(&fx, 1, JobState::Failed, Some(format_only()), None);
    fx.apply(&record);
    let message = refused_with(fx.apply(&record));
    assert!(
        message.starts_with(&format!("{RETURNED}1 of 3)")),
        "{message}"
    );
    assert_eq!(on_record(&fx).returned, 1);
}

/// A passed job bound to the effective contracts as they stand now.
fn passed_job(fx: &Fixture, reports: &[ContractRunReport]) -> JobRecord {
    let record = fx.job(JobState::Passed);
    let stage = load_effective_stage(STAGE, &fx.work_dir).unwrap();
    let site = locate(&fx.work_dir, &stage).unwrap();
    let files: Vec<serde_json::Value> = read_files(&site, &stage)
        .unwrap()
        .iter()
        .map(|(path, bytes)| serde_json::json!({ "path": path, "sha256": sha256_hex(bytes) }))
        .collect();
    let binding = serde_json::json!({
        "head": record.head,
        "tree_digest": record.tree_digest,
        "contracts_digest": sha256_hex(&serde_json::to_vec(&stage.contracts).unwrap()),
        "files": files,
        "reports": reports,
    });
    let bytes = serde_json::to_vec(&binding).unwrap();
    store::save_job_file(&fx.work_dir, STAGE, &record.id, REPORTS_FILE, &bytes).unwrap();
    record
}

#[test]
#[serial]
fn a_job_run_under_other_choices_fails_its_binding() {
    let fx = Fixture::new("exit 1");
    let record = passed_job(&fx, &red_reports());
    let chosen = ContractChoice {
        test: Some("other_test".to_string()),
        ..ContractChoice::default()
    };
    save_choices(
        &fx.work_dir,
        STAGE,
        &ContractChoices::from([(CONTRACT_ID.to_string(), chosen)]),
    )
    .unwrap();
    let message = refused_with(apply_freeze_result(&fx.work_dir, &record).unwrap());
    assert!(
        message.contains("contract definitions changed"),
        "{message}"
    );
    assert!(!fx.frozen());
}

/// The writer's `--test` passed admission while an untracked manifest made a
/// runner answer for the contract file; the manifest is gone from the tree the
/// job runs on, so the test would run as a shell command, which only the plan
/// may give. The job refuses before it runs anything or freezes anything.
#[test]
#[serial]
fn a_test_choice_that_needs_a_manifest_the_job_tree_lacks_is_refused() {
    let fx = Fixture::new("exit 1");
    let manifest = fx.worktree.join("Cargo.toml");
    std::fs::write(&manifest, PACKAGE_MANIFEST).unwrap();
    let choices = ContractChoices::from([(
        CONTRACT_ID.to_string(),
        ContractChoice {
            test: Some("touch pwned".to_string()),
            ..ContractChoice::default()
        },
    )]);
    let (head, tree) = (
        digest::head(&fx.worktree).unwrap(),
        digest::tree_digest(&fx.worktree).unwrap(),
    );
    let admitted = handle_freeze_contracts(
        &fx.work_dir,
        STAGE,
        &fx.session_id,
        &head,
        &tree,
        "r1",
        &choices,
    )
    .unwrap();
    assert_eq!(admitted, Response::Ok, "admission sees the manifest");
    assert_eq!(load_choices(&fx.work_dir, STAGE).unwrap(), choices);

    std::fs::remove_file(&manifest).unwrap();
    let (record, result) = fx.run(fx.job(JobState::Running));
    let error = result.error.clone().unwrap_or_default();
    assert!(error.contains("needs a test-runner adapter"), "{error}");
    assert!(result.criteria.is_empty(), "no contract ran: {result:?}");
    assert_eq!(record.state, JobState::Failed);
    assert!(!fx.worktree.join("pwned").exists());
    let message = refused_with(fx.apply(&record));
    assert!(message.starts_with(FILED), "{message}");
    assert_eq!(on_record(&fx).returned, 0, "a standing refusal");
    assert!(!fx.frozen());
}

/// Prepare and run the effective `contract` the way the job does, over a
/// package directory holding `files`. Returns the preparation's error, if
/// any, and whether the contract's command was started: it writes `ran` in
/// the directory when it is.
fn drive(
    files: &[(&str, &str)],
    planned_test: Option<&str>,
    chosen_test: &str,
) -> (Option<String>, bool) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    crate::verify::contracts::test_support::write_files(&root, files);
    let mut effective = crate::verify::contracts::test_support::contract();
    effective.runner = None;
    effective.test = Some(chosen_test.to_string());
    let mut planned = effective.clone();
    planned.test = planned_test.map(str::to_string);
    let ran = root.join("ran");
    let driven = run_memoized(
        std::slice::from_ref(&effective),
        |contract| prepare_planned(&root, &root, std::slice::from_ref(&planned), contract),
        PreparedContract::build_key,
        |_, _| {
            std::fs::write(&ran, "").unwrap();
            Ok(JudgedRun {
                judged: Err("red".to_string()),
                compile_failed: false,
            })
        },
        |_, _| {},
    );
    (driven.err().map(|e| format!("{e:#}")), ran.exists())
}

/// The adapter the writer's `--test` was admitted under can be gone when the
/// job resolves the contract (a manifest created and deleted around the
/// admission): the writer's test then has no adapter at the read that decides
/// how it runs, and must not run as a shell command.
#[test]
fn a_writer_test_without_an_adapter_where_the_contract_runs_never_runs() {
    let (error, ran) = drive(&[], None, "touch ran");
    let error = error.expect("the preparation is refused");
    assert!(error.contains("needs a test-runner adapter"), "{error}");
    assert!(!ran, "the writer's command was run");

    let (error, ran) = drive(&[], Some("echo planned"), "touch ran");
    assert!(error.is_some(), "a writer test replacing a plan hint");
    assert!(!ran);
}

#[test]
fn plan_and_adapter_tests_still_run() {
    let (error, ran) = drive(&[], Some("touch ran"), "touch ran");
    assert_eq!(error, None, "a plan-set shell test");
    assert!(ran);

    let files = [("Cargo.toml", PACKAGE_MANIFEST), ("src/lib.rs", "")];
    let (error, ran) = drive(&files, None, "tests::rejects_x");
    assert_eq!(error, None, "a writer test with an adapter");
    assert!(ran);
}
