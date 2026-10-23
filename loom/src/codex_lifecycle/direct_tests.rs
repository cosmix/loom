use std::fs;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use crate::commands::status::data::execution_models_for_stage;
use crate::fs::work_dir::WorkDir;
use crate::models::forward_receipt::marker::{decode_marker_channel, MarkerChannel};
use crate::models::forward_receipt::{load_receipts, ForwardObservation, ForwardState};
use crate::subagent_lifecycle::{
    replay, CodexEvidence, CodexExecution, LifecycleProducer, LifecycleRecord, LifecycleState,
    WorkerIdentity, WorkerOutcome,
};

use super::super::reconcile::reconcile_with_state_root;
use super::super::test_support::{
    identity, liquidmon_direct_fixture, receipt_line, Fixture, EFFORT, FORWARDER, INV_A,
    LIQUIDMON_RUNNING, LIQUIDMON_STAGE, LIQUIDMON_SUCCEEDED, MODEL, PARENT, SESSION, STAGE,
};
use super::super::{
    codex_worker_identity, has_correlated_lifecycle, reconcile_at, CodexAuthorization,
    ReconcileReport,
};
use super::{AWAITING_RECEIPT, RECEIPT_WINDOW_SECS};

const THREAD: &str = "01a169b0-a2fd-7793-9874-db0889fa035c";

/// `secs` after the liquidmon authorization's `ts`.
fn at(secs: i64) -> DateTime<Utc> {
    let authorized_at: DateTime<Utc> = "2026-10-23T16:55:45Z".parse().expect("fixture ts");
    authorized_at + Duration::seconds(secs)
}

fn run(fixture: &Fixture, now: DateTime<Utc>) -> Result<ReconcileReport> {
    reconcile_at(
        &fixture.work_dir,
        std::slice::from_ref(&fixture.session),
        &fixture.state_root,
        now,
    )
}

/// The liquidmon fixture with `lines` as its receipt journal.
fn liquidmon<S: AsRef<str>>(lines: &[S]) -> Result<(Fixture, CodexAuthorization)> {
    let fixture = liquidmon_direct_fixture()?;
    fixture.write_receipts(lines)?;
    let authorization =
        CodexAuthorization::from_v2_value(&fixture.liquidmon_authorization_value()?)?;
    Ok((fixture, authorization))
}

fn terminal(state: &str, exit_code: i32) -> Result<String> {
    receipt_line(LIQUIDMON_SUCCEEDED, |value| {
        value["state"] = json!(state);
        value["exit_code"] = json!(exit_code);
    })
}

fn direct_identity(authorization: &CodexAuthorization, thread_id: &str) -> WorkerIdentity {
    let execution = CodexExecution::Direct {
        thread_id: thread_id.into(),
        tool_use_id: authorization.tool_use_id.clone(),
    };
    codex_worker_identity(authorization, execution)
}

fn records(fixture: &Fixture) -> Result<Vec<LifecycleRecord>> {
    fs::read_to_string(fixture.stage_path("lifecycle.jsonl"))?
        .lines()
        .map(|line| serde_json::from_str(line).context("lifecycle record"))
        .collect()
}

fn only_outcome(report: &ReconcileReport) -> Result<&WorkerOutcome> {
    assert_eq!(report.entries.len(), 1, "{report:?}");
    Ok(&report.entries.first().context("one entry")?.outcome)
}

fn assert_nothing_written(fixture: &Fixture, authorization: &CodexAuthorization) {
    assert!(!fixture.stage_path("lifecycle.jsonl").exists());
    assert!(!has_correlated_lifecycle(&fixture.work_dir, authorization));
}

#[test]
fn direct_forward_running_receipt_is_live_not_unknown() -> Result<()> {
    let (fixture, authorization) = liquidmon(&[LIQUIDMON_RUNNING])?;

    let running = run(&fixture, at(60))?;

    assert_eq!(only_outcome(&running)?, &WorkerOutcome::Active);
    assert_eq!(running.appended, 2);
    let identity = direct_identity(&authorization, THREAD);
    assert_eq!(
        replay(&fixture.work_dir)?.outcome(&identity),
        WorkerOutcome::Active
    );
    fixture.write_receipts(&[LIQUIDMON_RUNNING, LIQUIDMON_SUCCEEDED])?;
    let finished = run(&fixture, at(600))?;
    assert_eq!((finished.appended, finished.duplicates), (1, 1));
    assert_eq!(
        replay(&fixture.work_dir)?.outcome(&identity),
        WorkerOutcome::Succeeded
    );
    Ok(())
}

