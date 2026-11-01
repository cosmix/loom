//! Load one goal definition from the active plan, then run its checks.

use std::path::Path;

use anyhow::{Context, Result};

use super::{run_goal_backward_verification, GoalBackwardResult};
use crate::models::stage::{CommandConfinement, TruthCheck};
use crate::plan::parser::parse_plan;
use crate::plan::schema::StageDefinition;
use crate::verify::criteria::resolve_confinement;
use crate::verify::transitions::load_stage;

pub(crate) struct LoadedGoal {
    definition: StageDefinition,
    confinement: CommandConfinement,
}

impl LoadedGoal {
    pub(crate) fn has_checks(&self) -> bool {
        self.definition.has_any_goal_checks()
    }

    pub(crate) fn after_stage(&self) -> &[TruthCheck] {
        &self.definition.after_stage
    }

    pub(crate) fn run(
        &self,
        verification_dir: &Path,
        tree: Option<&Path>,
        plan_version: u32,
    ) -> Result<GoalBackwardResult> {
        run_goal_backward_verification(
            &self.definition,
            verification_dir,
            tree,
            self.confinement,
            plan_version,
        )
    }
}

pub(crate) fn run_from_plan(
    stage_id: &str,
    verification_dir: &Path,
    tree: Option<&Path>,
    work_dir: &Path,
    plan_version: u32,
    expected_policy: Option<&str>,
) -> Result<GoalBackwardResult> {
    load(stage_id, work_dir, expected_policy)?.run(verification_dir, tree, plan_version)
}

pub(crate) fn load(
    stage_id: &str,
    work_dir: &Path,
    expected_policy: Option<&str>,
) -> Result<LoadedGoal> {
    let plan_path = crate::fs::resolve_source_path(work_dir)?
        .context("No plan source path configured in config.toml")?;
    load_at_path(stage_id, work_dir, expected_policy, &plan_path)?
        .with_context(|| format!("Stage '{stage_id}' not found in plan"))
}

pub(crate) fn load_optional(
    stage_id: &str,
    work_dir: &Path,
    expected_policy: Option<&str>,
) -> Result<Option<LoadedGoal>> {
    let Some(plan_path) = crate::fs::resolve_source_path(work_dir)? else {
        return Ok(None);
    };
    load_at_path(stage_id, work_dir, expected_policy, &plan_path)
}

fn load_at_path(
    stage_id: &str,
    work_dir: &Path,
    expected_policy: Option<&str>,
    plan_path: &Path,
) -> Result<Option<LoadedGoal>> {
    let plan = parse_plan(plan_path)
        .with_context(|| format!("Failed to parse plan: {}", plan_path.display()))?;
    let Some(stage_def) = plan.stages.iter().find(|stage| stage.id == stage_id) else {
        return Ok(None);
    };
    if let Some(expected) = expected_policy {
        let stage = load_stage(stage_id, work_dir)?;
        let actual = crate::handoff::completion::policy::effective_policy_hash(&stage, work_dir)?;
        anyhow::ensure!(
            expected == actual,
            "verification policy changed before goal checks"
        );
    }
    let confinement = resolve_confinement(
        stage_def.sandbox.command_confinement,
        Some(plan.metadata.loom.sandbox.command_confinement),
    );
    Ok(Some(LoadedGoal {
        definition: stage_def.clone(),
        confinement,
    }))
}
