use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::commands::status::data::execution_models_for_stage;
use crate::fs::work_dir::WorkDir;
use crate::subagent_lifecycle::{
    replay, CodexEvidence, CodexEvidenceKind, LifecycleRecord, LifecycleState, WorkerOutcome,
};

use super::has_correlated_lifecycle;
use super::jobs::workspace_state_dir;
use super::reconcile::{companion_outcome_with_state_root, reconcile_with_state_root};
use super::test_support::{identity, Fixture, INV_A, INV_B, STAGE};

#[test]
fn workspace_state_dir_matches_companion_v1_0_6() -> Result<()> {
    let directory = workspace_state_dir(Path::new("/state"), Path::new("/tmp/My project"))?;

    assert_eq!(directory, Path::new("/state/My-project-96d65a145a5cd013"));
    Ok(())
}

#[test]
fn exact_join_handles_two_units_and_duplicate_restart_replay() -> Result<()> {
    let fixture = Fixture::new()?;
    let first = fixture.authorization("unit-a", INV_A)?;
    let second = fixture.authorization("unit-b", INV_B)?;
    fixture.write_authorizations(&[
        fixture.authorization_value("unit-a", INV_A),
        fixture.authorization_value("unit-b", INV_B),
    ])?;
    fixture.write_job(&first, "task-a", "queued")?;
    fixture.write_job(&second, "task-b", "running")?;
    fixture.write_custom_job(
        &first,
        "task-newer",
        "running",
        "loom.v1:stage-1:session-1:unrelated:inv-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "gpt-5.6-sol",
        &fixture.workspace,
    )?;

    let report = reconcile_with_state_root(
        &fixture.work_dir,
        std::slice::from_ref(&fixture.session),
        &fixture.state_root,
    )?;

    assert_eq!(report.appended, 4);
    assert!(report.entries.iter().all(|entry| !entry.is_unknown()));
    let restarted = replay(&fixture.work_dir)?;
    assert_eq!(
        restarted.outcome(&identity(&first, "task-a")),
        WorkerOutcome::Active
    );
    assert_eq!(
        restarted.outcome(&identity(&second, "task-b")),
        WorkerOutcome::Active
    );

    let duplicate = reconcile_with_state_root(
        &fixture.work_dir,
        std::slice::from_ref(&fixture.session),
        &fixture.state_root,
    )?;
    assert_eq!((duplicate.appended, duplicate.duplicates), (0, 4));
    assert_eq!(lifecycle_records(&fixture)?.len(), 4);
    Ok(())
}

#[test]
fn running_then_completed_appends_exact_observations() -> Result<()> {
    let fixture = Fixture::new()?;
    let authorization = fixture.authorization("unit-a", INV_A)?;
    fixture.write_authorizations(&[fixture.authorization_value("unit-a", INV_A)])?;
    fixture.write_job(&authorization, "task-a", "running")?;

    let running = reconcile_with_state_root(
        &fixture.work_dir,
        std::slice::from_ref(&fixture.session),
        &fixture.state_root,
    )?;

    assert_eq!(running.appended, 2);
    assert_eq!(
        companion_outcome_with_state_root(&fixture.work_dir, &authorization, &fixture.state_root,),
        WorkerOutcome::Active
    );
    assert!(has_correlated_lifecycle(&fixture.work_dir, &authorization));
    let display_work_dir = WorkDir::new(&fixture.work_dir)?;
    assert_eq!(
        execution_models_for_stage(&display_work_dir, STAGE),
        ["gpt-5.6-sol (requested)"]
    );

    fixture.write_job(&authorization, "task-a", "completed")?;
    let completed = reconcile_with_state_root(
        &fixture.work_dir,
        std::slice::from_ref(&fixture.session),
        &fixture.state_root,
    )?;

    assert_eq!((completed.appended, completed.duplicates), (1, 1));
    assert_completed_evidence(&fixture, &authorization)
}

