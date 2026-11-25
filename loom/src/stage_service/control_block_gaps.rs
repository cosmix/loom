//! The indexed outcomes a failed completion records and the dispute routes the
//! block guard (`control_block.rs`) names for them: `acceptance-<i>` criteria
//! and the goal-backward gaps `artifact-<i>`, `wiring-<i>` and
//! `wiring_tests-<i>`.

use super::DisputableFailures;
use crate::handoff::schema::CriterionResult;
use crate::models::dispute::CriterionField;
use crate::models::stage::Stage;

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

/// The failing `artifact-<i>`, `wiring-<i>` and `wiring_tests-<i>` indices of
/// `results`, in that order, that name an entry `stage` still has. A gap id is
/// read from a message the plan's own text can reach, so an index past the end
/// of its list is not a gap a dispute can file.
pub(super) fn failed_gaps(
    stage: &Stage,
    results: &[CriterionResult],
) -> (Vec<usize>, Vec<usize>, Vec<usize>) {
    let within = |prefix: &str, len: usize| {
        let mut indices = failed_indices(results, prefix);
        indices.retain(|index| *index < len);
        indices
    };
    (
        within("artifact", stage.artifacts.len()),
        within("wiring", stage.wiring.len()),
        within("wiring_tests", stage.wiring_tests.len()),
    )
}

/// `--criterion-index <i>` for each of `indices`, space-separated.
fn criterion_index_flags(indices: &[usize]) -> String {
    let flags: Vec<String> = indices
        .iter()
        .map(|index| format!("--criterion-index {index}"))
        .collect();
    flags.join(" ")
}

/// The route for failing `acceptance-<i>` criteria.
pub(super) fn criteria_route(stage_id: &str, criteria: &[usize]) -> String {
    format!(
        "Failing criteria: `loom stage dispute-criteria {stage_id} {} --reason \"...\"` names \
         every failing criterion in one filing.",
        criterion_index_flags(criteria)
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
        format!(
            "Failing `[{} n]` gaps: `loom stage dispute-criteria {stage_id} --field {} {} \
             --reason \"...\"` names every failing one in one filing.",
            field.gap_label(),
            field.as_str(),
            criterion_index_flags(indices)
        )
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(id: &str) -> CriterionResult {
        CriterionResult {
            id: id.to_string(),
            passed: false,
        }
    }

    #[test]
    fn a_gap_past_the_end_of_its_list_is_not_disputable() {
        let stage = Stage {
            artifacts: vec!["a.md".into(), "b.md".into()],
            ..Stage::default()
        };
        let results = [
            failed("artifact-1"),
            failed("artifact-2"),
            failed("wiring-0"),
            failed("wiring_tests-0"),
        ];

        let (artifacts, wiring, wiring_tests) = failed_gaps(&stage, &results);

        assert_eq!(artifacts, vec![1]);
        assert!(wiring.is_empty());
        assert!(wiring_tests.is_empty());
    }
}

#[cfg(test)]
#[path = "control_block_gap_route_tests.rs"]
mod route_tests;
