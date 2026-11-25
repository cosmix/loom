//! The indexed outcomes a failed completion records and the dispute routes the
//! block guard (`control_block.rs`) names for them: `acceptance-<i>` criteria
//! and the goal-backward gaps `artifact-<i>`, `wiring-<i>` and
//! `wiring_tests-<i>`.

use super::DisputableFailures;
use crate::handoff::schema::CriterionResult;
use crate::models::dispute::CriterionField;

/// The indices `n` of the failed `<prefix>-<n>` outcomes of `results`, sorted
/// and without duplicates.
pub(super) fn failed_indices(results: &[CriterionResult], prefix: &str) -> Vec<usize> {
    let mut indices: Vec<usize> = results
        .iter()
        .filter(|result| !result.passed)
        .filter_map(|result| {
            result
                .id
                .strip_prefix(prefix)?
                .strip_prefix('-')?
                .parse()
                .ok()
        })
        .collect();
    indices.sort_unstable();
    indices.dedup();
    indices
}

/// The route for failing `acceptance-<i>` criteria.
pub(super) fn criteria_route(stage_id: &str, criteria: &[usize]) -> String {
    let flags: Vec<String> = criteria
        .iter()
        .map(|index| format!("--criterion-index {index}"))
        .collect();
    format!(
        "Failing criteria: `loom stage dispute-criteria {stage_id} {} --reason \"...\"` names \
         every failing criterion in one filing.",
        flags.join(" ")
    )
}

/// One route per goal-backward list that has a failing gap, each naming the
/// `--field` and the failing indices.
pub(super) fn gap_routes(stage_id: &str, failed: &DisputableFailures) -> Vec<String> {
    [
        (CriterionField::Artifacts, &failed.artifacts),
        (CriterionField::Wiring, &failed.wiring),
        (CriterionField::WiringTests, &failed.wiring_tests),
    ]
    .into_iter()
    .filter(|(_, indices)| !indices.is_empty())
    .map(|(field, indices)| {
        let flags: Vec<String> = indices
            .iter()
            .map(|index| format!("--criterion-index {index}"))
            .collect();
        format!(
            "Failing `[{} n]` gaps: `loom stage dispute-criteria {stage_id} --field {} {} \
             --reason \"...\"` names every failing one in one filing.",
            field.gap_label(),
            field.as_str(),
            flags.join(" ")
        )
    })
    .collect()
}
