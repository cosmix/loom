//! The impact-test record of a failed completion (DESIGN D14, D15): how the
//! failures the impact gate observed reach the stage's observed record
//! (`verify::impact_tests::observed`), which a dispute of them names. Every
//! failing completion replaces the record, so a dispute names only failures
//! of the latest run. The operator's completion records it here; a session's
//! completion runs as a host job, whose child records it
//! (`host_verify::child_complete`).

use std::path::Path;

use crate::verify::impact_tests::{record_observed, ImpactTestsFailed, ObservedImpact};

/// The impact-test failures a completion that failed with `error` observed:
/// none unless the impact gate itself failed it.
pub(crate) fn observed_impact(error: &anyhow::Error) -> ObservedImpact {
    let failures = error
        .downcast_ref::<ImpactTestsFailed>()
        .map(|failed| failed.failures.as_slice())
        .unwrap_or_default();
    ObservedImpact::of(failures)
}

/// After the operator's completion of `stage_id` failed with `error`, record
/// what the impact gate observed. A record that cannot be kept is a warning;
/// the completion's own error stands.
pub(super) fn after_failed_completion(stage_id: &str, work_dir: &Path, error: &anyhow::Error) {
    if let Err(error) = record_observed(work_dir, stage_id, &observed_impact(error)) {
        eprintln!("Warning: the impact-test failures were not recorded: {error:#}");
    }
}
