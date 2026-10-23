//! How a finished freeze job that did not freeze words its refusal.

use std::path::Path;

use super::{store, JobRecord};
use crate::verify::contracts::refusal::run_output_line;

/// Most bytes of the refusal a failed run records.
pub(super) const REFUSAL_MAX_BYTES: usize = 4 * 1024;

/// Each failing outcome's label and summary, loom's own judgement, then the
/// run's error, else the job's detail. The error and the detail are quoted as
/// one run-output line each ([`run_output_line`]): they can carry the child's
/// log and the contract run's output, which the writer's tests print. A result
/// that cannot be read counts as none, so the refusal still carries the
/// detail.
pub(super) fn failure_message(work_dir: &Path, record: &JobRecord) -> String {
    let result =
        store::load_result(work_dir, &record.stage_id, &record.id).unwrap_or_else(|error| {
            tracing::warn!(
                job = %record.id,
                error = %format!("{error:#}"),
                "cannot read a freeze job's result",
            );
            None
        });
    let mut lines: Vec<String> = result
        .iter()
        .flat_map(|result| &result.criteria)
        .filter(|outcome| !outcome.passed)
        .map(|outcome| format!("{}: {}", outcome.label, outcome.summary))
        .collect();
    lines.extend(
        result
            .and_then(|result| result.error)
            .map(|error| run_output_line(&error)),
    );
    if lines.is_empty() {
        let line = match record.detail.as_deref() {
            Some(detail) => run_output_line(detail),
            None => "the contract run failed".to_string(),
        };
        lines.push(line);
    }
    lines.join("\n")
}
