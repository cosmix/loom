//! Execution graph loading from .work/stages/ or plan file.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use crate::fs::work_dir::WorkDir;
use crate::plan::graph::ExecutionGraph;
use crate::plan::parser::parse_plan;

use super::frontmatter::load_stages_from_work_dir;

/// Build execution graph from .work/stages/ files or fall back to plan file
pub fn build_execution_graph(work_dir: &WorkDir) -> Result<ExecutionGraph> {
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
    load_graph_from_plan_file(work_dir)
}

/// Load execution graph from the plan file referenced in config.toml
fn load_graph_from_plan_file(work_dir: &WorkDir) -> Result<ExecutionGraph> {
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
