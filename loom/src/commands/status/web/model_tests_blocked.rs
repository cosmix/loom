//! The blocked stage of the dashboard fixture: a long reason with a command in
//! it, and the history of a tmux session that ended.
use super::*;

/// What the stage agent wrote with `loom stage block`.
const REASON: &str = "Adjudicator needs an unsandboxed run only the operator can do. From the \
    worktree root run: TROOFOO_STACK=ephemeral bun run stack:ephemeral --with objectstore -- \
    bun test packages/storage packages/db/src/ephemeral-services; report exit code, test \
    summary, and .stack/objectstore.log on failure. Criterion 5 (stack:ephemeral --with \
    objectstore) cannot pass in this sandbox: it denies AF_UNIX socket creation everywhere.\n\
    Or allow AF_UNIX for this stage. All work committed; 0 open findings.";

pub(super) fn stage_local_runtime() -> StageSummary {
    let empty: &[&str] = &[];
    let mut stage = stage!(
        "local-runtime",
        "Local runtime",
        StageStatus::Blocked,
        StageType::Standard,
        &["knowledge-bootstrap"],
        None,
        Some(1800),
        Some(1500),
        None,
        ActivityStatus::Idle,
        None,
        None,
        None,
        None,
        None,
        false,
        None,
        false,
        0,
        None,
        None,
        false,
        "opus",
        None,
        None,
        empty,
        0,
        None,
        Some(SessionBackendKind::Tmux)
    );
    stage.close_reason = Some(REASON.to_owned());
    stage.acceptance = [
        "cargo build",
        "cargo test",
        "bun run typecheck",
        "bun test packages/storage",
        "TROOFOO_STACK=ephemeral bun run stack:ephemeral --with objectstore -- bun test packages/db/src/ephemeral-services",
    ]
    .map(str::to_owned)
    .to_vec();
    stage.terminal_history_session = Some("session-9c1d4e7a-1788700000".to_owned());
    stage
}
