//! Plan initialization and stage creation for loom init.

use crate::fs::stage_files::{compute_stage_depths, stage_file_path, StageDependencies};
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{Stage, StageStatus};
use crate::plan::parser::parse_plan;
use crate::plan::schema::StageDefinition;
use crate::verify::serialize_stage_to_markdown;
use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Get the current git branch name
fn get_current_branch() -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("Failed to execute git rev-parse")?;

    if !output.status.success() {
        anyhow::bail!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Initialize with a plan file
/// Returns the number of stages created
pub fn initialize_with_plan(work_dir: &WorkDir, plan_path: &Path) -> Result<usize> {
    if !plan_path.exists() {
        anyhow::bail!("Plan file does not exist: {}", plan_path.display());
    }

    let parsed_plan = parse_plan(plan_path)
        .with_context(|| format!("Failed to parse plan file: {}", plan_path.display()))?;

    println!(
        "  {} Plan parsed: {}",
        "✓".green().bold(),
        parsed_plan.name.bold()
    );

    let base_branch = get_current_branch().context("Failed to get current git branch")?;

    let config_content = format!(
        "# loom Configuration\n# Generated from plan: {}\n\n[plan]\nsource_path = \"{}\"\nplan_id = \"{}\"\nplan_name = \"{}\"\nbase_branch = \"{}\"\n",
        plan_path.display(),
        plan_path.display(),
        parsed_plan.id,
        parsed_plan.name,
        base_branch
    );

    let config_path = work_dir.root().join("config.toml");
    fs::write(&config_path, config_content).context("Failed to write config.toml")?;
    println!(
        "  {} Config saved {}",
        "✓".green().bold(),
        "config.toml".dimmed()
    );

    let stage_deps: Vec<StageDependencies> = parsed_plan
        .stages
        .iter()
        .map(|s| StageDependencies {
            id: s.id.clone(),
            dependencies: s.dependencies.clone(),
        })
        .collect();

    let depths = compute_stage_depths(&stage_deps).context("Failed to compute stage depths")?;

    let stages_dir = work_dir.root().join("stages");
    if !stages_dir.exists() {
        fs::create_dir_all(&stages_dir).context("Failed to create stages directory")?;
    }

    let stage_count = parsed_plan.stages.len();
    println!(
        "\n{} {}",
        "Stages".bold(),
        format!("({stage_count})").dimmed()
    );
    println!("{}", "─".repeat(40).dimmed());

    let max_id_len = parsed_plan
        .stages
        .iter()
        .map(|s| s.id.len())
        .max()
        .unwrap_or(0);

    for stage_def in &parsed_plan.stages {
        let stage = create_stage_from_definition(stage_def, &parsed_plan.id);
        let depth = depths.get(&stage.id).copied().unwrap_or(0);
        let stage_path = stage_file_path(&stages_dir, depth, &stage.id);

        let content = serialize_stage_to_markdown(&stage)
            .with_context(|| format!("Failed to serialize stage: {}", stage.id))?;

        fs::write(&stage_path, content)
            .with_context(|| format!("Failed to write stage file: {}", stage_path.display()))?;

        let status_indicator = if stage_def.dependencies.is_empty() {
            "●".green()
        } else {
            "○".yellow()
        };

        println!(
            "  {}  {:width$}  {}",
            status_indicator,
            stage.id.dimmed(),
            stage.name,
            width = max_id_len
        );
    }

    Ok(stage_count)
}

/// Create a Stage from a StageDefinition
pub(crate) fn create_stage_from_definition(stage_def: &StageDefinition, plan_id: &str) -> Stage {
    let now = Utc::now();

    let status = if stage_def.dependencies.is_empty() {
        StageStatus::Queued
    } else {
        StageStatus::WaitingForDeps
    };

    Stage {
        id: stage_def.id.clone(),
        name: stage_def.name.clone(),
        description: stage_def.description.clone(),
        status,
        dependencies: stage_def.dependencies.clone(),
        parallel_group: stage_def.parallel_group.clone(),
        acceptance: stage_def.acceptance.clone(),
        setup: stage_def.setup.clone(),
        files: stage_def.files.clone(),
        plan_id: Some(plan_id.to_string()),
        worktree: None,
        session: None,
        held: false,
        parent_stage: None,
        child_stages: Vec::new(),
        created_at: now,
        updated_at: now,
        completed_at: None,
        close_reason: None,
        auto_merge: stage_def.auto_merge,
        retry_count: 0,
        max_retries: None,
        last_failure_at: None,
        failure_info: None,
        resolved_base: None,
        base_branch: None,
        base_merged_from: Vec::new(),
        outputs: Vec::new(),
        completed_commit: None,
        merged: false,
        merge_conflict: false,
    }
}
