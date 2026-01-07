use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::fs::work_dir::WorkDir;
use crate::orchestrator::{Orchestrator, OrchestratorConfig, OrchestratorResult};
use crate::plan::graph::ExecutionGraph;
use crate::plan::parser::parse_plan;
use crate::plan::schema::StageDefinition;

const ORCHESTRATOR_SESSION: &str = "loom-orchestrator";

/// Execute plan stages via worktrees/sessions
/// Usage: loom run [--stage <id>] [--manual] [--max-parallel <n>] [--watch]
pub fn execute(
    stage_id: Option<String>,
    manual: bool,
    max_parallel: Option<usize>,
    watch: bool,
) -> Result<()> {
    // 1. Load .work/ directory
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    // 2. Build execution graph - prefer .work/stages/ files, fall back to plan file
    let graph = build_execution_graph(&work_dir)?;

    // 3. Configure orchestrator
    let config = OrchestratorConfig {
        max_parallel_sessions: max_parallel.unwrap_or(4),
        poll_interval: Duration::from_secs(5),
        manual_mode: manual,
        watch_mode: watch,
        tmux_prefix: "loom".to_string(),
        work_dir: work_dir.root().to_path_buf(),
        repo_root: std::env::current_dir()?,
        status_update_interval: Duration::from_secs(30),
    };

    // 4. Create and run orchestrator
    let mut orchestrator = Orchestrator::new(config, graph);

    let result = if let Some(id) = stage_id {
        println!("Running single stage: {id}");
        orchestrator.run_single(&id)?
    } else {
        if watch {
            println!("Running in watch mode (continuous execution)...");
            println!("Press Ctrl+C to stop.\n");
        } else {
            println!("Running all ready stages...");
        }
        orchestrator.run()?
    };

    // 5. Print results
    print_result(&result);

    if result.is_success() {
        Ok(())
    } else {
        bail!("Orchestration completed with failures")
    }
}

/// Execute orchestrator in a background tmux session
/// Usage: loom run --background
pub fn execute_background(
    stage_id: Option<String>,
    manual: bool,
    max_parallel: Option<usize>,
    watch: bool,
) -> Result<()> {
    // Check if orchestrator session already exists
    let check_output = Command::new("tmux")
        .args(["has-session", "-t", ORCHESTRATOR_SESSION])
        .output();

    if let Ok(output) = check_output {
        if output.status.success() {
            println!("Orchestrator is already running.");
            println!();
            println!("To attach:  loom attach orch");
            println!("To stop:    loom clean --sessions");
            return Ok(());
        }
    }

    // Build the loom run command to execute in tmux (must use --foreground!)
    let mut loom_cmd = String::from("loom run --foreground");
    if let Some(ref id) = stage_id {
        loom_cmd.push_str(&format!(" --stage {id}"));
    }
    if manual {
        loom_cmd.push_str(" --manual");
    }
    if let Some(n) = max_parallel {
        loom_cmd.push_str(&format!(" --max-parallel {n}"));
    }
    if watch {
        loom_cmd.push_str(" --watch");
    }

    // Get current directory for tmux session
    let cwd = std::env::current_dir()?;

    // Create new tmux session with orchestrator
    let status = Command::new("tmux")
        .args([
            "new-session",
            "-d", // Detached
            "-s",
            ORCHESTRATOR_SESSION, // Session name
            "-c",
            &cwd.to_string_lossy(), // Working directory
            &loom_cmd,              // Command to run
        ])
        .status()
        .context("Failed to create tmux session for orchestrator")?;

    if !status.success() {
        bail!("Failed to start orchestrator in tmux session");
    }

    println!("Orchestrator started in background.");
    println!();
    println!("To view orchestrator:   loom run --attach");
    println!("To view stage sessions: loom attach <stage-id>");
    println!("To list sessions:       loom sessions list");
    println!("To check status:        loom status");
    println!("To stop orchestrator:   loom clean --sessions");

    Ok(())
}