#[test]
fn direct_forward_succeeded_receipt_writes_correlated_lifecycle() -> Result<()> {
    let (fixture, authorization) = liquidmon(&[LIQUIDMON_RUNNING, LIQUIDMON_SUCCEEDED])?;

    let report = run(&fixture, at(3600))?;

    assert!(report.entries.iter().all(|entry| !entry.is_unknown()));
    assert_eq!(only_outcome(&report)?, &WorkerOutcome::Succeeded);
    let written = records(&fixture)?;
    assert_eq!(written.len(), 2);
    assert!(written
        .iter()
        .all(|record| record.producer == LifecycleProducer::CodexDirect));
    let finished = written
        .iter()
        .find(|record| record.state == LifecycleState::Completed)
        .context("completed record")?;
    let evidence: CodexEvidence = serde_json::from_value(finished.evidence.clone())?;
    assert_eq!(evidence.thread_id.as_deref(), Some(THREAD));
    assert_eq!(
        evidence.tool_use_id,
        Some(authorization.tool_use_id.clone())
    );
    assert_eq!((evidence.job_id, evidence.turn_id), (None, None));
    assert_eq!(evidence.terminal_at, Some(finished.observed_at));
    assert!(has_correlated_lifecycle(&fixture.work_dir, &authorization));
    let again = run(&fixture, at(7200))?;
    assert_eq!((again.appended, again.duplicates), (0, 2));
    Ok(())
}

#[test]
fn direct_forward_failed_receipt_is_failed_not_unknown() -> Result<()> {
    let (fixture, authorization) = liquidmon(&[LIQUIDMON_RUNNING.into(), terminal("failed", 2)?])?;

    let report = run(&fixture, at(600))?;

    let failed = WorkerOutcome::Failed("direct Codex forward failed with exit code 2".into());
    assert_eq!(only_outcome(&report)?, &failed);
    let identity = direct_identity(&authorization, THREAD);
    assert_eq!(replay(&fixture.work_dir)?.outcome(&identity), failed);
    Ok(())
}

#[test]
fn direct_forward_receipt_for_other_session_is_ignored() -> Result<()> {
    for field in [
        "loom_session_id",
        "parent_session_id",
        "agent_id",
        "tool_use_id",
    ] {
        let other = |value: &mut Value| value[field] = json!("other-identity");
        let receipts = [
            receipt_line(LIQUIDMON_RUNNING, other)?,
            receipt_line(LIQUIDMON_SUCCEEDED, other)?,
        ];
        let (fixture, authorization) = liquidmon(&receipts)?;

        let report = run(&fixture, at(60))?;

        assert_eq!(only_outcome(&report)?, &WorkerOutcome::Active, "{field}");
        assert_nothing_written(&fixture, &authorization);
    }
    Ok(())
}

#[test]
fn direct_forward_shows_requested_model_in_execution_models() -> Result<()> {
    let (fixture, _) = liquidmon(&[LIQUIDMON_RUNNING, LIQUIDMON_SUCCEEDED])?;

    reconcile_with_state_root(
        &fixture.work_dir,
        std::slice::from_ref(&fixture.session),
        &fixture.state_root,
    )?;

    let models = execution_models_for_stage(&WorkDir::new(&fixture.work_dir)?, LIQUIDMON_STAGE);
    assert!(
        models.contains(&"gpt-5.6-terra (requested)".to_owned()),
        "{models:?}"
    );
    Ok(())
}

