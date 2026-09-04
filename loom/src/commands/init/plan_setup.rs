//! Plan initialization and stage creation for loom init.

use crate::fs::stage_files::stage_file_path;
use crate::fs::work_dir::{self, ContextConfig, WorkDir};
use crate::git::branch::current_branch;
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::models::stage::Stage;
use crate::plan::graph::levels::compute_all_levels;
use crate::plan::parser::{parse_plan, ParsedPlan};
use crate::plan::schema::{
    check_knowledge_recommendations, check_sandbox_recommendations, detect_stage_type,
    unsafe_plan_reasons, validate_structural_preflight, StageDefinition,
};
use crate::sandbox::{
    merge_config as merge_sandbox_config, validate_config as validate_sandbox, validate_emittable,
};
use crate::verify::serialize_stage_to_markdown;
use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{value, Item, Table};

// Plan / config writes go through the centralized `fs::work_dir` API using
// `toml_edit`, which preserves comments and unknown keys across edits.

/// A plan that has been parsed and had every check that can FAIL run against
/// it, before `loom init` creates or edits anything on disk. Produced by
/// `preflight_plan` and consumed by `initialize_with_plan`.
#[derive(Debug)]
pub struct PreflightedPlan {
    canonical_path: PathBuf,
    parsed_plan: ParsedPlan,
}

/// Parse a plan and run every check on it that can fail, without printing or
/// writing anything. `execute()` calls this before it bootstraps the repo or
/// creates the state directory, so a rejected plan never leaves behind a
/// half-initialized state directory, an installed pre-commit hook, or an
/// operator-answered backend prompt that then has to be answered again.
///
/// `allow_unsafe_plan` is the operator's explicit acknowledgement of a plan
/// that expands the sandbox policy (see `require_unsafe_plan_acknowledgement`).
pub fn preflight_plan(plan_path: &Path, allow_unsafe_plan: bool) -> Result<PreflightedPlan> {
    if !plan_path.exists() {
        anyhow::bail!("Plan file does not exist: {}", plan_path.display());
    }

    // Canonicalize the plan path to resolve symlinks and relative paths
    let canonical_path = plan_path
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize plan path: {}", plan_path.display()))?;
    require_utf8_plan_path(&canonical_path)?;

    let parsed_plan = parse_plan(&canonical_path)
        .with_context(|| format!("Failed to parse plan file: {}", canonical_path.display()))?;

    let unsafe_reasons = unsafe_plan_reasons(&parsed_plan.metadata);
    require_unsafe_plan_acknowledgement(&unsafe_reasons, allow_unsafe_plan)?;

    // Validate every stage's resolved sandbox configuration at init time.
    // This catches incompatible combinations (e.g. bypass-permissions) before
    // the repo is even bootstrapped, not just before the daemon ever tries to
    // spawn a session.
    let plan_sandbox = &parsed_plan.metadata.loom.sandbox;
    for stage_def in &parsed_plan.stages {
        let stage_type = detect_stage_type(stage_def);
        let merged = merge_sandbox_config(
            plan_sandbox,
            &stage_def.sandbox,
            stage_type,
            &stage_def.implementers,
        );
        validate_sandbox(&merged).with_context(|| {
            format!(
                "Stage '{}' has an incompatible sandbox configuration",
                stage_def.id
            )
        })?;
        validate_emittable(&merged).with_context(|| {
            format!(
                "Stage '{}' has a sandbox policy that cannot be enforced",
                stage_def.id
            )
        })?;
    }

    Ok(PreflightedPlan {
        canonical_path,
        parsed_plan,
    })
}

