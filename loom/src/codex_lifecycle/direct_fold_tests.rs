//! How a direct receipt's folded state reaches the lifecycle journal.

use anyhow::{Context, Result};
use serde_json::json;

use crate::subagent_lifecycle::{
    replay, CodexEvidenceOutcome, CodexExecution, LifecycleState, WorkerIdentity, WorkerOutcome,
};

use super::super::codex_worker_identity;
use super::super::test_support::{
    at, direct_identity, liquidmon, only_outcome, receipt_line, records, run, LIQUIDMON_RUNNING,
    THREAD,
};
use super::{unknown_fold, UnknownFold};

/// A `queued` direct observation is the live forward, so reconcile records
/// it as running rather than closing the execution as unknown.
#[test]
fn direct_forward_queued_receipt_is_running_not_unknown() -> Result<()> {
    let queued = receipt_line(LIQUIDMON_RUNNING, |value| value["state"] = json!("queued"))?;
    let (fixture, authorization) = liquidmon(&[queued])?;

    let report = run(&fixture, at(60))?;

    assert_eq!(only_outcome(&report)?, &WorkerOutcome::Active);
    assert_eq!(report.appended, 2);
    let written = records(&fixture)?;
    assert!(written
        .iter()
        .all(|record| record.state == LifecycleState::Running));
    let observation = written.last().context("observation record")?;
    let evidence: crate::subagent_lifecycle::CodexEvidence =
        serde_json::from_value(observation.evidence.clone())?;
    assert_eq!(evidence.outcome, CodexEvidenceOutcome::Running);
    assert_eq!(evidence.detail, None);
    assert_eq!(
        replay(&fixture.work_dir)?.outcome(&direct_identity(&authorization, THREAD)),
        WorkerOutcome::Active
    );
    Ok(())
}

/// Only a direct `unknown` is classified against the journal: it is recorded
/// once the journal opened the execution, skipped once it closed it, and
/// refused while the journal never saw the forward run.
#[test]
fn unknown_fold_classifies_against_the_journal() -> Result<()> {
    let (fixture, authorization) = liquidmon(&[LIQUIDMON_RUNNING])?;
    let direct = direct_identity(&authorization, THREAD);
    let companion = codex_worker_identity(
        &authorization,
        CodexExecution::Companion {
            job_id: "job-a".into(),
        },
    );
    assert!(matches!(direct, WorkerIdentity::Codex { .. }));

    assert_eq!(
        unknown_fold(&[], &direct, LifecycleState::Unknown),
        UnknownFold::Unopened
    );
    assert_eq!(
        unknown_fold(&[], &direct, LifecycleState::Running),
        UnknownFold::Record
    );
    assert_eq!(
        unknown_fold(&[], &companion, LifecycleState::Unknown),
        UnknownFold::Record
    );

    run(&fixture, at(60))?;
    let opened = records(&fixture)?;
    assert_eq!(
        unknown_fold(&opened, &direct, LifecycleState::Unknown),
        UnknownFold::Record
    );

    let unknown = LIQUIDMON_RUNNING.replace(r#""state":"running""#, r#""state":"unknown""#);
    fixture.write_receipts(&[LIQUIDMON_RUNNING, unknown.as_str()])?;
    run(&fixture, at(120))?;
    let closed = records(&fixture)?;
    assert!(closed
        .iter()
        .any(|record| record.state == LifecycleState::Unknown));
    assert_eq!(
        unknown_fold(&closed, &direct, LifecycleState::Unknown),
        UnknownFold::Recorded
    );
    Ok(())
}
