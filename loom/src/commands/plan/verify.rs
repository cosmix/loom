//! loom plan verify — validate a plan file without side effects.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::commands::graph::colors::stage_color;
use crate::plan::graph::levels::compute_all_levels;
use crate::plan::parser::{extract_plan_name, extract_yaml_metadata};
use crate::plan::schema::{
    check_knowledge_recommendations, check_sandbox_recommendations, detect_stage_type,
    validate_structural_preflight, LoomMetadata, StageDefinition, StageType,
};

const MAX_FILE_BYTES: u64 = 1_048_576; // 1 MiB

// ── JSON output structs ────────────────────────────────────────────────────

#[derive(Serialize)]
struct JsonPlan {
    id: Option<String>,
    name: Option<String>,
    source: String,
}

#[derive(Serialize)]
struct JsonError {
    stage_id: Option<String>,
    message: String,
}

#[derive(Serialize)]
struct JsonWarnings {
    structural: Vec<String>,
    knowledge: Vec<String>,
    sandbox: Vec<String>,
}

impl JsonWarnings {
    fn empty() -> Self {
        Self {
            structural: vec![],
            knowledge: vec![],
            sandbox: vec![],
        }
    }

    fn total(&self) -> usize {
        self.structural.len() + self.knowledge.len() + self.sandbox.len()
    }
}

#[derive(Serialize)]
struct JsonStageLevel {
    id: String,
    name: String,
    stage_type: String,
    dependencies: Vec<String>,
}

#[derive(Serialize)]
struct JsonOutput {
    plan: JsonPlan,
    valid: bool,
    errors: Vec<JsonError>,
    warnings: JsonWarnings,
    levels: Vec<Vec<JsonStageLevel>>,
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn should_fail(errors: usize, warnings: usize, strict: bool) -> bool {
    errors > 0 || (strict && warnings > 0)
}

fn stage_type_label(st: StageType) -> &'static str {
    match st {
        StageType::Standard => "standard",
        StageType::Knowledge => "knowledge",
        StageType::IntegrationVerify => "integration-verify",
        StageType::KnowledgeDistill => "knowledge-distill",
    }
}

/// Walk up from plan_path.parent() looking for a directory containing `.git`.
/// Handles both `.git` directories and `.git` files (worktrees).
fn find_repo_root(plan_path: &Path) -> Option<PathBuf> {
    let mut dir = plan_path.parent()?;
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent,
            _ => return None,
        }
    }
}

fn build_levels_output(
    stages: &[StageDefinition],
    levels_map: &std::collections::HashMap<String, usize>,
) -> Vec<Vec<JsonStageLevel>> {
    let max_level = levels_map.values().copied().max().unwrap_or(0);
    let mut result = Vec::new();
    for level_num in 0..=max_level {
        let mut level_stages: Vec<&StageDefinition> = stages
            .iter()
            .filter(|s| levels_map.get(&s.id).copied().unwrap_or(0) == level_num)
            .collect();
        level_stages.sort_by(|a, b| a.id.cmp(&b.id));
        let json_stages = level_stages
            .iter()
            .map(|s| {
                let st = detect_stage_type(s);
                JsonStageLevel {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    stage_type: stage_type_label(st).to_string(),
                    dependencies: s.dependencies.clone(),
                }
            })
            .collect();
        result.push(json_stages);
    }
    result
}

fn emit_json(output: &JsonOutput) {
    match serde_json::to_string_pretty(output) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("JSON serialization error: {e}"),
    }
    let _ = std::io::stdout().flush();
}

// ── Human output ──────────────────────────────────────────────────────────

struct HumanArgs<'a> {
    source: &'a str,
    plan_id: &'a Option<String>,
    plan_name: &'a Option<String>,
    hard_errors: &'a [JsonError],
    warnings: &'a JsonWarnings,
    levels: &'a [Vec<JsonStageLevel>],
    total_errors: usize,
    total_warnings: usize,
    strict: bool,
}

