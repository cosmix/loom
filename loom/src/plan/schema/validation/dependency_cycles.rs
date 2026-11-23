use super::super::types::{StageDefinition, ValidationError};
use crate::plan::graph::ExecutionGraph;

/// Check cycles at the end of schema validation, once all dependencies are known.
/// Plan verification, init preflight, the run loader and the planner share this
/// validation path.
pub(super) fn finish(
    stages: &[StageDefinition],
    errors: Vec<ValidationError>,
) -> Result<(), Vec<ValidationError>> {
    if !errors.is_empty() {
        return Err(errors);
    }

    ExecutionGraph::build(stages.to_vec())
        .map(|_| ())
        .map_err(|error| {
            vec![ValidationError {
                message: error.to_string(),
                stage_id: None,
            }]
        })
}
