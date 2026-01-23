//! Orchestrator spawning and execution graph building.

use super::super::protocol::DaemonConfig;
use super::core::DaemonServer;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::orchestrator::terminal::BackendType;
use crate::orchestrator::{Orchestrator, OrchestratorConfig};
use crate::plan::graph::ExecutionGraph;
use crate::plan::parser::parse_plan;
use crate::plan::schema::StageDefinition;

/// Parse base_branch from config.toml
fn parse_base_branch_from_config(work_dir: &Path) -> Result<Option<String>> {
    let config_path = work_dir.join("config.toml");

    if !config_path.exists() {
        return Ok(None);
    }

    let config_content = fs::read_to_string(&config_path).context("Failed to read config.toml")?;

    let config: toml::Value =
        toml::from_str(&config_content).context("Failed to parse config.toml")?;

    let base_branch = config
        .get("plan")
        .and_then(|p| p.get("base_branch"))
        .and_then(|b| b.as_str())
        .map(String::from);

    Ok(base_branch)
}

/// Spawn the orchestrator thread to execute stages.
///
/// Returns a join handle for the orchestrator thread.
pub fn spawn_orchestrator(server: &DaemonServer) -> Option<JoinHandle<()>> {
    let work_dir = server.work_dir.clone();
    let daemon_config = server.config.clone();
    let shutdown_flag = Arc::clone(&server.shutdown_flag);

    Some(thread::spawn(move || {
        if let Err(e) = run_orchestrator(&work_dir, &daemon_config, shutdown_flag) {
            eprintln!("Orchestrator error: {e}");
        }
    }))
}

