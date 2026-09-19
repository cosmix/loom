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
    base_tree, check_knowledge_recommendations, check_sandbox_recommendations, detect_stage_type,
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

#[derive(Serialize, Default)]
struct JsonWarnings {
    structural: Vec<String>,
    knowledge: Vec<String>,
    sandbox: Vec<String>,
    /// Criteria that already pass on the untouched tree (see `base_tree`).
    baseline: Vec<String>,
}

impl JsonWarnings {
    /// Every bucket with its human-output heading, in display order.
    fn sections(&self) -> [(&'static str, &[String]); 4] {
        [
            ("Structural", self.structural.as_slice()),
            ("Knowledge", self.knowledge.as_slice()),
            ("Sandbox", self.sandbox.as_slice()),
            ("Baseline", self.baseline.as_slice()),
        ]
    }

    /// Warnings across every bucket; `--strict` fails on any of them.
    fn total(&self) -> usize {
        self.sections().iter().map(|(_, items)| items.len()).sum()
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
    /// Checks that were skipped, and why; never counted as warnings.
    notes: Vec<String>,
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

/// Report a failure that stops verification before validation runs: a JSON
/// envelope and exit 1 under `--json`, an error otherwise.
fn early_failure(json: bool, source: String, message: String) -> Result<()> {
    if json {
        emit_json(&JsonOutput {
            plan: JsonPlan {
                id: None,
                name: None,
                source,
            },
            valid: false,
            errors: vec![JsonError {
                stage_id: None,
                message,
            }],
            warnings: JsonWarnings::default(),
            notes: vec![],
            levels: vec![],
        });
        std::process::exit(1);
    }
    bail!("{message}")
}

/// Sandbox policy hard errors: run the SAME merge + validation that
/// `loom init` and stage spawn use (crate::sandbox::merge_config /
/// validate_config / validate_emittable), so a plan `loom init` would refuse
/// is not reported clean here. These are errors, not warnings — init already
/// rejects these plans outright.
fn sandbox_policy_errors(metadata: &LoomMetadata) -> Vec<JsonError> {
    let mut errors = Vec::new();
    for stage in &metadata.loom.stages {
        let merged = crate::sandbox::merge_config(
            &metadata.loom.sandbox,
            &stage.sandbox,
            detect_stage_type(stage),
            &stage.implementers,
        );
        if let Err(e) = crate::sandbox::validate_config(&merged) {
            errors.push(JsonError {
                stage_id: Some(stage.id.clone()),
                message: e.to_string(),
            });
        }
        if let Err(e) = crate::sandbox::validate_emittable(&merged) {
            errors.push(JsonError {
                stage_id: Some(stage.id.clone()),
                message: e.to_string(),
            });
        }
    }
    errors
}

/// DAG cycle detection, then the stages grouped by execution level.
fn dag_levels(stages: &[StageDefinition]) -> Result<Vec<Vec<JsonStageLevel>>, JsonError> {
    if let Err(e) = crate::plan::graph::ExecutionGraph::build(stages.to_vec()) {
        return Err(JsonError {
            stage_id: None,
            message: e.to_string(),
        });
    }
    let levels_map = compute_all_levels(stages, |s| s.id.as_str(), |s| &s.dependencies);
    Ok(build_levels_output(stages, &levels_map))
}

// ── Human output ──────────────────────────────────────────────────────────

struct HumanArgs<'a> {
    source: &'a str,
    plan_id: &'a Option<String>,
    plan_name: &'a Option<String>,
    hard_errors: &'a [JsonError],
    warnings: &'a JsonWarnings,
    notes: &'a [String],
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
        notes,
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

    // Warnings sections, then notes on checks that were skipped
    for (title, items) in warnings.sections() {
        if items.is_empty() {
            continue;
        }
        println!("{}", title.yellow().bold());
        for w in items {
            println!("  {} {}", "⚠".yellow(), w);
        }
        println!();
    }
    for note in notes {
        println!("{}", format!("Note: {note}").dimmed());
        println!();
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
        let message = format!("Plan file not found: {}", path.display());
        return early_failure(json, source_str, message);
    }

    // File size check
    let file_len = std::fs::metadata(path)
        .with_context(|| format!("Failed to stat {}", path.display()))?
        .len();
    if file_len > MAX_FILE_BYTES {
        let message =
            format!("Plan file too large: {file_len} bytes (limit: {MAX_FILE_BYTES} bytes)");
        return early_failure(json, source_str, message);
    }

    // Read content
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            let message = format!("Failed to read {}: {e}", path.display());
            return early_failure(json, source_str, message);
        }
    };

