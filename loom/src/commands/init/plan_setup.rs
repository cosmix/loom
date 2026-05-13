//! Plan initialization and stage creation for loom init.

use crate::fs::stage_files::stage_file_path;
use crate::fs::work_dir::{self, WorkDir};
use crate::git::branch::current_branch;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::terminal::container::probe::{run_firewall_smoke_test, ProbeResult};
use crate::orchestrator::terminal::container::runtime::Runtime;
use crate::plan::graph::levels::compute_all_levels;
use crate::plan::parser::parse_plan;
use crate::plan::schema::{
    check_knowledge_recommendations, check_sandbox_recommendations, detect_stage_type,
    validate_structural_preflight, BackendType, StageDefinition,
};
use crate::sandbox::{merge_config as merge_sandbox_config, validate_config as validate_sandbox};
use crate::verify::serialize_stage_to_markdown;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use colored::Colorize;
use std::fs;
use std::path::Path;
use toml_edit::{value, Item, Table};

// Plan / config writes go through the centralized `fs::work_dir` API using
// `toml_edit`, which preserves comments and unknown keys across edits.

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

    // If plan has no sandbox network domains, suggest some based on project type
    if parsed_plan
        .metadata
        .loom
        .sandbox
        .network
        .allowed_domains
        .is_empty()
    {
        let current_dir = std::env::current_dir()?;
        let detected = crate::language::detect_project_languages(&current_dir);
        if !detected.is_empty() {
            use crate::language::DetectedLanguage;
            let mut domains = vec!["github.com".to_string(), "api.github.com".to_string()];
            for lang in &detected {
                match lang {
                    DetectedLanguage::Rust => {
                        domains.push("crates.io".to_string());
                        domains.push("static.crates.io".to_string());
                    }
                    DetectedLanguage::TypeScript => {
                        domains.push("registry.npmjs.org".to_string());
                    }
                    DetectedLanguage::Python => {
                        domains.push("pypi.org".to_string());
                    }
                    DetectedLanguage::Go => {
                        domains.push("proxy.golang.org".to_string());
                    }
                }
            }
            println!(
                "  {} {}",
                "💡".blue(),
                "No sandbox network domains configured. Suggested domains for your project:".blue()
            );
            for d in &domains {
                println!("      - \"{}\"", d);
            }
        }
    }

    let stages = parsed_plan.stages.clone();

    // Run structural preflight validation (non-fatal warnings)
    let repo_root = std::env::current_dir().ok();
    let preflight_warnings = validate_structural_preflight(&stages, repo_root.as_deref());
    for warning in &preflight_warnings {
        println!("  {} {}", "⚠".yellow().bold(), warning.yellow());
    }

    // Validate every stage's resolved sandbox against the project backend.
    // This catches incompatible combinations (e.g. bypass-permissions on
    // native) at init time — far cheaper than discovering the mismatch
    // mid-run when the daemon refuses to spawn the session.
    let project_backend = work_dir::read_project_execution(work_dir.root())
        .context("Failed to read project execution config from .work/config.toml")?
        .map(|cfg| cfg.backend)
        .unwrap_or(BackendType::Native);
    let plan_sandbox = &parsed_plan.metadata.loom.sandbox;
    for stage_def in &stages {
        let stage_type = detect_stage_type(stage_def);
        let merged = merge_sandbox_config(
            plan_sandbox,
            &stage_def.sandbox,
            stage_type,
            project_backend,
        );
        validate_sandbox(&merged, project_backend).with_context(|| {
            format!(
                "Stage '{}' has an incompatible sandbox configuration for backend '{}'",
                stage_def.id, project_backend
            )
        })?;
    }

    let base_branch =
        current_branch(&std::env::current_dir()?).context("Failed to get current git branch")?;

    // Store source_path as relative to the project root so it works from
    // both the main repo and worktrees (where .work/ is a symlink).
    // Falls back to canonical (absolute) if the plan is outside the repo.
    let project_root = std::env::current_dir()?;
    let relative_source_path = canonical_path
        .strip_prefix(&project_root)
        .unwrap_or(&canonical_path);

    // Build config using the centralized fs::work_dir API. We start from an
    // existing document (preserving comments / unknown keys) and write the
    // [plan] table via toml_edit so structured serde wrappers don't flatten
    // ad-hoc additions made by other tools.
    let mut doc = work_dir::read_config(work_dir.root())?;

    if doc.iter().next().is_none() {
        // First-time write: prepend a header comment so the file is human-friendly.
        let header = format!(
            "# loom Configuration\n# Generated from plan: {}\n",
            canonical_path.display()
        );
        doc.decor_mut().set_prefix(header);
    }

    let mut plan_table = Table::new();
    plan_table["source_path"] = value(relative_source_path.display().to_string());
    plan_table["plan_id"] = value(parsed_plan.id.clone());
    plan_table["plan_name"] = value(parsed_plan.name.clone());
    plan_table["base_branch"] = value(base_branch.clone());
    doc.insert("plan", Item::Table(plan_table));

    work_dir::write_config(work_dir.root(), &doc).context("Failed to write .work/config.toml")?;

    // Persist plan-level sandbox + execution snapshots so loader fallbacks
    // don't silently substitute defaults after .work/stages exists.
    work_dir::write_plan_sandbox(work_dir.root(), &parsed_plan.metadata.loom.sandbox)
        .context("Failed to persist plan-level sandbox config")?;

    if let Some(plan_exec) = &parsed_plan.metadata.loom.execution {
        work_dir::write_plan_execution(work_dir.root(), plan_exec)
            .context("Failed to persist plan-level execution config")?;
    }

    println!(
        "  {} Config saved {}",
        "✓".green().bold(),
        "config.toml".dimmed()
    );

    let depths = compute_all_levels(&stages, |s| s.id.as_str(), |s| &s.dependencies);

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

