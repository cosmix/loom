//! The contract dispute briefing reads the test the writer chose: from the
//! freeze record once frozen, from the daemon's saved choices before.

use chrono::Utc;
use std::collections::BTreeMap;
use std::path::Path;

use super::*;
use crate::models::dispute::DisputeRequest;
use crate::models::stage::Stage;
use crate::orchestrator::adjudication::prompt::ExecutionSite;
use crate::plan::schema::{ContractChoice, ContractChoices};

fn plan_spec(hint: Option<&str>) -> ContractSpec {
    ContractSpec {
        id: "rejects-x".to_string(),
        file: "tests/x_contract.rs".to_string(),
        test: hint.map(str::to_string),
        runner: Some("cargo-test".to_string()),
        cwd: None,
        scenario: "x arrives".to_string(),
        rejects: "accepting x".to_string(),
    }
}

fn record_with(test: Option<&str>) -> store::FreezeRecord {
    store::FreezeRecord {
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
            test: test.map(str::to_string),
        }],
    }
}

fn block(plan: &ContractSpec, record: store::FreezeRecord) -> String {
    let effective = store::effective_contract(&record, plan);
    spec_block(plan, &effective, &Ok(Some(record)))
}

#[test]
fn the_spec_block_prints_the_writers_test_and_the_hint_it_replaced() {
    let text = block(&plan_spec(Some("hint::x")), record_with(Some("writer::x")));

    assert!(text.contains("\ntest: writer::x\n"), "{text}");
    assert!(
        text.contains("\nwriter's test: chosen when the contracts were frozen"),
        "{text}"
    );
    assert!(text.contains("(hint::x)"), "{text}");
}

#[test]
fn the_spec_block_names_a_test_filled_in_where_the_plan_had_none() {
    let text = block(&plan_spec(None), record_with(Some("writer::x")));

    assert!(text.contains("\ntest: writer::x\n"), "{text}");
    assert!(
        text.contains("in place of the plan's hint (none)"),
        "{text}"
    );
}

#[test]
fn the_spec_block_has_no_writer_line_when_the_hint_stands() {
    let text = block(&plan_spec(Some("hint::x")), record_with(None));

    assert!(text.contains("\ntest: hint::x\n"), "{text}");
    assert!(!text.contains("writer's test"), "{text}");
}

/// A prompt input over an empty work directory, whose stage holds `plan`.
fn with_input<T>(
    plan: ContractSpec,
    work_dir: &Path,
    check: impl FnOnce(&KindPromptInput<'_>) -> T,
) -> T {
    let stage = Stage {
        id: "demo".to_string(),
        contracts: vec![plan],
        ..Stage::default()
    };
    let request = DisputeRequest {
        id: 4,
        stage_id: "demo".to_string(),
        kind: DisputeKind::Contract {
            contract_id: "rejects-x".to_string(),
        },
        reason: "r".to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 0,
        created_at: Utc::now(),
    };
    let site = ExecutionSite {
        path: work_dir.to_path_buf(),
        working_dir: ".".to_string(),
        worktree_present: true,
        root: work_dir.to_path_buf(),
    };
    check(&KindPromptInput {
        stage: &stage,
        dispute_id: 4,
        request: &request,
        site: &site,
        worktree: Some(work_dir),
        work_dir,
    })
}

#[test]
fn before_a_freeze_the_saved_choice_is_the_test() {
    let temp = tempfile::tempdir().unwrap();
    let saved: ContractChoices = BTreeMap::from([(
        "rejects-x".to_string(),
        ContractChoice {
            test: Some("saved::x".to_string()),
            ..ContractChoice::default()
        },
    )]);
    crate::verify::contracts::choices::save_choices(temp.path(), "demo", &saved).unwrap();
    let plan = plan_spec(None);

    let effective = with_input(plan.clone(), temp.path(), |input| {
        effective_spec(input, &plan, &Ok(None))
    });

    assert_eq!(effective.test.as_deref(), Some("saved::x"));
}

#[test]
fn a_contract_nobody_named_a_test_for_cannot_be_run_and_says_so() {
    let temp = tempfile::tempdir().unwrap();
    let plan = plan_spec(None);

    let run = with_input(plan.clone(), temp.path(), |input| {
        contract_run(input, &plan, temp.path(), &Ok(None))
    });

    let message = format!("{:#}", run.unwrap_err());
    assert!(
        message.contains("--test rejects-x=<identifier>"),
        "{message}"
    );
}