/// A companion receipt for the standard fixture's `unit-a` naming `job_id`.
fn companion_receipt(job_id: &str) -> Result<String> {
    receipt_line(LIQUIDMON_SUCCEEDED, |value| {
        let fields = [
            ("parent_session_id", PARENT),
            ("agent_id", FORWARDER),
            ("tool_use_id", "tool-unit-a"),
            ("stage_id", STAGE),
            ("loom_session_id", SESSION),
            ("backend", "companion"),
            ("backend_id", job_id),
            ("codex_thread_id", "thread-1"),
            ("model", MODEL),
            ("effort", EFFORT),
        ];
        for (name, text) in fields {
            value[name] = json!(text);
        }
    })
}

#[test]
fn direct_forward_companion_rows_keep_companion_path() -> Result<()> {
    let fixture = Fixture::new()?;
    let authorization = fixture.authorization("unit-a", INV_A)?;
    fixture.write_authorizations(&[fixture.authorization_value("unit-a", INV_A)])?;
    fixture.write_job(&authorization, "task-a", "completed")?;
    fixture.write_receipts(&[companion_receipt("task-a")?])?;

    let report = run(&fixture, Utc::now())?;

    assert_eq!(only_outcome(&report)?, &WorkerOutcome::Succeeded);
    assert!(records(&fixture)?
        .iter()
        .all(|record| record.producer == LifecycleProducer::CodexCompanion));
    let outcome = replay(&fixture.work_dir)?.outcome(&identity(&authorization, "task-a"));
    assert_eq!(outcome, WorkerOutcome::Succeeded);
    Ok(())
}

#[test]
fn direct_forward_live_companion_without_receipt_still_reconciles_from_job() -> Result<()> {
    let fixture = Fixture::new()?;
    let authorization = fixture.authorization("unit-a", INV_A)?;
    fixture.write_authorizations(&[fixture.authorization_value("unit-a", INV_A)])?;
    fixture.write_job(&authorization, "task-a", "running")?;

    let report = run(&fixture, Utc::now())?;

    assert_eq!(only_outcome(&report)?, &WorkerOutcome::Active);
    assert_eq!(report.appended, 2);
    assert!(has_correlated_lifecycle(&fixture.work_dir, &authorization));
    Ok(())
}

#[test]
fn direct_forward_without_receipt_within_window_is_active() -> Result<()> {
    let (fixture, authorization) = liquidmon::<&str>(&[])?;

    let report = run(&fixture, at(RECEIPT_WINDOW_SECS - 1))?;

    assert_eq!(only_outcome(&report)?, &WorkerOutcome::Active);
    assert_eq!(report.entries[0].detail, AWAITING_RECEIPT);
    assert_nothing_written(&fixture, &authorization);
    Ok(())
}

#[test]
fn direct_forward_without_receipt_past_window_is_unknown() -> Result<()> {
    let (fixture, authorization) = liquidmon::<&str>(&[])?;

    let report = run(&fixture, at(RECEIPT_WINDOW_SECS))?;

    assert!(matches!(
        only_outcome(&report)?,
        WorkerOutcome::Unknown(detail)
            if detail.starts_with("no forward receipt 600 s after authorization")
    ));
    assert_nothing_written(&fixture, &authorization);
    Ok(())
}

#[test]
fn direct_forward_two_units_join_their_own_receipts() -> Result<()> {
    let fixture = liquidmon_direct_fixture()?;
    let first = fixture.liquidmon_authorization_value()?;
    let mut second = first.clone();
    second["unit_id"] = json!("L2-second");
    second["invocation_id"] = json!("inv-11111111111111111111111111111111");
    second["tool_use_id"] = json!("toolu_second");
    fixture.write_authorizations(&[first.clone(), second.clone()])?;
    let second_receipt = |state: &str, exit_code: Option<i32>| {
        receipt_line(LIQUIDMON_SUCCEEDED, |value| {
            value["tool_use_id"] = json!("toolu_second");
            value["backend_id"] = json!("thread-second");
            value["codex_thread_id"] = json!("thread-second");
            value["state"] = json!(state);
            value["exit_code"] = json!(exit_code);
        })
    };
    fixture.write_receipts(&[
        LIQUIDMON_RUNNING.into(),
        second_receipt("running", None)?,
        second_receipt("failed", Some(3))?,
        LIQUIDMON_SUCCEEDED.into(),
    ])?;

    run(&fixture, at(600))?;

    let index = replay(&fixture.work_dir)?;
    let first = CodexAuthorization::from_v2_value(&first)?;
    let second = CodexAuthorization::from_v2_value(&second)?;
    assert_eq!(
        index.outcome(&direct_identity(&first, THREAD)),
        WorkerOutcome::Succeeded
    );
    assert_eq!(
        index.outcome(&direct_identity(&second, "thread-second")),
        WorkerOutcome::Failed("direct Codex forward failed with exit code 3".into())
    );
    Ok(())
}