/// Run the container firewall enforcement smoke test for a freshly built
/// image and bail with an actionable error if the firewall is not enforced.
///
/// Called by [`crate::commands::init::execute`] after the container image
/// is built. Kept in `plan_setup` so the post-init wiring (image build →
/// firewall probe → stage file generation) lives in one module and goal-
/// backward wiring checks can spot the integration in a single place.
pub(crate) fn probe_firewall_or_bail(runtime: Runtime, image_ref: &str) -> Result<ProbeResult> {
    let result = run_firewall_smoke_test(runtime, image_ref)
        .context("Failed to run firewall enforcement smoke test")?;
    if !result.enforced {
        bail!(
            "Firewall enforcement failed on this runtime. The container \
             firewall is the authoritative network policy for stages — \
             refusing to proceed because traffic was not blocked despite \
             an empty allowlist. Re-run with --allow-insecure-runtime to \
             override (use with caution; container egress will not be \
             filtered). Diagnostic:\n{}",
            result.diagnostic
        );
    }
    Ok(result)
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
        execution_secs: None,
        attempt_started_at: None,
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
        artifacts: stage_def.artifacts.clone(),
        wiring: stage_def.wiring.clone(),
        wiring_tests: stage_def.wiring_tests.clone(),
        dead_code_check: stage_def.dead_code_check.clone(),
        before_stage: stage_def.before_stage.clone(),
        after_stage: stage_def.after_stage.clone(),
        fix_attempts: 0,
        dispute_count: 0,
        evidence_rounds: 0,
        amendments_applied: 0,
        sandbox: stage_def.sandbox.clone(),
        execution_mode: stage_def.execution_mode,
        max_fix_attempts: None,
        review_reason: None,
        bug_fix: stage_def.bug_fix,
        regression_test: stage_def.regression_test.clone(),
        model: stage_def.model.clone(),
        reasoning_effort: stage_def.reasoning_effort.clone(),
        execution_backend: stage_def.execution.as_ref().and_then(|e| e.backend),
        is_possibly_stuck: false,
    }
}
