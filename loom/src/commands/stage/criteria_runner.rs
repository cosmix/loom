//! Acceptance criteria reload and management
//!
//! This module handles reloading acceptance criteria from plan files
//! when retrying or verifying stages.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::models::stage::Stage;
use crate::plan::parser::parse_plan;

/// Reload acceptance criteria from the plan file.
///
/// Reads config.toml to find the plan source path, parses the plan,
/// finds the stage definition, and updates stage.acceptance, stage.working_dir,
/// and stage.setup from the plan.
pub fn reload_acceptance_from_plan(stage: &mut Stage, work_dir: &Path) -> Result<()> {
    let config_path = work_dir.join("config.toml");

    if !config_path.exists() {
        bail!("No config.toml found in .work/. Cannot reload acceptance criteria.");
    }

    let config_content =
        std::fs::read_to_string(&config_path).context("Failed to read config.toml")?;

    let config: toml::Value =
        toml::from_str(&config_content).context("Failed to parse config.toml")?;

    let source_path = config
        .get("plan")
        .and_then(|p| p.get("source_path"))
        .and_then(|s| s.as_str())
        .ok_or_else(|| anyhow::anyhow!("No 'plan.source_path' found in config.toml"))?;

    let plan_path = PathBuf::from(source_path);

    if !plan_path.exists() {
        bail!(
            "Plan file not found: {}\nCannot reload acceptance criteria.",
            plan_path.display()
        );
    }

    let parsed_plan = parse_plan(&plan_path)
        .with_context(|| format!("Failed to parse plan: {}", plan_path.display()))?;

    // Find the stage definition in the plan
    let stage_def = parsed_plan
        .stages
        .iter()
        .find(|s| s.id == stage.id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Stage '{}' not found in plan file: {}",
                stage.id,
                plan_path.display()
            )
        })?;

    // Track what was updated for logging
    let mut updates = Vec::new();

    // Update acceptance criteria
    if stage.acceptance != stage_def.acceptance {
        updates.push(format!(
            "acceptance: {} -> {} criteria",
            stage.acceptance.len(),
            stage_def.acceptance.len()
        ));
        stage.acceptance = stage_def.acceptance.clone();
    }

    // Update working_dir
    let new_working_dir = Some(stage_def.working_dir.clone());
    if stage.working_dir != new_working_dir {
        updates.push(format!(
            "working_dir: {:?} -> {:?}",
            stage.working_dir, new_working_dir
        ));
        stage.working_dir = new_working_dir;
    }

    // Update setup commands
    if stage.setup != stage_def.setup {
        updates.push(format!(
            "setup: {} -> {} commands",
            stage.setup.len(),
            stage_def.setup.len()
        ));
        stage.setup = stage_def.setup.clone();
    }

    if updates.is_empty() {
        println!("Acceptance criteria already up to date with plan.");
    } else {
        println!("Reloaded from plan file:");
        for update in updates {
            println!("  - {update}");
        }
    }

    Ok(())
}
