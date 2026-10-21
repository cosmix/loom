//! Matrix rows for the host verification kinds, kept apart so `matrix.rs`
//! stays under the file-size limit.

use super::kind::RequestKind;
use super::matrix::MatrixVerdict;
use crate::models::session::SessionType;

/// `check` runs the stage's gate on the host for the sessions that verify:
/// `Stage` (and its `loom-verifier` subagents), `Knowledge`, and the judge
/// (`Adjudication`, whose first step is `loom check <stage> --no-cache`).
/// Merge, base-conflict and contract sessions never request one.
pub(super) const HOST_JOB_MATRIX: &[(SessionType, RequestKind, MatrixVerdict)] = &[
    (SessionType::Stage, RequestKind::Check, MatrixVerdict::Apply),
    (
        SessionType::Knowledge,
        RequestKind::Check,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Adjudication,
        RequestKind::Check,
        MatrixVerdict::Apply,
    ),
    (
        SessionType::Merge,
        RequestKind::Check,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::BaseConflict,
        RequestKind::Check,
        MatrixVerdict::Refuse,
    ),
    (
        SessionType::Contract,
        RequestKind::Check,
        MatrixVerdict::Refuse,
    ),
];
