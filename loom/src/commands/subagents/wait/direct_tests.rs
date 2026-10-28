use std::time::Duration;

use anyhow::{Context, Result};

use crate::codex_lifecycle::test_support::{
    liquidmon_direct_fixture, Fixture, LIQUIDMON_RUNNING, LIQUIDMON_STAGE, LIQUIDMON_SUCCEEDED,
};
use crate::codex_lifecycle::{reconcile_at, CodexAuthorization};
use crate::subagent_lifecycle::{CodexExecution, WorkerIdentity, WorkerOutcome};

use super::codex_binding::resolve_wait_codex_execution;
use super::engine::{wait_for_workers, EngineResult, EvidenceSource, POLL_INTERVAL};
use super::evidence::LifecycleEvidence;
use super::lease::deadline_after;
use super::model::{
    BoundWorker, CodexAuthority, TerminalOutcome, WaitIdentity, WorkerKind, WorkerSpec,
};
use super::tests::{FakeClock, FakeSleeper};

#[test]
fn direct_forward_watch_binding_resolves_terminal_after_reconcile() -> Result<()> {
    let fixture = liquidmon_direct_fixture()?;
    fixture.write_receipts(&[LIQUIDMON_RUNNING, LIQUIDMON_SUCCEEDED])?;
    let authorization =
        CodexAuthorization::from_v2_value(&fixture.liquidmon_authorization_value()?)?;
    let now = authorization.authorized_at + chrono::Duration::seconds(600);
    let sessions = std::slice::from_ref(&fixture.session);
    let report = reconcile_at(&fixture.work_dir, sessions, &fixture.state_root, now)?;
    assert_eq!(report.appended, 2);

    let execution =
        resolve_wait_codex_execution(&fixture.work_dir, LIQUIDMON_STAGE, &authorization)?;

    assert!(matches!(
        &execution.identity,
        WorkerIdentity::Codex {
            execution: CodexExecution::Direct { thread_id, tool_use_id },
            ..
        } if thread_id == "01a169b0-a2fd-7793-9874-db0889fa035c"
            && tool_use_id == &authorization.tool_use_id
    ));
    let worker = bound_worker(execution.identity, authorization);
    assert_eq!(
        evidence(&fixture).outcome(&worker),
        WorkerOutcome::Succeeded
    );
    Ok(())
}

fn bound_worker(identity: WorkerIdentity, authorization: CodexAuthorization) -> BoundWorker {
    BoundWorker {
        worker: WorkerSpec {
            kind: WorkerKind::Codex,
            id: authorization.unit_id.clone(),
        },
        lifecycle_identity: identity,
        authority: Some(CodexAuthority::from(authorization)),
        evidence: Vec::new(),
    }
}

fn evidence(fixture: &Fixture) -> LifecycleEvidence {
    LifecycleEvidence {
        work_dir: fixture.work_dir.clone(),
        stall_budget: Duration::from_secs(600),
    }
}

/// Run the bounded wait engine over `worker` alone with a fake clock, so the
/// deadline passes only through the sleeps the engine takes.
fn wait(fixture: &Fixture, worker: &BoundWorker, timeout: Duration) -> EngineResult {
    let identity = WaitIdentity {
        canonical_repo: "/repo".into(),
        canonical_worktree: "/repo/worktree".into(),
        stage_id: worker.lifecycle_identity.stage_id().to_owned(),
        loom_session_id: "loom-a".into(),
        parent_session_id: "parent-a".into(),
        workers: vec![worker.clone()],
    };
    let clock = FakeClock::new();
    let sleeper = FakeSleeper::new(clock.clone());
    let deadline = deadline_after(&clock, timeout).expect("deadline");
    wait_for_workers(
        &identity,
        &deadline,
        &clock,
        &sleeper,
        &evidence(fixture),
        POLL_INTERVAL,
    )
}

