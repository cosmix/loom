//! Plan initialization and stage creation for loom init.

use crate::fs::stage_files::{compute_stage_depths, stage_file_path, StageDependencies};
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::plan::parser::parse_plan;
use crate::plan::schema::{
    check_code_review_recommendations, check_knowledge_recommendations,
    check_sandbox_recommendations, StageDefinition, StageSandboxConfig,
};
use crate::verify::serialize_stage_to_markdown;
use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use serde::Serialize;
use std::fs;
use std::path::Path;

use crate::git::runner::run_git_checked;

/// Get the current git branch name
fn get_current_branch() -> Result<String> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    run_git_checked(&["rev-parse", "--abbrev-ref", "HEAD"], &cwd)
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

    // Check for sandbox-related recommendations (non-fatal warnings)
    let sandbox_warnings = check_sandbox_recommendations(&parsed_plan.metadata);
    for warning in &sandbox_warnings {
        println!("  {} {}", "⚠".yellow().bold(), warning.yellow());
    }

    // Auto-insert code-review stage before integration-verify if not present
    let mut stages = parsed_plan.stages.clone();
    let stages = auto_insert_code_review_stage(&mut stages);

    // Check for code-review-related recommendations (non-fatal warnings)
    let code_review_warnings = check_code_review_recommendations(&stages);
    for warning in &code_review_warnings {
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

    let stage_deps: Vec<StageDependencies> = stages
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

    let stage_count = stages.len();
    println!(
        "\n{} {}",
        "Stages".bold(),
        format!("({stage_count})").dimmed()
    );
    println!("{}", "─".repeat(40).dimmed());

    let max_id_len = stages.iter().map(|s| s.id.len()).max().unwrap_or(0);

    for stage_def in &stages {
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

/// Auto-insert a code-review stage before integration-verify if not present.
///
/// Returns the (potentially modified) list of stages.
fn auto_insert_code_review_stage(stages: &mut Vec<StageDefinition>) -> Vec<StageDefinition> {
    // Check if there's an integration-verify stage
    let integration_verify_idx = stages.iter().position(|s| {
        s.stage_type == StageType::IntegrationVerify
            || s.id.to_lowercase().contains("integration-verify")
            || s.name.to_lowercase().contains("integration verify")
    });

    // Check if there's already a code-review stage
    let has_code_review = stages.iter().any(|s| {
        s.stage_type == StageType::CodeReview
            || s.id.to_lowercase().contains("code-review")
            || s.name.to_lowercase().contains("code review")
    });

    // If integration-verify exists but code-review doesn't, insert code-review
    if let Some(iv_idx) = integration_verify_idx {
        if !has_code_review {
            let integration_verify = &stages[iv_idx];
            let working_dir = integration_verify.working_dir.clone();

            // Create code-review stage
            let code_review =
                create_code_review_stage(stages, &integration_verify.id, &working_dir);

            println!(
                "  {} Auto-inserting {} stage before integration-verify",
                "✓".green().bold(),
                "code-review".cyan().bold()
            );

            // Update integration-verify to depend on code-review instead of its current deps
            let old_deps = stages[iv_idx].dependencies.clone();
            stages[iv_idx].dependencies = vec!["code-review".to_string()];

            // Insert code-review before integration-verify with the old deps
            let mut code_review = code_review;
            code_review.dependencies = old_deps;
            stages.insert(iv_idx, code_review);
        }
    }

    stages.clone()
}

/// Create a default code-review stage that depends on all non-special stages.
///
/// This stage is auto-inserted before integration-verify if not present.
fn create_code_review_stage(
    stages: &[StageDefinition],
    integration_verify_id: &str,
    working_dir: &str,
) -> StageDefinition {
    // Find all stages that integration-verify depends on (these are the implementation stages)
    let integration_verify_stage = stages
        .iter()
        .find(|s| s.id == integration_verify_id)
        .expect("integration-verify stage must exist");

    // Code review depends on all the same stages that integration-verify depends on
    let dependencies = integration_verify_stage.dependencies.clone();

    StageDefinition {
        id: "code-review".to_string(),
        name: "Code Review".to_string(),
        description: Some(
            r#"Automated code review stage for security and quality analysis.

Use parallel subagents to perform comprehensive review:

PARALLEL SUBAGENT 1 - Security Review:
  - Check for security vulnerabilities (OWASP Top 10)
  - Review input validation and sanitization
  - Check for hardcoded secrets or credentials
  - Verify authentication and authorization patterns

PARALLEL SUBAGENT 2 - Architecture Review:
  - Review code structure and organization
  - Check for proper separation of concerns
  - Verify error handling patterns
  - Review API design consistency

PARALLEL SUBAGENT 3 - Code Quality Review:
  - Check for code duplication
  - Review test coverage for new code
  - Verify edge case handling
  - Check for proper logging and observability

FIX any issues found - do not just report them.

MEMORY RECORDING (use memory ONLY - never knowledge):
- Record issues found: loom memory note "Found: description"
- Record fixes applied: loom memory decision "Fixed X by Y" --context "why""#
                .to_string(),
        ),
        dependencies,
        parallel_group: None,
        acceptance: vec![
            "cargo clippy --all-targets -- -D warnings".to_string(),
            "cargo test".to_string(),
        ],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: working_dir.to_string(),
        stage_type: StageType::CodeReview,
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
        truth_checks: vec![],
        wiring_tests: vec![],
        dead_code_check: None,
        context_budget: None,
        sandbox: StageSandboxConfig::default(),
    }
}

/// Detect the stage type from the definition.
///
/// Uses explicit `stage_type` field if set, otherwise falls back to
/// detecting stage type based on ID or name patterns (case-insensitive):
/// - "knowledge" -> Knowledge
/// - "code-review" or "code review" -> CodeReview
/// - "integration-verify" or "integration verify" -> IntegrationVerify
fn detect_stage_type(stage_def: &StageDefinition) -> StageType {
    // Check explicit stage_type field first (if not default Standard)
    if stage_def.stage_type != StageType::Standard {
        return stage_def.stage_type;
    }

    let id_lower = stage_def.id.to_lowercase();
    let name_lower = stage_def.name.to_lowercase();

    // Detect Knowledge stage
    if id_lower.contains("knowledge") || name_lower.contains("knowledge") {
        return StageType::Knowledge;
    }

    // Detect CodeReview stage
    if id_lower.contains("code-review")
        || name_lower.contains("code-review")
        || name_lower.contains("code review")
    {
        return StageType::CodeReview;
    }

    // Detect IntegrationVerify stage
    if id_lower.contains("integration-verify")
        || name_lower.contains("integration-verify")
        || name_lower.contains("integration verify")
    {
        return StageType::IntegrationVerify;
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
        truths: stage_def.truths.clone(),
        artifacts: stage_def.artifacts.clone(),
        wiring: stage_def.wiring.clone(),
        sandbox: stage_def.sandbox.clone(),
    }
}
