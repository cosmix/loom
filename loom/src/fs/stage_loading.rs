//! Shared utilities for loading stage definitions from .work/stages/ files.
//!
//! On-disk `.work/stages/*.md` files carry a full serialized
//! [`Stage`] in their YAML frontmatter (written by
//! `serialize_stage_to_markdown`), including runtime-only keys `StageDefinition`
//! does not declare (`status`, `created_at`, `merged`, `fix_attempts`,
//! `resolved_base`, `session`, `worktree`, …). `StageDefinition` is
//! `#[serde(deny_unknown_fields)]`, so it cannot deserialize that frontmatter
//! directly — we parse it as a `Stage` instead and project it down to a
//! `StageDefinition` via `definition_from_stage`, the exact inverse of
//! [`Stage::from_definition`](crate::models::stage::Stage::from_definition).
//! `StageDefinition` keeps `deny_unknown_fields` for parsing *plan* YAML, where
//! an unknown key is almost always a typo the plan author should see at
//! `loom plan verify` time — that guarantee is untouched by this module. This
//! is deliberately NOT a partial hand-rolled struct: an intermediate struct
//! previously dropped `stage_type`, `auto_merge`, `sandbox`, `context_budget`,
//! and `before_stage`/`after_stage` on every daemon restart (the loader prefers
//! stage files over the plan).

use anyhow::{Context, Result};
use std::path::Path;

use crate::models::stage::Stage;
use crate::parser::frontmatter::parse_from_markdown;
use crate::plan::schema::StageDefinition;
use crate::validation::validate_id;

/// Deserialize a [`StageDefinition`] from a stage markdown file's YAML
/// frontmatter.
///
/// Stage files hold a full serialized [`Stage`], not a `StageDefinition`, so
/// the frontmatter is parsed as a `Stage` first and then projected down via
/// `definition_from_stage`.
pub fn extract_stage_definition(content: &str) -> Result<StageDefinition> {
    let stage: Stage = parse_from_markdown(content, "Stage")?;
    Ok(definition_from_stage(&stage))
}

/// Project a runtime [`Stage`] down to the plan-level fields
/// [`StageDefinition`] carries.
///
/// This is the exact inverse of
/// [`Stage::from_definition`](crate::models::stage::Stage::from_definition) —
/// kept field-for-field in sync with it so the two cannot silently diverge as
/// policy fields are added to either type.
fn definition_from_stage(stage: &Stage) -> StageDefinition {
    StageDefinition {
        id: stage.id.clone(),
        name: stage.name.clone(),
        description: stage.description.clone(),
        dependencies: stage.dependencies.clone(),
        parallel_group: stage.parallel_group.clone(),
        acceptance: stage.acceptance.clone(),
        setup: stage.setup.clone(),
        files: stage.files.clone(),
        auto_merge: stage.auto_merge,
        // `Stage::working_dir` is `Option<String>` (unset means "not yet
        // resolved"); `StageDefinition::working_dir` is a required `String`
        // that defaults to the worktree root when the plan omitted it.
        working_dir: stage.working_dir.clone().unwrap_or_else(|| ".".to_string()),
        // `Stage::stage_type` is resolved (non-optional) by `detect_stage_type`
        // at `from_definition` time; wrap it back into the plan's `Option` so it
        // reads as the plan author's explicit, final answer.
        stage_type: Some(stage.stage_type),
        artifacts: stage.artifacts.clone(),
        wiring: stage.wiring.clone(),
        wiring_tests: stage.wiring_tests.clone(),
        dead_code_check: stage.dead_code_check.clone(),
        before_stage: stage.before_stage.clone(),
        after_stage: stage.after_stage.clone(),
        context_budget: stage.context_budget,
        sandbox: stage.sandbox.clone(),
        execution_mode: stage.execution_mode,
        bug_fix: stage.bug_fix,
        regression_test: stage.regression_test.clone(),
        model: stage.model.clone(),
        reasoning_effort: stage.reasoning_effort.clone(),
        code_review: stage.code_review.clone(),
        ultracode: stage.ultracode,
        implementers: stage.implementers.clone(),
        subagent_timeout_secs: stage.subagent_timeout_secs,
    }
}

/// Load stage definitions from .work/stages/ directory
pub fn load_stages_from_work_dir(stages_dir: &Path) -> Result<Vec<StageDefinition>> {
    let mut stages = Vec::new();

    for entry in std::fs::read_dir(stages_dir)
        .with_context(|| format!("Failed to read stages directory: {}", stages_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        // Skip non-markdown files
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        // Read and parse the stage file
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read stage file: {}", path.display()))?;

        // Parse the file's full Stage frontmatter and project it down to a
        // StageDefinition (lossless for every field StageDefinition carries —
        // stage_type, auto_merge, sandbox, context_budget, before/after_stage
        // all survive).
        let stage_def = match extract_stage_definition(&content) {
            Ok(def) => {
                // Validate the stage ID before using it
                if let Err(e) = validate_id(&def.id) {
                    eprintln!("Warning: Invalid stage ID in {}: {}", path.display(), e);
                    continue;
                }
                def
            }
            Err(e) => {
                eprintln!("Warning: Could not parse {}: {}", path.display(), e);
                continue;
            }
        };

        stages.push(stage_def);
    }

    Ok(stages)
}

#[cfg(test)]
#[path = "tests_stage_loading.rs"]
mod tests;

#[cfg(test)]
#[path = "tests_stage_loading_fields.rs"]
mod tests_fields;

#[cfg(test)]
#[path = "tests_stage_loading_round_trip.rs"]
mod tests_round_trip;