/// A watch binds a direct forward on its raw `running` receipt and then reads
/// only the lifecycle journal; a later `unknown` fold must reach the journal
/// so the watch ends with the unknown outcome before its deadline, and the
/// fold is recorded only once, however many instants it is observed at.
/// While the journal still calls the forward running, the same watch holds
/// until its deadline.
#[test]
fn direct_forward_bound_watch_ends_on_unknown_receipt() -> Result<()> {
    let fixture = liquidmon_direct_fixture()?;
    let authorization =
        CodexAuthorization::from_v2_value(&fixture.liquidmon_authorization_value()?)?;
    let now = authorization.authorized_at + chrono::Duration::seconds(600);
    let sessions = std::slice::from_ref(&fixture.session);
    fixture.write_receipts(&[LIQUIDMON_RUNNING])?;
    reconcile_at(&fixture.work_dir, sessions, &fixture.state_root, now)?;
    let execution =
        resolve_wait_codex_execution(&fixture.work_dir, LIQUIDMON_STAGE, &authorization)?;
    let worker = bound_worker(execution.identity, authorization);
    assert_eq!(evidence(&fixture).outcome(&worker), WorkerOutcome::Active);
    let held = wait(&fixture, &worker, Duration::from_secs(6));
    assert_eq!(held.outcome, TerminalOutcome::TimedOut);
    assert!(held.polls >= 3, "{held:?}");

    let unknown = LIQUIDMON_RUNNING.replace(r#""state":"running""#, r#""state":"unknown""#);
    fixture.write_receipts(&[LIQUIDMON_RUNNING, unknown.as_str()])?;
    let report = reconcile_at(&fixture.work_dir, sessions, &fixture.state_root, now)?;
    assert_eq!(report.appended, 1);
    assert!(matches!(
        evidence(&fixture).outcome(&worker),
        WorkerOutcome::Unknown(_)
    ));
    let ended = wait(&fixture, &worker, Duration::from_secs(3_600));
    assert_eq!(ended.outcome, TerminalOutcome::Unknown);
    assert_eq!(ended.polls, 0);
    let detail = ended.detail.context("unknown detail")?;
    assert!(detail.starts_with("codex:"), "{detail}");
    assert!(detail.contains("unknown:"), "{detail}");

    let later = unknown.replace("17:00:37.322417869Z", "17:00:41.000000000Z");
    fixture.write_receipts(&[LIQUIDMON_RUNNING, later.as_str()])?;
    let report = reconcile_at(&fixture.work_dir, sessions, &fixture.state_root, now)?;
    assert_eq!(report.appended, 0);
    assert!(matches!(
        evidence(&fixture).outcome(&worker),
        WorkerOutcome::Unknown(_)
    ));
    Ok(())
}

/// A bound direct worker with no lifecycle record yet is unknown for want of
/// evidence, not settled: the watch keeps polling to its deadline.
#[test]
fn direct_forward_bound_watch_holds_while_evidence_is_missing() -> Result<()> {
    let fixture = liquidmon_direct_fixture()?;
    let authorization =
        CodexAuthorization::from_v2_value(&fixture.liquidmon_authorization_value()?)?;
    fixture.write_receipts(&[LIQUIDMON_RUNNING])?;
    let execution =
        resolve_wait_codex_execution(&fixture.work_dir, LIQUIDMON_STAGE, &authorization)?;
    let worker = bound_worker(execution.identity, authorization);
    assert_eq!(
        evidence(&fixture).outcome(&worker),
        WorkerOutcome::Unknown("no matching lifecycle evidence".into())
    );

    let held = wait(&fixture, &worker, Duration::from_secs(6));

    assert_eq!(held.outcome, TerminalOutcome::Unknown);
    assert!(held.polls >= 3, "{held:?}");
    let detail = held.detail.context("deadline detail")?;
    assert!(
        detail.starts_with("unknown worker evidence at deadline"),
        "{detail}"
    );
    Ok(())
}

/// An unreadable journal settles nothing: the bound forward may still run.
#[test]
fn direct_forward_bound_watch_holds_on_an_unreadable_journal() -> Result<()> {
    let fixture = liquidmon_direct_fixture()?;
    let authorization =
        CodexAuthorization::from_v2_value(&fixture.liquidmon_authorization_value()?)?;
    fixture.write_receipts(&[LIQUIDMON_RUNNING])?;
    let execution =
        resolve_wait_codex_execution(&fixture.work_dir, LIQUIDMON_STAGE, &authorization)?;
    let worker = bound_worker(execution.identity, authorization);
    let journal = fixture
        .work_dir
        .join("subagents")
        .join(LIQUIDMON_STAGE)
        .join("lifecycle.jsonl");
    std::fs::create_dir_all(&journal)?;

    assert!(!evidence(&fixture).unknown_settled(&worker));
    let held = wait(&fixture, &worker, Duration::from_secs(6));
    assert!(held.polls >= 3, "{held:?}");
    Ok(())
}

/// A direct receipt that folds to `Unknown` (a reported unknown outcome, or
/// contradicting observations) is refused by a new bind at once, even after
/// its `Running` start was reconciled.
#[test]
fn direct_forward_watch_binding_refuses_unknown_receipts() -> Result<()> {
    let reported = LIQUIDMON_RUNNING.replace(r#""state":"running""#, r#""state":"unknown""#);
    let contradicting = LIQUIDMON_RUNNING.replace(
        "01a169b0-a2fd-7793-9874-db0889fa035c",
        "01a169b0-a2fd-7793-9874-db0889fa0000",
    );
    for later in [reported, contradicting] {
        let fixture = liquidmon_direct_fixture()?;
        let authorization =
            CodexAuthorization::from_v2_value(&fixture.liquidmon_authorization_value()?)?;
        let now = authorization.authorized_at + chrono::Duration::seconds(600);
        let sessions = std::slice::from_ref(&fixture.session);
        fixture.write_receipts(&[LIQUIDMON_RUNNING])?;
        reconcile_at(&fixture.work_dir, sessions, &fixture.state_root, now)?;
        fixture.write_receipts(&[LIQUIDMON_RUNNING, later.as_str()])?;

        let error =
            resolve_wait_codex_execution(&fixture.work_dir, LIQUIDMON_STAGE, &authorization)
                .err()
                .context("an unknown direct receipt must not bind")?;
        assert_eq!(error.to_string(), "invalid exact forward receipt");
    }
    Ok(())
}
