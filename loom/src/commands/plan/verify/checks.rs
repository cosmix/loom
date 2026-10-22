//! Checks `loom plan verify` runs once the plan schema validates.

use std::path::Path;

use super::{build_levels_output, JsonError, JsonStageLevel, JsonWarnings};
use crate::plan::graph::levels::compute_all_levels;
use crate::plan::schema::{
    check_knowledge_recommendations, check_sandbox_recommendations, detect_stage_type,
    split_lint_findings, v2_lints, validate_structural_preflight, LoomMetadata, StageDefinition,
};

/// Sandbox, lint and DAG hard errors go to `hard_errors`, skipped-check notes
/// to `notes`; returns the soft warnings and the execution levels.
pub(super) fn soft_checks(
    metadata: &LoomMetadata,
    repo_root: Option<&Path>,
    hard_errors: &mut Vec<JsonError>,
    notes: &mut Vec<String>,
) -> (JsonWarnings, Vec<Vec<JsonStageLevel>>) {
    let stages = &metadata.loom.stages;
    hard_errors.extend(sandbox_policy_errors(metadata));
    let context = v2_lints::LintContext {
        metadata,
        repo_root,
    };
    let lints = v2_lints::run(&context, notes);
    let (lint_errs, lint_warns) = split_lint_findings(metadata.loom.version, lints);
    hard_errors.extend(lint_errs.into_iter().map(JsonError::from));
    let mut warnings = JsonWarnings {
        structural: validate_structural_preflight(stages, repo_root),
        knowledge: check_knowledge_recommendations(stages, repo_root),
        sandbox: check_sandbox_recommendations(metadata),
        baseline: Vec::new(),
    };
    warnings.structural.extend(lint_warns);
    let mut levels = Vec::new();
    match dag_levels(stages) {
        Ok(by_level) => levels = by_level,
        Err(error) => hard_errors.push(error),
    }
    (warnings, levels)
}

/// Sandbox policy hard errors: run the SAME merge + validation that
/// `loom init` and stage spawn use (crate::sandbox::merge_config /
/// validate_config / validate_emittable), so a plan `loom init` would refuse
/// is not reported clean here. These are errors, not warnings — init already
/// rejects these plans outright.
fn sandbox_policy_errors(metadata: &LoomMetadata) -> Vec<JsonError> {
    use crate::plan::schema::stage_host_path_errors;
    let mut errors = Vec::new();
    for stage in &metadata.loom.stages {
        let merged = crate::sandbox::merge_config(
            &metadata.loom.sandbox,
            &stage.sandbox,
            detect_stage_type(stage),
            &stage.implementers,
        );
        let mut messages = stage_host_path_errors(stage, &merged.filesystem.allow_write);
        if let Err(e) = crate::sandbox::validate_config(&merged) {
            messages.push(e.to_string());
        }
        if let Err(e) = crate::sandbox::validate_emittable(&merged) {
            messages.push(e.to_string());
        }
        errors.extend(messages.into_iter().map(|message| JsonError {
            stage_id: Some(stage.id.clone()),
            message,
        }));
    }
    errors
}

/// DAG cycle detection, then the stages grouped by execution level.
fn dag_levels(stages: &[StageDefinition]) -> Result<Vec<Vec<JsonStageLevel>>, JsonError> {
    if let Err(e) = crate::plan::graph::ExecutionGraph::build(stages.to_vec()) {
        return Err(JsonError {
            stage_id: None,
            message: e.to_string(),
        });
    }
    let levels_map = compute_all_levels(stages, |s| s.id.as_str(), |s| &s.dependencies);
    Ok(build_levels_output(stages, &levels_map))
}