/// Run the orchestrator loop (static method for thread).
fn run_orchestrator(
    work_dir: &Path,
    daemon_config: &DaemonConfig,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<()> {
    // Build execution graph from stage files
    let graph = build_execution_graph(work_dir)?;

    // Get repo root (parent of .work/)
    let repo_root = work_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    // Parse base_branch from config.toml
    let base_branch = match parse_base_branch_from_config(work_dir) {
        Ok(branch) => {
            if let Some(ref b) = branch {
                eprintln!("Loaded base_branch from config: {b}");
            } else {
                eprintln!("Warning: No base_branch in config.toml, will use default_branch()");
            }
            branch
        }
        Err(e) => {
            eprintln!("Warning: Failed to parse base_branch from config.toml: {e}");
            None
        }
    };

    // Configure orchestrator using daemon config
    let config = OrchestratorConfig {
        max_parallel_sessions: daemon_config.max_parallel.unwrap_or(4),
        poll_interval: Duration::from_secs(5),
        manual_mode: daemon_config.manual_mode,
        watch_mode: daemon_config.watch_mode,
        work_dir: work_dir.to_path_buf(),
        repo_root,
        status_update_interval: Duration::from_secs(30),
        backend_type: BackendType::Native,
        auto_merge: daemon_config.auto_merge,
        base_branch,
        skills_dir: None, // Use default ~/.claude/skills/
        enable_skill_routing: true,
        max_skill_recommendations: 5,
    };

    // Create and run orchestrator
    let mut orchestrator =
        Orchestrator::new(config, graph).context("Failed to create orchestrator")?;

    println!("Orchestrator started, spawning ready stages...");

    // Check shutdown flag before starting
    if shutdown_flag.load(Ordering::Relaxed) {
        println!("Orchestrator shutdown requested before start");
        return Ok(());
    }

    // Run orchestrator - it runs its own loop internally and returns when complete
    let result = orchestrator.run();

    match result {
        Ok(result) => {
            if !result.completed_stages.is_empty() {
                println!("Completed stages: {}", result.completed_stages.join(", "));
            }
            if !result.failed_stages.is_empty() {
                println!("Failed stages: {}", result.failed_stages.join(", "));
            }
            if result.is_success() {
                println!("All stages completed successfully");
            }

            // Write completion marker file to signal broadcaster
            write_completion_marker(work_dir);
        }
        Err(e) => {
            eprintln!("Orchestrator run error: {e}");
            // Still write completion marker on error so clients know orchestration stopped
            write_completion_marker(work_dir);
        }
    }

    Ok(())
}

/// Write a completion marker file to signal that orchestration has finished.
///
/// The status broadcaster detects this file and sends OrchestrationComplete
/// to all subscribers.
fn write_completion_marker(work_dir: &Path) {
    let marker_path = work_dir.join("orchestrator.complete");
    if let Err(e) = fs::write(&marker_path, chrono::Utc::now().to_rfc3339()) {
        eprintln!("Failed to write completion marker: {e}");
    }
}

/// Build execution graph from .work/stages/ files.
pub(super) fn build_execution_graph(work_dir: &Path) -> Result<ExecutionGraph> {
    let stages_dir = work_dir.join("stages");

    if stages_dir.exists() {
        let stages = load_stages_from_work_dir(&stages_dir)?;
        if !stages.is_empty() {
            return ExecutionGraph::build(stages)
                .context("Failed to build execution graph from stage files");
        }
    }

    // Fall back to reading from plan file
    let config_path = work_dir.join("config.toml");

    if !config_path.exists() {
        anyhow::bail!("No active plan. Run 'loom init <plan-path>' first.");
    }

    let config_content = fs::read_to_string(&config_path).context("Failed to read config.toml")?;

    let config: toml::Value =
        toml::from_str(&config_content).context("Failed to parse config.toml")?;

    let source_path = config
        .get("plan")
        .and_then(|p| p.get("source_path"))
        .and_then(|s| s.as_str())
        .ok_or_else(|| anyhow::anyhow!("No 'plan.source_path' found in config.toml"))?;

    let path = PathBuf::from(source_path);

    if !path.exists() {
        anyhow::bail!(
            "Plan file not found: {}\nThe plan may have been moved or deleted.",
            path.display()
        );
    }

    let parsed_plan =
        parse_plan(&path).with_context(|| format!("Failed to parse plan: {}", path.display()))?;

    ExecutionGraph::build(parsed_plan.stages).context("Failed to build execution graph")
}

/// Load stage definitions from .work/stages/ directory.
fn load_stages_from_work_dir(stages_dir: &Path) -> Result<Vec<StageDefinition>> {
    let mut stages = Vec::new();

    for entry in fs::read_dir(stages_dir)
        .with_context(|| format!("Failed to read stages directory: {}", stages_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        // Skip non-markdown files
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        // Read and parse the stage file
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read stage file: {}", path.display()))?;

        // Extract YAML frontmatter
        let frontmatter = match extract_stage_frontmatter(&content) {
            Ok(fm) => fm,
            Err(e) => {
                eprintln!("Warning: Could not parse {}: {}", path.display(), e);
                continue;
            }
        };

        stages.push(frontmatter);
    }

    Ok(stages)
}

/// Extract stage definition from YAML frontmatter.
fn extract_stage_frontmatter(content: &str) -> Result<StageDefinition> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || !lines[0].trim().starts_with("---") {
        anyhow::bail!("No frontmatter delimiter found");
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

    #[derive(serde::Deserialize)]
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
        #[serde(default)]
        working_dir: Option<String>,
    }

    let fm: StageFrontmatter =
        serde_yaml::from_str(&yaml_content).context("Failed to parse stage YAML frontmatter")?;

    Ok(StageDefinition {
        id: fm.id,
        name: fm.name,
        description: fm.description,
        dependencies: fm.dependencies,
        parallel_group: fm.parallel_group,
        acceptance: fm.acceptance,
        setup: fm.setup,
        files: fm.files,
        auto_merge: None,
        working_dir: fm.working_dir.unwrap_or_else(|| ".".to_string()),
        stage_type: crate::plan::schema::StageType::default(),
    })
}
