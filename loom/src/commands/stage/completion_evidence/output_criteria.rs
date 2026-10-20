//! Per-criterion outcomes of a failed `loom stage complete`, read back from
//! the criterion lines `criterion_output::format_criterion_result` prints.
//!
//! The broker runs in a separate process and sees only the tool output, so
//! the printed lines are the one place the outcomes survive. Only exact
//! two-space-indented result lines count; the captured output of a failing
//! command is indented deeper. Plan-authored command text printed on a label
//! line could still imitate a result line, so the outcomes are a guide, not
//! proof, and feed the diagnostic checkpoint only.

use crate::handoff::CriterionResult;
use crate::models::stage::Stage;

const PASSED_PREFIX: &str = "  ✓ passed";
const FAILED_PREFIXES: [&str; 2] = ["  ✗ FAILED [criterion ", "  ✗ TIMEOUT [criterion "];

/// One `acceptance-<i>` result per criterion line in `output` that names an
/// index inside the stage's acceptance list. Criteria that never ran (the
/// runner stopped early) have no line and are omitted.
pub fn criteria_from_output(output: &str, stage: &Stage) -> Vec<CriterionResult> {
    let mut results: Vec<CriterionResult> = Vec::new();
    let mut position = 0usize;
    for line in output.lines() {
        let (index, passed) = if line.starts_with(PASSED_PREFIX) {
            (Some(position), true)
        } else if let Some(index) = failed_index(line) {
            (index, false)
        } else {
            continue;
        };
        // A failure line whose index does not parse still occupies its slot,
        // so the pass lines after it keep their positions.
        position += 1;
        let Some(index) = index else { continue };
        let id = format!("acceptance-{index}");
        if index < stage.acceptance.len() && !results.iter().any(|result| result.id == id) {
            results.push(CriterionResult { id, passed });
        }
    }
    results
}

/// `None` when `line` is not a failure line; `Some(None)` when it is one whose
/// index does not parse.
fn failed_index(line: &str) -> Option<Option<usize>> {
    let rest = FAILED_PREFIXES
        .iter()
        .find_map(|prefix| line.strip_prefix(prefix))?;
    Some(
        rest.split_once(']')
            .and_then(|(index, _)| index.parse().ok()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::AcceptanceCriterion;

    #[test]
    fn unparsable_failure_index_still_advances_the_position() {
        let mut stage = Stage::new("Output stage".to_string(), None);
        stage.acceptance = ["a", "b", "c"]
            .map(|command| AcceptanceCriterion::Simple(command.to_string()))
            .to_vec();
        let output = "  ✓ passed: a\n  ✗ FAILED [criterion x]: b\n  ✓ passed: c\n";

        let ids: Vec<(String, bool)> = criteria_from_output(output, &stage)
            .into_iter()
            .map(|result| (result.id, result.passed))
            .collect();

        assert_eq!(
            ids,
            [
                ("acceptance-0".to_string(), true),
                ("acceptance-2".to_string(), true)
            ]
        );
    }
}