#[test]
fn direct_forward_damaged_receipt_journal_fails_closed() -> Result<()> {
    let receipts = [LIQUIDMON_RUNNING, LIQUIDMON_SUCCEEDED, "{torn"];
    let (fixture, authorization) = liquidmon(&receipts)?;

    let report = run(&fixture, at(60))?;

    let damaged = WorkerOutcome::Unknown("forward receipt journal is damaged".into());
    assert_eq!(only_outcome(&report)?, &damaged);
    assert_nothing_written(&fixture, &authorization);
    Ok(())
}

#[test]
fn direct_forward_cancelled_marker_word_yields_a_canceled_receipt() -> Result<()> {
    let channel = decode_marker_channel(&format!(
        "LOOM-FORWARD-START {{\"v\":1,\"backend\":\"direct\",\"thread_id\":\"{THREAD}\"}}\n\
         LOOM-FORWARD-END {{\"v\":1,\"backend\":\"direct\",\"thread_id\":\"{THREAD}\",\
         \"outcome\":\"cancelled\",\"exit_code\":124}}\n--- LOOM-FORWARD-OUTPUT ---"
    ));
    let MarkerChannel::Finished(_, end) = channel else {
        panic!("direct cancelled marker did not decode: {channel:?}");
    };
    assert_eq!((end.outcome, end.exit_code), (ForwardState::Canceled, 124));
    let row = ForwardObservation::decode_line(&terminal("cancelled", 124)?)?;
    assert_eq!(row.state, ForwardState::Canceled);
    let (fixture, _) = liquidmon(&[LIQUIDMON_RUNNING.into(), terminal("canceled", 124)?])?;

    let report = run(&fixture, at(600))?;

    let cancelled = WorkerOutcome::Cancelled("direct Codex forward canceled".into());
    assert_eq!(only_outcome(&report)?, &cancelled);
    Ok(())
}

#[test]
fn direct_forward_conflicting_terminal_receipts_stay_unknown() -> Result<()> {
    let receipts = [
        LIQUIDMON_RUNNING.into(),
        terminal("failed", 2)?,
        LIQUIDMON_SUCCEEDED.into(),
    ];
    let (fixture, authorization) = liquidmon(&receipts)?;
    let loaded = load_receipts(&fixture.stage_path("forward-receipts.jsonl"))?;
    let folded = loaded.receipts.first().context("folded receipt")?;
    assert_eq!(
        (folded.state, folded.exit_code),
        (ForwardState::Unknown, Some(2))
    );

    let report = run(&fixture, at(600))?;

    assert!(matches!(only_outcome(&report)?, WorkerOutcome::Unknown(_)));
    assert_nothing_written(&fixture, &authorization);
    Ok(())
}

#[test]
fn direct_forward_terminal_without_running_stays_unknown() -> Result<()> {
    let (fixture, authorization) = liquidmon(&[LIQUIDMON_SUCCEEDED])?;

    let report = run(&fixture, at(600))?;

    assert!(matches!(only_outcome(&report)?, WorkerOutcome::Unknown(_)));
    assert_eq!(report.appended, 0);
    assert_nothing_written(&fixture, &authorization);
    Ok(())
}

#[test]
fn direct_forward_without_receipt_or_job_writes_nothing() -> Result<()> {
    let fixture = Fixture::new()?;
    let authorization = fixture.authorization("unit-a", INV_A)?;
    fixture.write_authorizations(&[fixture.authorization_value("unit-a", INV_A)])?;

    let report = run(
        &fixture,
        authorization.authorized_at + Duration::seconds(60),
    )?;

    assert_eq!(only_outcome(&report)?, &WorkerOutcome::Active);
    assert_nothing_written(&fixture, &authorization);
    Ok(())
}