fn print_human(args: HumanArgs<'_>) {
    let HumanArgs {
        source,
        plan_id,
        plan_name,
        hard_errors,
        warnings,
        levels,
        total_errors,
        total_warnings,
        strict,
    } = args;
    // Header
    let id_str = plan_id.as_deref().unwrap_or("unknown");
    let name_str = plan_name.as_deref().unwrap_or("(no title)");
    println!("{}", format!("── Plan: {id_str} ──").cyan().bold());
    println!("   {} ({})", name_str.bold(), source.dimmed());
    println!();

    // Errors section
    if !hard_errors.is_empty() {
        println!("{}", "Errors".red().bold());
        for e in hard_errors {
            let prefix = match &e.stage_id {
                Some(id) => format!("[{id}] "),
                None => String::new(),
            };
            println!("  {} {}{}", "✗".red().bold(), prefix.red(), e.message);
        }
        println!();
    }

    // Warnings sections
    let has_structural = !warnings.structural.is_empty();
    let has_knowledge = !warnings.knowledge.is_empty();
    let has_sandbox = !warnings.sandbox.is_empty();

    if has_structural || has_knowledge || has_sandbox {
        if has_structural {
            println!("{}", "Structural".yellow().bold());
            for w in &warnings.structural {
                println!("  {} {}", "⚠".yellow(), w);
            }
            println!();
        }
        if has_knowledge {
            println!("{}", "Knowledge".yellow().bold());
            for w in &warnings.knowledge {
                println!("  {} {}", "⚠".yellow(), w);
            }
            println!();
        }
        if has_sandbox {
            println!("{}", "Sandbox".yellow().bold());
            for w in &warnings.sandbox {
                println!("  {} {}", "⚠".yellow(), w);
            }
            println!();
        }
    }

    // Stages by level
    if !levels.is_empty() {
        println!("{}", "Stages by Level".bold());
        for (level_num, stage_list) in levels.iter().enumerate() {
            println!(
                "  {}",
                format!("Level {level_num} ({} stage(s))", stage_list.len()).dimmed()
            );
            for stage in stage_list {
                let color = stage_color(&stage.id);
                let indicator = if stage.dependencies.is_empty() {
                    "●".green().to_string()
                } else {
                    "○".yellow().to_string()
                };
                let type_label = stage.stage_type.dimmed().to_string();
                let deps_str = if stage.dependencies.is_empty() {
                    String::new()
                } else {
                    format!("  ← {}", stage.dependencies.join(", ").dimmed())
                };
                println!(
                    "    {}  {}  {}  {}{}",
                    indicator,
                    stage.id.color(color),
                    stage.name.bold(),
                    type_label,
                    deps_str,
                );
            }
        }
        println!();
    }

    // Summary
    let strict_note = if strict && total_warnings > 0 {
        " (strict: failing)"
    } else {
        ""
    };
    let summary = format!("{total_errors} error(s), {total_warnings} warning(s){strict_note}");
    if total_errors > 0 || (strict && total_warnings > 0) {
        println!("{}", summary.red());
    } else if total_warnings > 0 {
        println!("{}", summary.yellow());
    } else {
        println!("{}", summary.green());
    }
}

// ── Entry point ────────────────────────────────────────────────────────────

