//! The per-writer matrix (`doc/plans/PLAN-loom-state-confinement.md` section
//! 5), keyed by session type and request kind.

use super::kind::RequestKind;
use crate::models::session::SessionType;

/// What the daemon does with a relayed request of a given kind from a given
/// session type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixVerdict {
    /// Apply through the normal handler. For a handoff request this writes
    /// the handoff document and, when the request carries `--trigger
    /// ceiling`, also performs the stage transition to `NeedsHandoff` (only
    /// when the stage is `Executing` and owned by the requesting session).
    Apply,
    /// Never apply the requested transition, but still record it — a handoff
    /// document is always written even when the stage transition it can also
    /// trigger is refused for this session type.
    DocumentOnly,
    /// Reject outright.
    Refuse,
}

/// Section 5's table, with one change: a `Merge` session may relay `block`,
/// its way to say it cannot finish the merge. The daemon applies it as a give-up
/// (stops the resolver, routes the stage to human review with the reason), not
/// as the stage block a stage session gets. `BaseConflict` follows `Merge`
/// except that it refuses `block`, `commit`, `check` and `merge-resolved`; for
/// the last, a base-conflict session resolves the *pre-stage* merge, so
/// finalizing an unrelated stage merge from inside one would be a forgery.
///
/// Verification v2 (DESIGN D8) adds the `Contract` session and the
/// `freeze-contracts` kind. `Contract` follows `Stage` except that it may not
/// dispute or record a verdict, and `freeze-contracts` is its alone.
///
/// `file-dispute` (DESIGN D15) disputes a plan v2 stage's findings, contract,
/// integrity events or failing impact tests; only a `Stage` session may relay
/// it.
///
/// `commit` hands the daemon a session's staged index to commit. The three
/// sessions that write a branch relay it: `Stage` (its stage branch),
/// `Knowledge` (the target branch, knowledge prefix only) and `Merge` (the
/// resolved merge). A contract, judge or base-conflict session never commits.
///
/// `check` runs a stage's gate on the host and changes no stage state. A
/// `Merge` session relays it to verify its merge, whose criteria cannot run
/// inside its sandbox; a passing run never lands the merge.
pub fn verdict(session: SessionType, kind: RequestKind) -> MatrixVerdict {
    match kind {
        RequestKind::Memory => policy_memory(session),
        RequestKind::Block => policy_block(session),
        RequestKind::Dispute | RequestKind::Complete => policy_dispute_and_complete(session),
        RequestKind::Handoff => policy_handoff(session),
        RequestKind::MergeResolved => policy_merge_resolved(session),
        RequestKind::Verdict => policy_verdict(session),
        RequestKind::Telemetry => policy_telemetry(session),
        RequestKind::FreezeContracts => policy_freeze_contracts(session),
        RequestKind::FileDispute => policy_file_dispute(session),
        RequestKind::Commit => policy_commit(session),
        RequestKind::Check => policy_check(session),
    }
}

fn policy_memory(session: SessionType) -> MatrixVerdict {
    match session {
        SessionType::Stage
        | SessionType::Knowledge
        | SessionType::Merge
        | SessionType::BaseConflict
        | SessionType::Contract => MatrixVerdict::Apply,
        SessionType::Adjudication => MatrixVerdict::Refuse,
    }
}

fn policy_block(session: SessionType) -> MatrixVerdict {
    match session {
        SessionType::Stage
        | SessionType::Knowledge
        | SessionType::Merge
        | SessionType::Contract => MatrixVerdict::Apply,
        SessionType::BaseConflict | SessionType::Adjudication => MatrixVerdict::Refuse,
    }
}

fn policy_dispute_and_complete(session: SessionType) -> MatrixVerdict {
    match session {
        SessionType::Stage | SessionType::Knowledge => MatrixVerdict::Apply,
        SessionType::Merge
        | SessionType::BaseConflict
        | SessionType::Adjudication
        | SessionType::Contract => MatrixVerdict::Refuse,
    }
}

fn policy_handoff(session: SessionType) -> MatrixVerdict {
    match session {
        SessionType::Stage | SessionType::Knowledge | SessionType::Contract => MatrixVerdict::Apply,
        SessionType::Merge | SessionType::BaseConflict => MatrixVerdict::DocumentOnly,
        SessionType::Adjudication => MatrixVerdict::Refuse,
    }
}

fn policy_merge_resolved(session: SessionType) -> MatrixVerdict {
    match session {
        SessionType::Stage
        | SessionType::Knowledge
        | SessionType::BaseConflict
        | SessionType::Adjudication
        | SessionType::Contract => MatrixVerdict::Refuse,
        SessionType::Merge => MatrixVerdict::Apply,
    }
}

fn policy_verdict(session: SessionType) -> MatrixVerdict {
    match session {
        SessionType::Stage
        | SessionType::Knowledge
        | SessionType::Merge
        | SessionType::BaseConflict
        | SessionType::Contract => MatrixVerdict::Refuse,
        SessionType::Adjudication => MatrixVerdict::Apply,
    }
}

fn policy_telemetry(session: SessionType) -> MatrixVerdict {
    match session {
        SessionType::Stage
        | SessionType::Knowledge
        | SessionType::Merge
        | SessionType::BaseConflict
        | SessionType::Adjudication
        | SessionType::Contract => MatrixVerdict::Apply,
    }
}

fn policy_freeze_contracts(session: SessionType) -> MatrixVerdict {
    match session {
        SessionType::Stage
        | SessionType::Knowledge
        | SessionType::Merge
        | SessionType::BaseConflict
        | SessionType::Adjudication => MatrixVerdict::Refuse,
        SessionType::Contract => MatrixVerdict::Apply,
    }
}

fn policy_file_dispute(session: SessionType) -> MatrixVerdict {
    match session {
        SessionType::Stage => MatrixVerdict::Apply,
        SessionType::Knowledge
        | SessionType::Merge
        | SessionType::BaseConflict
        | SessionType::Adjudication
        | SessionType::Contract => MatrixVerdict::Refuse,
    }
}

fn policy_commit(session: SessionType) -> MatrixVerdict {
    match session {
        SessionType::Stage | SessionType::Knowledge | SessionType::Merge => MatrixVerdict::Apply,
        SessionType::BaseConflict | SessionType::Adjudication | SessionType::Contract => {
            MatrixVerdict::Refuse
        }
    }
}

fn policy_check(session: SessionType) -> MatrixVerdict {
    match session {
        SessionType::Stage
        | SessionType::Knowledge
        | SessionType::Merge
        | SessionType::Adjudication => MatrixVerdict::Apply,
        SessionType::BaseConflict | SessionType::Contract => MatrixVerdict::Refuse,
    }
}

#[cfg(test)]
#[path = "tests_matrix.rs"]
mod tests;