/// Initialize the state directory from an already-preflighted plan (see
/// `preflight_plan`). Everything here PRINTS or WRITES, so it stays in the
/// same place in `execute()`'s flow as before the preflight split.
pub fn initialize_with_plan(
    work_dir: &WorkDir,
    plan: &PreflightedPlan,
    terminal_backend: Option<SessionBackendKind>,
) -> Result<usize> {
    let canonical_path = &plan.canonical_path;
    let parsed_plan = &plan.parsed_plan;

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

    let base_branch =
        current_branch(&std::env::current_dir()?).context("Failed to get current git branch")?;

    // Store source_path as relative to the project root so it works from
    // both the main repo and worktrees (where the state directory is a symlink).
    // Falls back to canonical (absolute) if the plan is outside the repo.
    let project_root = std::env::current_dir()?;
    let relative_source_path = canonical_path
        .strip_prefix(&project_root)
        .unwrap_or(canonical_path);

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
    plan_table["source_path"] = value(require_utf8_plan_path(relative_source_path)?.to_string());
    plan_table["plan_id"] = value(parsed_plan.id.clone());
    plan_table["plan_name"] = value(parsed_plan.name.clone());
    plan_table["base_branch"] = value(base_branch.clone());
    doc.insert("plan", Item::Table(plan_table));

    work_dir::write_config(work_dir.root(), &doc).context("Failed to write config.toml")?;

    // Persist plan-level sandbox snapshot so the loader fallback doesn't
    // silently substitute defaults after the state directory's stages exists.
    work_dir::write_plan_sandbox(work_dir.root(), &parsed_plan.metadata.loom.sandbox)
        .context("Failed to persist plan-level sandbox config")?;

    // Persist a default [remote_control] section so the operator has a
    // documented, editable toggle in config.toml from the start.
    work_dir::write_remote_control_config(
        work_dir.root(),
        &crate::remote_control::RemoteControlConfig::default(),
    )
    .context("Failed to persist remote control config")?;

    // Persist [terminal] only when an explicit choice was made (an
    // operator-supplied --backend flag or an interactive prompt answer) — an
    // absent section is what lets `~/.loom/config.toml`'s terminal.backend,
    // then the built-in default, decide at read time (see
    // `fs::work_dir::read_terminal_config`).
    if let Some(backend) = terminal_backend {
        work_dir::write_terminal_config(work_dir.root(), &TerminalConfig { backend })
            .context("Failed to persist terminal config")?;
    }

    // Persist [context] only when the plan itself set a ceiling — an absent
    // section is what lets `~/.loom/config.toml`'s context.ceiling_tokens,
    // then the built-in default, decide at read time (see
    // `fs::work_dir::read_context_config`).
    if parsed_plan.metadata.loom.context_ceiling_tokens.is_some()
        || parsed_plan.metadata.loom.subagent_ceiling_tokens.is_some()
    {
        let context_defaults = ContextConfig::default();
        let context_config = ContextConfig {
            ceiling_tokens: parsed_plan
                .metadata
                .loom
                .context_ceiling_tokens
                .unwrap_or(context_defaults.ceiling_tokens),
            subagent_ceiling_tokens: parsed_plan
                .metadata
                .loom
                .subagent_ceiling_tokens
                .unwrap_or(context_defaults.subagent_ceiling_tokens),
            model_window_tokens: context_defaults.model_window_tokens,
        };
        work_dir::write_context_config(work_dir.root(), &context_config)
            .context("Failed to persist context config")?;
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

fn require_utf8_plan_path(path: &Path) -> Result<&str> {
    path.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "Plan path is not valid UTF-8 and cannot be persisted safely: {:?}",
            path
        )
    })
}

fn require_unsafe_plan_acknowledgement(reasons: &[String], acknowledged: bool) -> Result<()> {
    if !reasons.is_empty() && !acknowledged {
        anyhow::bail!(
            "Plan expands the sandbox policy and requires explicit operator acknowledgement:\n  - {}\n\
             Re-run initialization with --allow-unsafe-plan after reviewing this policy diff.",
            reasons.join("\n  - ")
        );
    }
    Ok(())
}

/// Create a Stage from a StageDefinition
pub(crate) fn create_stage_from_definition(stage_def: &StageDefinition, plan_id: &str) -> Stage {
    Stage::from_definition(stage_def, plan_id)
}

#[cfg(all(test, unix))]
mod tests {
    use super::{require_unsafe_plan_acknowledgement, require_utf8_plan_path};
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    #[test]
    fn rejects_non_utf8_plan_path_before_persistence() {
        let path = PathBuf::from(OsString::from_vec(b"doc/plans/PLAN-\xFF.md".to_vec()));
        let error = require_utf8_plan_path(&path).unwrap_err().to_string();
        assert!(error.contains("not valid UTF-8"));
    }

    #[test]
    fn unsafe_plan_requires_explicit_acknowledgement() {
        let reasons = vec!["plan sandbox.enabled is false".to_string()];
        let error = require_unsafe_plan_acknowledgement(&reasons, false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("--allow-unsafe-plan"));
        assert!(require_unsafe_plan_acknowledgement(&reasons, true).is_ok());
    }
}
