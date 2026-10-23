use std::time::Duration;

use anyhow::Result;

use crate::codex_lifecycle::test_support::{
    liquidmon_direct_fixture, LIQUIDMON_RUNNING, LIQUIDMON_STAGE, LIQUIDMON_SUCCEEDED,
};
use crate::codex_lifecycle::{reconcile_at, CodexAuthorization};
use crate::subagent_lifecycle::{CodexExecution, WorkerIdentity, WorkerOutcome};

use super::codex_binding::resolve_wait_codex_execution;
use super::engine::{EvidenceSource, LifecycleEvidence};
use super::model::{BoundWorker, CodexAuthority, WorkerKind, WorkerSpec};

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
    let worker = BoundWorker {
        worker: WorkerSpec {
            kind: WorkerKind::Codex,
            id: authorization.unit_id.clone(),
        },
        lifecycle_identity: execution.identity,
        authority: Some(CodexAuthority::from(authorization)),
        evidence: Vec::new(),
    };
    let evidence = LifecycleEvidence {
        work_dir: fixture.work_dir.clone(),
        stall_budget: Duration::from_secs(600),
    };
    assert_eq!(evidence.outcome(&worker), WorkerOutcome::Succeeded);
    Ok(())
}
