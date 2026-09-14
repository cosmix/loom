use anyhow::Result;

use super::model::{
    EventOutcome, TerminalOutcome, WaitEvent, WaitLease, WorkerKind, WorkerSpec,
    WAIT_SCHEMA_VERSION,
};

pub fn event(lease: &WaitLease, outcome: EventOutcome, detail: Option<String>) -> WaitEvent {
    WaitEvent {
        schema_version: WAIT_SCHEMA_VERSION,
        wait_id: lease.wait_id.clone(),
        parent_session_id: lease.identity.parent_session_id.clone(),
        loom_session_id: lease.identity.loom_session_id.clone(),
        workers: lease.identity.worker_specs(),
        deadline: lease.deadline.clone(),
        outcome,
        evidence: lease.identity.evidence(),
        detail,
    }
}

pub fn terminal_outcome(outcome: &TerminalOutcome) -> EventOutcome {
    match outcome {
        TerminalOutcome::Succeeded => EventOutcome::Succeeded,
        TerminalOutcome::TimedOut => EventOutcome::TimedOut,
        TerminalOutcome::Failed => EventOutcome::Failed,
        TerminalOutcome::Cancelled => EventOutcome::Cancelled,
        TerminalOutcome::Unknown => EventOutcome::Unknown,
        TerminalOutcome::Interrupted => EventOutcome::Interrupted,
    }
}

pub fn emit(event: &WaitEvent, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(event)?);
    } else {
        println!("{}", human_line(event));
    }
    Ok(())
}

pub fn emit_unknown(detail: &str, json: bool) -> Result<()> {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "schema_version": WAIT_SCHEMA_VERSION,
                "outcome": "unknown",
                "detail": detail,
            })
        );
    } else {
        println!("unknown: {detail}");
    }
    Ok(())
}

pub fn bound_workers(lease: &WaitLease) -> String {
    worker_list(&lease.identity.worker_specs())
}

fn human_line(event: &WaitEvent) -> String {
    let label = outcome_label(&event.outcome);
    let workers = worker_list(&event.workers);
    let evidence = event
        .evidence
        .iter()
        .map(|item| format!("{}:{}#{}", item.source, item.path.display(), item.identity))
        .collect::<Vec<_>>()
        .join(",");
    let mut line = format!(
        "{label}: wait_id={} parent={} loom_session={} workers=[{}] deadline={}:{} evidence=[{}]",
        event.wait_id,
        event.parent_session_id,
        event.loom_session_id,
        workers,
        event.deadline.boot_id,
        event.deadline.monotonic_ns,
        evidence,
    );
    if let Some(detail) = &event.detail {
        line.push_str(&format!(" detail={detail}"));
    }
    line
}

fn outcome_label(outcome: &EventOutcome) -> &'static str {
    match outcome {
        EventOutcome::Waiting => "waiting",
        EventOutcome::Succeeded => "succeeded",
        EventOutcome::TimedOut => "timed out (deadline reached; not proof of worker death)",
        EventOutcome::Failed => "failed",
        EventOutcome::Cancelled => "cancelled",
        EventOutcome::AlreadyWaiting => "already waiting",
        EventOutcome::Busy => "busy",
        EventOutcome::Unknown => "unknown",
        EventOutcome::Interrupted => "interrupted",
    }
}

fn worker_list(workers: &[WorkerSpec]) -> String {
    workers
        .iter()
        .map(|worker| {
            let kind = match worker.kind {
                WorkerKind::Claude => "claude",
                WorkerKind::Codex => "codex",
            };
            format!("{kind}:{}", worker.id)
        })
        .collect::<Vec<_>>()
        .join(",")
}
