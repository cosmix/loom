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

/// Section 5's table, verbatim. `BaseConflict` follows `Merge` except for
/// `merge-resolved`, which it refuses: a base-conflict session resolves the
/// *pre-stage* merge, so finalizing an unrelated stage merge from inside one
/// would be a forgery.
///
/// Verification v2 (DESIGN D8) adds the `Contract` session and the
/// `freeze-contracts` kind. `Contract` follows `Stage` except that it may not
/// dispute or record a verdict, and `freeze-contracts` is its alone.
const MATRIX: &[(SessionType, RequestKind, MatrixVerdict)] = &[
    (
        SessionType::Stage,
        RequestKind::Memory,
        MatrixVerdict::Apply,
    ),
    (SessionType::Stage, RequestKind::Block, MatrixVerdict::Apply),
    (
        SessionType::Stage,
        RequestKind::Dispute,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Stage,
        RequestKind::Handoff,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Stage,
        RequestKind::MergeResolved,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Stage,
        RequestKind::Verdict,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Stage,
        RequestKind::Telemetry,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Knowledge,
        RequestKind::Memory,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Knowledge,
        RequestKind::Block,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Knowledge,
        RequestKind::Dispute,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Knowledge,
        RequestKind::Handoff,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Knowledge,
        RequestKind::MergeResolved,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Knowledge,
        RequestKind::Verdict,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Knowledge,
        RequestKind::Telemetry,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Merge,
        RequestKind::Memory,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Merge,
        RequestKind::Block,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Merge,
        RequestKind::Dispute,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Merge,
        RequestKind::Handoff,
        MatrixVerdict::DocumentOnly,
    ),
    (
        SessionType::Merge,
        RequestKind::MergeResolved,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Merge,
        RequestKind::Verdict,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Merge,
        RequestKind::Telemetry,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::BaseConflict,
        RequestKind::Memory,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::BaseConflict,
        RequestKind::Block,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::BaseConflict,
        RequestKind::Dispute,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::BaseConflict,
        RequestKind::Handoff,
        MatrixVerdict::DocumentOnly,
    ),
    (
        SessionType::BaseConflict,
        RequestKind::MergeResolved,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::BaseConflict,
        RequestKind::Verdict,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::BaseConflict,
        RequestKind::Telemetry,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Adjudication,
        RequestKind::Memory,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Adjudication,
        RequestKind::Block,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Adjudication,
        RequestKind::Dispute,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Adjudication,
        RequestKind::Handoff,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Adjudication,
        RequestKind::MergeResolved,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Adjudication,
        RequestKind::Verdict,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Adjudication,
        RequestKind::Telemetry,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Stage,
        RequestKind::FreezeContracts,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Knowledge,
        RequestKind::FreezeContracts,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Merge,
        RequestKind::FreezeContracts,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::BaseConflict,
        RequestKind::FreezeContracts,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Adjudication,
        RequestKind::FreezeContracts,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Contract,
        RequestKind::Memory,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Contract,
        RequestKind::Block,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Contract,
        RequestKind::Dispute,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Contract,
        RequestKind::Handoff,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Contract,
        RequestKind::MergeResolved,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Contract,
        RequestKind::Verdict,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Contract,
        RequestKind::Telemetry,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Contract,
        RequestKind::FreezeContracts,
        MatrixVerdict::Apply,
    ),
];

/// Look up the matrix verdict for one (session type, request kind) pair.
pub fn verdict(session: SessionType, kind: RequestKind) -> MatrixVerdict {
    MATRIX
        .iter()
        .find(|(s, k, _)| *s == session && *k == kind)
        .map(|(_, _, verdict)| *verdict)
        .expect("MATRIX covers every SessionType x RequestKind pair")
}

#[cfg(test)]
#[path = "tests_matrix.rs"]
mod tests;