fn assert_completed_evidence(
    fixture: &Fixture,
    authorization: &super::CodexAuthorization,
) -> Result<()> {
    let records = lifecycle_records(fixture)?;
    assert_eq!(records.len(), 3);
    let terminal = records
        .iter()
        .find(|record| record.state == LifecycleState::Completed)
        .context("completed observation")?;
    let evidence: CodexEvidence = serde_json::from_value(terminal.evidence.clone())?;
    assert_eq!(evidence.evidence_kind, CodexEvidenceKind::Observation);
    assert_eq!(evidence.requested_model, "gpt-5.6-sol");
    assert_eq!(evidence.requested_effort, "xhigh");
    assert_eq!(evidence.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(evidence.turn_id.as_deref(), Some("turn-1"));
    assert!(evidence.terminal_at.is_some());
    assert_eq!(
        replay(&fixture.work_dir)?.outcome(&identity(authorization, "task-a")),
        WorkerOutcome::Succeeded
    );
    Ok(())
}

#[test]
fn failed_and_cancelled_remain_distinct_terminal_outcomes() -> Result<()> {
    let fixture = Fixture::new()?;
    let failed = fixture.authorization("unit-a", INV_A)?;
    let cancelled = fixture.authorization("unit-b", INV_B)?;
    fixture.write_authorizations(&[
        fixture.authorization_value("unit-a", INV_A),
        fixture.authorization_value("unit-b", INV_B),
    ])?;
    fixture.write_job(&failed, "task-failed", "failed")?;
    fixture.write_job(&cancelled, "task-cancelled", "cancelled")?;

    let report = reconcile_with_state_root(
        &fixture.work_dir,
        std::slice::from_ref(&fixture.session),
        &fixture.state_root,
    )?;

    assert_eq!(report.appended, 4);
    let index = replay(&fixture.work_dir)?;
    assert_eq!(
        index.outcome(&identity(&failed, "task-failed")),
        WorkerOutcome::Failed("companion failed".into())
    );
    assert_eq!(
        index.outcome(&identity(&cancelled, "task-cancelled")),
        WorkerOutcome::Cancelled("companion cancelled".into())
    );
    Ok(())
}

#[test]
fn contradictory_terminal_replay_fails_closed() -> Result<()> {
    let fixture = Fixture::new()?;
    let authorization = fixture.authorization("unit-a", INV_A)?;
    fixture.write_authorizations(&[fixture.authorization_value("unit-a", INV_A)])?;
    fixture.write_job(&authorization, "task-a", "completed")?;
    reconcile_with_state_root(
        &fixture.work_dir,
        std::slice::from_ref(&fixture.session),
        &fixture.state_root,
    )?;
    fixture.write_job(&authorization, "task-a", "failed")?;

    reconcile_with_state_root(
        &fixture.work_dir,
        std::slice::from_ref(&fixture.session),
        &fixture.state_root,
    )?;

    assert!(matches!(
        replay(&fixture.work_dir)?.outcome(&identity(&authorization, "task-a")),
        WorkerOutcome::Unknown(_)
    ));
    Ok(())
}

#[test]
fn missing_malformed_and_mismatched_evidence_never_writes() -> Result<()> {
    let fixture = Fixture::new()?;
    let cases = authorization_cases(&fixture)?;
    let mut rows: Vec<_> = cases.iter().map(|(value, _)| value.clone()).collect();
    let mut unsupported =
        fixture.authorization_value("unit-unsupported", "inv-77777777777777777777777777777777");
    unsupported["companion_version"] = serde_json::json!("1.0.7");
    rows.push(unsupported);
    fixture.write_authorizations(&rows)?;
    fixture.append_authorization_text("{not-json}\n{torn")?;
    write_mismatched_jobs(&fixture, &cases)?;
    fixture.write_job(&cases[6].1, "task-ambiguous-a", "running")?;
    fixture.write_job(&cases[6].1, "task-ambiguous-b", "running")?;

    let report = reconcile_with_state_root(
        &fixture.work_dir,
        std::slice::from_ref(&fixture.session),
        &fixture.state_root,
    )?;

    assert_eq!(report.appended, 0);
    assert_eq!(report.entries.len(), 9);
    assert!(report.entries.iter().all(|entry| entry.is_unknown()));
    assert!(!fixture
        .work_dir
        .join("subagents")
        .join(STAGE)
        .join("lifecycle.jsonl")
        .exists());
    Ok(())
}

fn authorization_cases(
    fixture: &Fixture,
) -> Result<Vec<(serde_json::Value, super::CodexAuthorization)>> {
    (1_u8..=7)
        .map(|number| {
            let unit = format!("unit-{number}");
            let invocation = format!("inv-{number:032x}");
            let value = fixture.authorization_value(&unit, &invocation);
            let authorization = super::CodexAuthorization::from_v2_value(&value)?;
            Ok((value, authorization))
        })
        .collect()
}

fn write_mismatched_jobs(
    fixture: &Fixture,
    cases: &[(serde_json::Value, super::CodexAuthorization)],
) -> Result<()> {
    fixture.write_malformed_job("task-malformed")?;
    let other_workspace = fixture.workspace.join("other");
    fs::create_dir_all(&other_workspace)?;
    let other_workspace = fs::canonicalize(other_workspace)?;
    fixture.write_custom_job(
        &cases[2].1,
        "task-worktree",
        "running",
        &cases[2].1.encoded_session_id(),
        "gpt-5.6-sol",
        &other_workspace,
    )?;
    fixture.write_custom_job(
        &cases[3].1,
        "task-session",
        "running",
        "loom.v1:stage-1:wrong-session:unit-4:inv-00000000000000000000000000000004",
        "gpt-5.6-sol",
        &fixture.workspace,
    )?;
    fixture.write_custom_job(
        &cases[4].1,
        "task-unit",
        "running",
        "loom.v1:stage-1:session-1:wrong-unit:inv-00000000000000000000000000000005",
        "gpt-5.6-sol",
        &fixture.workspace,
    )?;
    fixture.write_custom_job(
        &cases[5].1,
        "task-model",
        "running",
        &cases[5].1.encoded_session_id(),
        "gpt-wrong",
        &fixture.workspace,
    )?;
    Ok(())
}

fn lifecycle_records(fixture: &Fixture) -> Result<Vec<LifecycleRecord>> {
    let path = fixture
        .work_dir
        .join("subagents")
        .join(STAGE)
        .join("lifecycle.jsonl");
    fs::read_to_string(path)?
        .lines()
        .map(|line| serde_json::from_str(line).context("lifecycle fixture record"))
        .collect()
}