    // Derive plan name from H1 header (best-effort)
    let plan_name = extract_plan_name(&content).ok();

    // Extract YAML block — failure means we can't confirm this is a loom plan
    let yaml = match extract_yaml_metadata(&content) {
        Ok(y) => y,
        Err(e) => return early_failure(json, source_str, e.to_string()),
    };

    // Derive plan ID from filename (available once we know it's a loom plan)
    let plan_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());

    // Deserialize LoomMetadata
    let loom_metadata: LoomMetadata = match serde_yaml::from_str(&yaml) {
        Ok(m) => m,
        Err(e) => return early_failure(json, source_str, format!("YAML parse error: {e}")),
    };

    // ── Validation ────────────────────────────────────────────────────────

    let stages = &loom_metadata.loom.stages;
    let repo_root = find_repo_root(path);
    let mut hard_errors: Vec<JsonError> = Vec::new();
    let mut soft_warnings = JsonWarnings::default();
    let mut levels: Vec<Vec<JsonStageLevel>> = Vec::new();

    match crate::plan::schema::validate(&loom_metadata) {
        Err(errs) => {
            for e in errs {
                hard_errors.push(JsonError {
                    stage_id: e.stage_id,
                    message: e.message,
                });
            }
        }
        Ok(()) => {
            hard_errors.extend(sandbox_policy_errors(&loom_metadata));
            // Soft checks (only when schema validation passes)
            soft_warnings = JsonWarnings {
                structural: validate_structural_preflight(stages, repo_root.as_deref()),
                knowledge: check_knowledge_recommendations(stages),
                sandbox: check_sandbox_recommendations(&loom_metadata),
                baseline: Vec::new(),
            };
            match dag_levels(stages) {
                Ok(by_level) => levels = by_level,
                Err(error) => hard_errors.push(error),
            }
        }
    }

    // Base-tree evaluation runs whether or not the schema validates, so one
    // pass reports a plan's hazard errors and its already-green criteria.
    let baseline = base_tree::check_base_tree(stages, repo_root.as_deref());
    soft_warnings.baseline = baseline.warnings;
    let notes: Vec<String> = baseline.note.into_iter().collect();

    let total_errors = hard_errors.len();
    let total_warnings = soft_warnings.total();
    let valid = total_errors == 0;
    let failed = should_fail(total_errors, total_warnings, strict);

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
            notes,
            levels,
        });
        std::process::exit(i32::from(failed));
    }

    print_human(HumanArgs {
        source: &source_str,
        plan_id: &plan_id,
        plan_name: &plan_name,
        hard_errors: &hard_errors,
        warnings: &soft_warnings,
        notes: &notes,
        levels: &levels,
        total_errors,
        total_warnings,
        strict,
    });

    if failed {
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

    #[test]
    fn test_repo_root_walk_with_dotgit_as_file() {
        // Worktrees use a `.git` *file* (containing `gitdir: <path>`) instead
        // of a directory. `find_repo_root` claims to support this via
        // `Path::exists`, which returns true for both files and dirs.
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::write(root.join(".git"), "gitdir: /some/where/else\n").unwrap();

        let deep_dir = root.join("a/b");
        fs::create_dir_all(&deep_dir).unwrap();
        let plan_path = deep_dir.join("plan.md");
        fs::write(&plan_path, "# Test").unwrap();

        let found = find_repo_root(&plan_path);
        assert_eq!(found.as_deref(), Some(root));
    }

    #[test]
    fn baseline_warnings_count_toward_strict() {
        let warnings = JsonWarnings {
            baseline: vec!["criterion passes on the untouched tree".to_string()],
            ..JsonWarnings::default()
        };
        assert_eq!(warnings.total(), 1);
        assert!(should_fail(0, warnings.total(), true));
        assert!(!should_fail(0, warnings.total(), false));
        assert_eq!(warnings.sections()[3].0, "Baseline");
    }

    #[test]
    fn verifies_a_plan_with_no_repository_root() {
        let temp = TempDir::new().unwrap();
        let plan_path = temp.path().join("PLAN-no-repo.md");
        let plan = "# No Repo\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n  \
                    stages:\n    - id: stage-one\n      name: \"Stage One\"\n      \
                    stage_type: standard\n      working_dir: \".\"\n      acceptance:\n        \
                    - \"rg -q present notes.txt\"\n```\n\n<!-- END loom METADATA -->\n";
        fs::write(&plan_path, plan).unwrap();

        execute(&plan_path, false, false, false).unwrap();
        let report = base_tree::check_base_tree(&[], None);
        assert!(report.warnings.is_empty());
        assert!(report.note.is_some_and(|note| note.contains("skipped")));
    }
}
