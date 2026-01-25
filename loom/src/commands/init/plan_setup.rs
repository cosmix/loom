//! Plan initialization and stage creation for loom init.

use crate::fs::stage_files::{compute_stage_depths, stage_file_path, StageDependencies};
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::plan::parser::parse_plan;
use crate::plan::schema::{check_knowledge_recommendations, StageDefinition};
use crate::verify::serialize_stage_to_markdown;
use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use serde::Serialize;
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

/// Configuration file structure for type-safe TOML serialization.
/// Using serde ensures proper escaping of all string fields.
#[derive(Serialize)]
struct Config {
    plan: PlanConfig,
}

#[derive(Serialize)]
struct PlanConfig {
    source_path: String,
    plan_id: String,
    plan_name: String,
    base_branch: String,
}

/// Initialize with a plan file
/// Returns the number of stages created
pub fn initialize_with_plan(work_dir: &WorkDir, plan_path: &Path) -> Result<usize> {
    if !plan_path.exists() {
        anyhow::bail!("Plan file does not exist: {}", plan_path.display());
    }

    // Canonicalize the plan path to resolve symlinks and relative paths
    let canonical_path = plan_path
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize plan path: {}", plan_path.display()))?;

    let parsed_plan = parse_plan(&canonical_path)
        .with_context(|| format!("Failed to parse plan file: {}", canonical_path.display()))?;

    println!(
        "  {} Plan parsed: {}",
        "✓".green().bold(),
        parsed_plan.name.bold()
    );

    // Check for knowledge-related recommendations (non-fatal warnings)
    let warnings = check_knowledge_recommendations(&parsed_plan.stages);
    for warning in &warnings {
        println!("  {} {}", "⚠".yellow().bold(), warning.yellow());
    }

    let base_branch = get_current_branch().context("Failed to get current git branch")?;

    // Build config using serde serialization for proper TOML escaping
    // This prevents injection attacks via malicious plan names/paths
    let config = Config {
        plan: PlanConfig {
            source_path: canonical_path.display().to_string(),
            plan_id: parsed_plan.id.clone(),
            plan_name: parsed_plan.name.clone(),
            base_branch,
        },
    };

    let config_content = format!(
        "# loom Configuration\n# Generated from plan: {}\n\n{}",
        canonical_path.display(),
        toml::to_string_pretty(&config).context("Failed to serialize config to TOML")?
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

/// Detect the stage type from the definition.
///
/// Uses explicit `stage_type` field if set to Knowledge, otherwise
/// falls back to detecting "knowledge" in ID or name (case-insensitive).
fn detect_stage_type(stage_def: &StageDefinition) -> StageType {
    // Check explicit stage_type field first
    if stage_def.stage_type == StageType::Knowledge {
        return StageType::Knowledge;
    }

    // Fallback: detect based on ID or name containing "knowledge"
    if stage_def.id.to_lowercase().contains("knowledge")
        || stage_def.name.to_lowercase().contains("knowledge")
    {
        return StageType::Knowledge;
    }

    StageType::Standard
}

/// Create a Stage from a StageDefinition
pub(crate) fn create_stage_from_definition(stage_def: &StageDefinition, plan_id: &str) -> Stage {
    let now = Utc::now();

    let status = if stage_def.dependencies.is_empty() {
        StageStatus::Queued
    } else {
        StageStatus::WaitingForDeps
    };

    let stage_type = detect_stage_type(stage_def);

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
        stage_type,
        plan_id: Some(plan_id.to_string()),
        worktree: None,
        session: None,
        held: false,
        parent_stage: None,
        child_stages: Vec::new(),
        created_at: now,
        updated_at: now,
        completed_at: None,
        started_at: None,
        duration_secs: None,
        close_reason: None,
        auto_merge: stage_def.auto_merge,
        working_dir: Some(stage_def.working_dir.clone()),
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
        verification_status: Default::default(),
        context_budget: stage_def.context_budget,
    }
}
