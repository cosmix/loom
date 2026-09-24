//! Plan initialization and stage creation for loom init.

use crate::fs::stage_files::stage_file_path;
use crate::fs::work_dir::{self, WorkDir};
use crate::git::branch::current_branch;
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::models::stage::{PlanIdentity, Stage};
use crate::plan::graph::levels::compute_all_levels;
use crate::plan::parser::{parse_plan, ParsedPlan};
use crate::plan::schema::{
    check_knowledge_recommendations, check_sandbox_recommendations, detect_stage_type,
    validate_structural_preflight, SandboxConfig, StageDefinition,
};
use crate::sandbox::preflight::{sandbox_policy_refusals, SandboxPreflightRefusal};
use crate::sandbox::{merge_config as merge_sandbox_config, validate_emittable};
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
/// A plan whose sandbox is disabled or allows an unsandboxed escape is
/// refused outright, the same way `loom run` refuses one — see
/// `refuse_unconfined_sandbox`. There is no acknowledgement that lets one
/// through.
pub fn preflight_plan(plan_path: &Path) -> Result<PreflightedPlan> {
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

    let plan_sandbox = &parsed_plan.metadata.loom.sandbox;
    let plan = PlanIdentity::from(&parsed_plan);
    let stages: Vec<Stage> = parsed_plan
        .stages
        .iter()
        .map(|stage_def| create_stage_from_definition(stage_def, &plan))
        .collect();
    refuse_unconfined_sandbox(plan_sandbox, &stages)?;

    // Validate every stage's resolved sandbox configuration can still be
    // emitted as Claude Code settings at init time. This catches
    // combinations `sandbox_policy_refusals` does not (e.g. an unemittable
    // policy) before the repo is even bootstrapped, not just before the
    // daemon ever tries to spawn a session.
    for stage_def in &parsed_plan.stages {
        let stage_type = detect_stage_type(stage_def);
        let merged = merge_sandbox_config(
            plan_sandbox,
            &stage_def.sandbox,
            stage_type,
            &stage_def.implementers,
        );
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

/// Refuse a plan whose sandbox is disabled or allows an unsandboxed escape,
/// at the plan level or any stage's override, the same way `loom run`
/// refuses one: `validate_config` refuses both unconditionally, so no
/// acknowledgement could ever let one through later. Calls `loom run`'s own
/// `sandbox_policy_refusals` over the same `Stage` values, so the wording an
/// operator sees matches exactly rather than living as a second, drifting
/// copy.
fn refuse_unconfined_sandbox(plan_sandbox: &SandboxConfig, stages: &[Stage]) -> Result<()> {
    SandboxPreflightRefusal::check(sandbox_policy_refusals(plan_sandbox, stages))
        .context("loom init refuses to create a plan whose sessions would not be confined")
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

    // Persist [context] only for the ceiling keys the plan itself set — one
    // insert_key per key, so a plan setting only one of the pair does not
    // freeze the other's built-in into the file and shadow a user config
    // ceiling that should still apply to it (see
    // `fs::work_dir::read_context_config`).
    let context_ceiling_tokens = parsed_plan.metadata.loom.context_ceiling_tokens;
    let subagent_ceiling_tokens = parsed_plan.metadata.loom.subagent_ceiling_tokens;
    if context_ceiling_tokens.is_some() || subagent_ceiling_tokens.is_some() {
        work_dir::update_config(work_dir.root(), |doc| {
            if let Some(ceiling) = context_ceiling_tokens {
                write_ceiling_key(doc, "ceiling_tokens", ceiling)?;
            }
            if let Some(ceiling) = subagent_ceiling_tokens {
                write_ceiling_key(doc, "subagent_ceiling_tokens", ceiling)?;
            }
            Ok(())
        })
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
        let stage = create_stage_from_definition(stage_def, &PlanIdentity::from(parsed_plan));
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

/// Set `[context] <field> = <value>` in an in-flight `.work/config.toml`
/// document, one key at a time — see [`work_dir::insert_key`], which this
/// wraps so `initialize_with_plan` never writes a ceiling it wasn't told to.
fn write_ceiling_key(doc: &mut toml_edit::DocumentMut, field: &str, value: u32) -> Result<()> {
    work_dir::insert_key(doc, "context", field, toml_edit::Value::from(value as i64))
}

fn require_utf8_plan_path(path: &Path) -> Result<&str> {
    path.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "Plan path is not valid UTF-8 and cannot be persisted safely: {:?}",
            path
        )
    })
}

/// Create a Stage from a StageDefinition
pub(crate) fn create_stage_from_definition(
    stage_def: &StageDefinition,
    plan: &PlanIdentity<'_>,
) -> Stage {
    Stage::from_definition(stage_def, plan)
}

#[cfg(all(test, unix))]
mod tests {
    use super::{refuse_unconfined_sandbox, require_utf8_plan_path};
    use crate::models::stage::Stage;
    use crate::plan::schema::{SandboxConfig, StageSandboxConfig};
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    #[test]
    fn rejects_non_utf8_plan_path_before_persistence() {
        let path = PathBuf::from(OsString::from_vec(b"doc/plans/PLAN-\xFF.md".to_vec()));
        let error = require_utf8_plan_path(&path).unwrap_err().to_string();
        assert!(error.contains("not valid UTF-8"));
    }

    fn stage(id: &str) -> Stage {
        Stage {
            id: id.to_string(),
            ..Stage::default()
        }
    }

    #[test]
    fn accepts_a_plan_with_no_unsafe_sandbox_settings() {
        let plan_sandbox = SandboxConfig::default();
        assert!(refuse_unconfined_sandbox(&plan_sandbox, &[stage("stage-1")]).is_ok());
    }

    #[test]
    fn refuses_a_plan_level_disabled_sandbox_without_asking_for_acknowledgement() {
        let plan_sandbox = SandboxConfig {
            enabled: false,
            ..SandboxConfig::default()
        };
        let error = refuse_unconfined_sandbox(&plan_sandbox, &[stage("stage-1")]).unwrap_err();
        let error = format!("{error:#}");
        assert!(error.contains("sandbox.enabled=false is not permitted"));
        assert!(!error.contains("--allow-unsafe-plan"));
    }

    #[test]
    fn refuses_a_stage_level_unsandboxed_escape_without_asking_for_acknowledgement() {
        let mut stage_with_escape = stage("stage-1");
        stage_with_escape.sandbox = StageSandboxConfig {
            allow_unsandboxed_escape: Some(true),
            ..StageSandboxConfig::default()
        };
        let plan_sandbox = SandboxConfig::default();
        let error = refuse_unconfined_sandbox(&plan_sandbox, &[stage_with_escape]).unwrap_err();
        let error = format!("{error:#}");
        assert!(error.contains("allow_unsandboxed_escape=true is not permitted"));
        assert!(error.contains("stage-1"));
        assert!(!error.contains("--allow-unsafe-plan"));
    }
}