/// Attach to the running orchestrator tmux session
pub fn attach_orchestrator() -> Result<()> {
    // Check if orchestrator session exists
    let check_output = Command::new("tmux")
        .args(["has-session", "-t", ORCHESTRATOR_SESSION])
        .output();

    match check_output {
        Ok(output) if output.status.success() => {
            println!("Attaching to orchestrator...");
            println!("(Press Ctrl+B then D to return to your terminal)\n");

            // Attach to the session
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                let error = Command::new("tmux")
                    .args(["attach", "-t", ORCHESTRATOR_SESSION])
                    .exec();
                bail!("Failed to attach to orchestrator: {error}");
            }

            #[cfg(not(unix))]
            {
                let status = Command::new("tmux")
                    .args(["attach", "-t", ORCHESTRATOR_SESSION])
                    .status()
                    .context("Failed to attach to orchestrator")?;
                if !status.success() {
                    bail!("Failed to attach to orchestrator");
                }
                Ok(())
            }
        }
        _ => {
            println!("No orchestrator is currently running.");
            println!();
            println!("To start the orchestrator:  loom run");
            println!("To check status:            loom status");
            Ok(())
        }
    }
}

/// Build execution graph from .work/stages/ files or fall back to plan file
fn build_execution_graph(work_dir: &WorkDir) -> Result<ExecutionGraph> {
    let stages_dir = work_dir.root().join("stages");

    // First try to load from .work/stages/ files
    if stages_dir.exists() {
        let stages = load_stages_from_work_dir(&stages_dir)?;
        if !stages.is_empty() {
            return ExecutionGraph::build(stages)
                .context("Failed to build execution graph from stage files");
        }
    }

    // Fall back to reading from plan file
    let config_path = work_dir.root().join("config.toml");

    if !config_path.exists() {
        bail!("No active plan. Run 'loom init <plan-path>' first.");
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

    let path = PathBuf::from(source_path);

    if !path.exists() {
        bail!(
            "Plan file not found: {}\nThe plan may have been moved or deleted.\n\nNote: Stage files in .work/stages/ can be used instead of the plan file.",
            path.display()
        );
    }

    let parsed_plan =
        parse_plan(&path).with_context(|| format!("Failed to parse plan: {}", path.display()))?;

    ExecutionGraph::build(parsed_plan.stages).context("Failed to build execution graph")
}

/// Load stage definitions from .work/stages/ directory
fn load_stages_from_work_dir(stages_dir: &PathBuf) -> Result<Vec<StageDefinition>> {
    let mut stages = Vec::new();

    for entry in std::fs::read_dir(stages_dir)
        .with_context(|| format!("Failed to read stages directory: {}", stages_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        // Skip non-markdown files
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        // Read and parse the stage file
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read stage file: {}", path.display()))?;

        // Extract YAML frontmatter
        let frontmatter = match extract_stage_frontmatter(&content) {
            Ok(fm) => fm,
            Err(e) => {
                eprintln!("Warning: Could not parse {}: {}", path.display(), e);
                continue;
            }
        };

        // Convert to StageDefinition
        let stage_def = StageDefinition {
            id: frontmatter.id,
            name: frontmatter.name,
            description: frontmatter.description,
            dependencies: frontmatter.dependencies,
            parallel_group: frontmatter.parallel_group,
            acceptance: frontmatter.acceptance,
            setup: frontmatter.setup,
            files: frontmatter.files,
        };

        stages.push(stage_def);
    }

    Ok(stages)
}

/// Stage frontmatter data extracted from .work/stages/*.md files
#[derive(Debug, serde::Deserialize)]
struct StageFrontmatter {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    parallel_group: Option<String>,
    #[serde(default)]
    acceptance: Vec<String>,
    #[serde(default)]
    setup: Vec<String>,
    #[serde(default)]
    files: Vec<String>,
}

/// Extract YAML frontmatter from stage markdown file
fn extract_stage_frontmatter(content: &str) -> Result<StageFrontmatter> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || !lines[0].trim().starts_with("---") {
        bail!("No frontmatter delimiter found");
    }

    let mut end_idx = None;
    for (idx, line) in lines.iter().enumerate().skip(1) {
        if line.trim().starts_with("---") {
            end_idx = Some(idx);
            break;
        }
    }

    let end_idx = end_idx.ok_or_else(|| anyhow::anyhow!("Frontmatter not properly closed"))?;

    let yaml_content = lines[1..end_idx].join("\n");

    serde_yaml::from_str(&yaml_content).context("Failed to parse stage YAML frontmatter")
}

/// Print orchestrator result summary
fn print_result(result: &OrchestratorResult) {
    println!("\n=== Orchestration Complete ===\n");

    if !result.completed_stages.is_empty() {
        println!("✓ Completed stages:");
        for stage in &result.completed_stages {
            println!("  - {stage}");
        }
    }

    if !result.failed_stages.is_empty() {
        println!("\n✗ Failed stages:");
        for stage in &result.failed_stages {
            println!("  - {stage}");
        }
    }

    if !result.needs_handoff.is_empty() {
        println!("\n⚠ Stages needing handoff:");
        for stage in &result.needs_handoff {
            println!("  - {stage}");
        }
        println!("\nRun 'loom resume <stage-id>' to continue these stages.");
    }

    println!(
        "\nTotal sessions spawned: {}",
        result.total_sessions_spawned
    );

    if result.is_success() {
        println!("\n✓ All stages completed successfully!");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::schema::{LoomConfig, LoomMetadata, StageDefinition};
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    fn create_test_plan(dir: &Path, stages: Vec<StageDefinition>) -> PathBuf {
        let metadata = LoomMetadata {
            loom: LoomConfig { version: 1, stages },
        };

        let yaml = serde_yaml::to_string(&metadata).unwrap();
        let plan_content = format!(
            "# Test Plan\n\n## Overview\n\nTest plan\n\n<!-- loom METADATA -->\n```yaml\n{yaml}```\n<!-- END loom METADATA -->\n"
        );

        let plan_path = dir.join("test-plan.md");
        fs::write(&plan_path, plan_content).unwrap();
        plan_path
    }

    fn setup_work_dir_with_plan(temp_dir: &TempDir) -> (PathBuf, WorkDir) {
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        work_dir.initialize().unwrap();

        let stage_def = StageDefinition {
            id: "test-stage".to_string(),
            name: "Test Stage".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
        };

        let plan_path = create_test_plan(temp_dir.path(), vec![stage_def]);

        let config_content = format!(
            "[plan]\nsource_path = \"{}\"\nplan_id = \"test-plan\"\nplan_name = \"Test Plan\"\n",
            plan_path.display()
        );
        fs::write(work_dir.root().join("config.toml"), config_content).unwrap();

        (plan_path, work_dir)
    }

    #[test]
    fn test_extract_stage_frontmatter_valid() {
        let content = r#"---
id: stage-1
name: Test Stage
dependencies: []
acceptance: []
setup: []
files: []
---

# Stage: Test Stage

Content here
"#;

        let result = extract_stage_frontmatter(content);

        assert!(result.is_ok());
        let frontmatter = result.unwrap();
        assert_eq!(frontmatter.id, "stage-1");
        assert_eq!(frontmatter.name, "Test Stage");
        assert_eq!(frontmatter.dependencies.len(), 0);
    }

    #[test]
    fn test_extract_stage_frontmatter_with_fields() {
        let content = r#"---
id: stage-2
name: Complex Stage
description: A complex stage
dependencies:
  - stage-1
parallel_group: core
acceptance:
  - cargo test
setup:
  - cargo build
files:
  - src/*.rs
---

# Stage
"#;

        let result = extract_stage_frontmatter(content);

        assert!(result.is_ok());
        let frontmatter = result.unwrap();
        assert_eq!(frontmatter.id, "stage-2");
        assert_eq!(frontmatter.description, Some("A complex stage".to_string()));
        assert_eq!(frontmatter.dependencies, vec!["stage-1".to_string()]);
        assert_eq!(frontmatter.parallel_group, Some("core".to_string()));
        assert_eq!(frontmatter.acceptance.len(), 1);
        assert_eq!(frontmatter.setup.len(), 1);
        assert_eq!(frontmatter.files.len(), 1);
    }

    #[test]
    fn test_extract_stage_frontmatter_no_delimiter() {
        let content = "No frontmatter here";

        let result = extract_stage_frontmatter(content);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("frontmatter"));
    }

    #[test]
    fn test_extract_stage_frontmatter_not_closed() {
        let content = "---\nid: test\nname: Test\n\nNo closing delimiter";

        let result = extract_stage_frontmatter(content);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not properly closed"));
    }

    #[test]
    fn test_extract_stage_frontmatter_invalid_yaml() {
        let content = "---\ninvalid: yaml: content:\n---\n";

        let result = extract_stage_frontmatter(content);

        assert!(result.is_err());
    }

    #[test]
    fn test_build_execution_graph_no_config() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        work_dir.initialize().unwrap();

        let result = build_execution_graph(&work_dir);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No active plan"));
    }

    #[test]
    fn test_build_execution_graph_from_config() {
        let temp_dir = TempDir::new().unwrap();
        let (_plan_path, work_dir) = setup_work_dir_with_plan(&temp_dir);

        let result = build_execution_graph(&work_dir);

        assert!(result.is_ok());
        let _graph = result.unwrap();
    }

    #[test]
    fn test_build_execution_graph_missing_plan_file() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        work_dir.initialize().unwrap();

        let config_content =
            "[plan]\nsource_path = \"/nonexistent/plan.md\"\nplan_id = \"test\"\nplan_name = \"Test\"\n";
        fs::write(work_dir.root().join("config.toml"), config_content).unwrap();

        let result = build_execution_graph(&work_dir);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_load_stages_from_work_dir_empty() {
        let temp_dir = TempDir::new().unwrap();
        let stages_dir = temp_dir.path().join("stages");
        fs::create_dir(&stages_dir).unwrap();

        let result = load_stages_from_work_dir(&stages_dir);

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_load_stages_from_work_dir_with_stages() {
        let temp_dir = TempDir::new().unwrap();
        let stages_dir = temp_dir.path().join("stages");
        fs::create_dir(&stages_dir).unwrap();

        let stage_content = r#"---
id: stage-1
name: Test Stage
dependencies: []
acceptance: []
setup: []
files: []
---

# Stage: Test Stage
"#;

        fs::write(stages_dir.join("0-stage-1.md"), stage_content).unwrap();

        let result = load_stages_from_work_dir(&stages_dir);

        assert!(result.is_ok());
        let stages = result.unwrap();
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].id, "stage-1");
    }

    #[test]
    fn test_load_stages_from_work_dir_ignores_non_markdown() {
        let temp_dir = TempDir::new().unwrap();
        let stages_dir = temp_dir.path().join("stages");
        fs::create_dir(&stages_dir).unwrap();

        fs::write(stages_dir.join("readme.txt"), "Not a stage").unwrap();

        let result = load_stages_from_work_dir(&stages_dir);

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_load_stages_from_work_dir_skips_invalid() {
        let temp_dir = TempDir::new().unwrap();
        let stages_dir = temp_dir.path().join("stages");
        fs::create_dir(&stages_dir).unwrap();

        let valid_stage = r#"---
id: valid
name: Valid
dependencies: []
---
"#;
        fs::write(stages_dir.join("valid.md"), valid_stage).unwrap();
        fs::write(stages_dir.join("invalid.md"), "Invalid content").unwrap();

        let result = load_stages_from_work_dir(&stages_dir);

        assert!(result.is_ok());
        let stages = result.unwrap();
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].id, "valid");
    }

    #[test]
    fn test_print_result_success() {
        let result = OrchestratorResult {
            completed_stages: vec!["stage-1".to_string(), "stage-2".to_string()],
            failed_stages: vec![],
            needs_handoff: vec![],
            total_sessions_spawned: 2,
        };

        assert!(result.is_success());

        print_result(&result);
    }

    #[test]
    fn test_print_result_with_failures() {
        let result = OrchestratorResult {
            completed_stages: vec!["stage-1".to_string()],
            failed_stages: vec!["stage-2".to_string()],
            needs_handoff: vec![],
            total_sessions_spawned: 2,
        };

        assert!(!result.is_success());

        print_result(&result);
    }

    #[test]
    fn test_print_result_with_handoffs() {
        let result = OrchestratorResult {
            completed_stages: vec![],
            failed_stages: vec![],
            needs_handoff: vec!["stage-1".to_string()],
            total_sessions_spawned: 1,
        };

        print_result(&result);
    }
}