pub fn execute(path: &Path, strict: bool, json: bool, no_color: bool) -> Result<()> {
    if no_color {
        colored::control::set_override(false);
    }

    let source_str = path.to_string_lossy().to_string();

    // File existence check
    if !path.exists() || !path.is_file() {
        let msg = format!("Plan file not found: {}", path.display());
        if json {
            emit_json(&JsonOutput {
                plan: JsonPlan {
                    id: None,
                    name: None,
                    source: source_str,
                },
                valid: false,
                errors: vec![JsonError {
                    stage_id: None,
                    message: msg,
                }],
                warnings: JsonWarnings::empty(),
                levels: vec![],
            });
            std::process::exit(1);
        }
        bail!("Plan file not found: {}", path.display());
    }

    // File size check
    let file_len = std::fs::metadata(path)
        .with_context(|| format!("Failed to stat {}", path.display()))?
        .len();
    if file_len > MAX_FILE_BYTES {
        let msg = format!(
            "Plan file too large: {} bytes (limit: {} bytes)",
            file_len, MAX_FILE_BYTES
        );
        if json {
            emit_json(&JsonOutput {
                plan: JsonPlan {
                    id: None,
                    name: None,
                    source: source_str,
                },
                valid: false,
                errors: vec![JsonError {
                    stage_id: None,
                    message: msg,
                }],
                warnings: JsonWarnings::empty(),
                levels: vec![],
            });
            std::process::exit(1);
        }
        bail!(
            "Plan file too large: {} bytes (limit: {} bytes)",
            file_len,
            MAX_FILE_BYTES
        );
    }

    // Read content
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    // Derive plan name from H1 header (best-effort)
    let plan_name = extract_plan_name(&content).ok();

    // Extract YAML block — failure means we can't confirm this is a loom plan
    let yaml = match extract_yaml_metadata(&content) {
        Ok(y) => y,
        Err(e) => {
            let msg = e.to_string();
            if json {
                emit_json(&JsonOutput {
                    plan: JsonPlan {
                        id: None,
                        name: None,
                        source: source_str,
                    },
                    valid: false,
                    errors: vec![JsonError {
                        stage_id: None,
                        message: msg,
                    }],
                    warnings: JsonWarnings::empty(),
                    levels: vec![],
                });
                std::process::exit(1);
            }
            bail!("{}", e);
        }
    };

    // Derive plan ID from filename (available once we know it's a loom plan)
    let plan_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());

    // Deserialize LoomMetadata
    let loom_metadata: LoomMetadata = match serde_yaml::from_str(&yaml) {
        Ok(m) => m,
        Err(e) => {
            let msg = format!("YAML parse error: {e}");
            if json {
                emit_json(&JsonOutput {
                    plan: JsonPlan {
                        id: None,
                        name: None,
                        source: source_str,
                    },
                    valid: false,
                    errors: vec![JsonError {
                        stage_id: None,
                        message: msg,
                    }],
                    warnings: JsonWarnings::empty(),
                    levels: vec![],
                });
                std::process::exit(1);
            }
            bail!("YAML parse error: {}", e);
        }
    };

    // ── Validation ────────────────────────────────────────────────────────

    let validation_result = crate::plan::schema::validate(&loom_metadata);

    let mut hard_errors: Vec<JsonError> = Vec::new();
    let mut soft_warnings = JsonWarnings::empty();
    let mut levels: Vec<Vec<JsonStageLevel>> = Vec::new();

    match validation_result {
        Err(errs) => {
            for e in errs {
                hard_errors.push(JsonError {
                    stage_id: e.stage_id,
                    message: e.message,
                });
            }
        }
        Ok(()) => {
            // Soft checks (only when schema validation passes)
            let repo_root_opt = find_repo_root(path);
            soft_warnings = JsonWarnings {
                structural: validate_structural_preflight(
                    &loom_metadata.loom.stages,
                    repo_root_opt.as_deref(),
                ),
                knowledge: check_knowledge_recommendations(&loom_metadata.loom.stages),
                sandbox: check_sandbox_recommendations(&loom_metadata),
            };

            // DAG cycle detection
            match crate::plan::graph::ExecutionGraph::build(loom_metadata.loom.stages.clone()) {
                Err(e) => {
                    hard_errors.push(JsonError {
                        stage_id: None,
                        message: e.to_string(),
                    });
                }
                Ok(_graph) => {
                    let levels_map = compute_all_levels(
                        &loom_metadata.loom.stages,
                        |s| s.id.as_str(),
                        |s| &s.dependencies,
                    );
                    levels = build_levels_output(&loom_metadata.loom.stages, &levels_map);
                }
            }
        }
    }

    let total_errors = hard_errors.len();
    let total_warnings = soft_warnings.total();
    let valid = total_errors == 0;

    // ── Output ─────────────────────────────────────────────────────────────

    if json {
        emit_json(&JsonOutput {
            plan: JsonPlan {
                id: plan_id,
                name: plan_name,
                source: source_str,
            },
            valid,
            errors: hard_errors,
            warnings: soft_warnings,
            levels,
        });
        std::process::exit(if should_fail(total_errors, total_warnings, strict) {
            1
        } else {
            0
        });
    }

    print_human(HumanArgs {
        source: &source_str,
        plan_id: &plan_id,
        plan_name: &plan_name,
        hard_errors: &hard_errors,
        warnings: &soft_warnings,
        levels: &levels,
        total_errors,
        total_warnings,
        strict,
    });

    if should_fail(total_errors, total_warnings, strict) {
        bail!(
            "Plan validation failed ({} error(s), {} warning(s))",
            total_errors,
            total_warnings
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_should_fail() {
        assert!(!should_fail(0, 0, false));
        assert!(!should_fail(0, 0, true));
        assert!(should_fail(1, 0, false));
        assert!(!should_fail(0, 1, false));
        assert!(should_fail(0, 1, true));
        assert!(should_fail(1, 1, true));
    }

    #[test]
    fn test_repo_root_walk() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create .git at root level to make this a "git repo"
        fs::create_dir(root.join(".git")).unwrap();

        // Create a plan file 3 levels deep
        let deep_dir = root.join("a/b/c");
        fs::create_dir_all(&deep_dir).unwrap();
        let plan_path = deep_dir.join("plan.md");
        fs::write(&plan_path, "# Test").unwrap();

        let found = find_repo_root(&plan_path);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), root);

        // Tree with no .git anywhere: use a path whose entire ancestor chain
        // is non-existent, so .git cannot exist at any level.
        // exists() returns false for non-existent paths, so this reliably → None.
        let nonexistent_plan =
            std::path::Path::new("/tmp-loom-verify-nonexistent-12345/a/b/plan.md");
        assert!(find_repo_root(nonexistent_plan).is_none());
    }
}
