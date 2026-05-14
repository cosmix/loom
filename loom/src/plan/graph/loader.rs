//! Execution graph loading from .work/stages/ or plan file.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::fs::work_dir::{self, WorkDir};
use crate::plan::graph::ExecutionGraph;
use crate::plan::parser::parse_plan;
use crate::plan::schema::{SandboxConfig, StageDefinition};

/// Build execution graph from .work/stages/ files or fall back to plan file.
///
/// This function accepts either a `WorkDir` reference or a raw `Path` to the .work directory.
/// It first attempts to load stages from .work/stages/ files, and if none are found,
/// falls back to loading from the plan file referenced in config.toml.
///
/// Returns the execution graph and the plan-level sandbox configuration. When loading
/// from stage files (which don't carry plan-level config), `SandboxConfig::default()` is returned.
pub fn build_execution_graph(work_dir: impl AsWorkDir) -> Result<(ExecutionGraph, SandboxConfig)> {
    work_dir.build_graph()
}

/// Trait to allow both `&WorkDir` and `&Path` to work with build_execution_graph.
///
/// This enables the function to be used from both the `loom run` command (which has WorkDir)
/// and the daemon orchestrator (which works with raw paths).
pub trait AsWorkDir {
    fn build_graph(&self) -> Result<(ExecutionGraph, SandboxConfig)>;
}

impl AsWorkDir for WorkDir {
    fn build_graph(&self) -> Result<(ExecutionGraph, SandboxConfig)> {
        build_graph_impl(self.root(), Some(self))
    }
}

impl AsWorkDir for &WorkDir {
    fn build_graph(&self) -> Result<(ExecutionGraph, SandboxConfig)> {
        build_graph_impl(self.root(), Some(*self))
    }
}

impl AsWorkDir for Path {
    fn build_graph(&self) -> Result<(ExecutionGraph, SandboxConfig)> {
        build_graph_impl(self, None)
    }
}

impl AsWorkDir for &Path {
    fn build_graph(&self) -> Result<(ExecutionGraph, SandboxConfig)> {
        build_graph_impl(self, None)
    }
}

/// Internal implementation that handles both WorkDir and Path cases.
fn build_graph_impl(
    work_dir_path: &Path,
    work_dir: Option<&WorkDir>,
) -> Result<(ExecutionGraph, SandboxConfig)> {
    let stages_dir = work_dir_path.join("stages");

    // First try to load from .work/stages/ files. Stage files don't carry
    // plan-level sandbox config, but `loom init` persists the plan-level
    // sandbox snapshot to .work/config.toml under [plan_sandbox] so we can
    // recover it on subsequent `loom run` invocations. Without this, the
    // loader silently substitutes `SandboxConfig::default()` and any
    // plan-declared sandbox rules are lost on restart.
    if stages_dir.exists() {
        let stages = load_stages_from_stages_dir(&stages_dir)?;
        if !stages.is_empty() {
            let graph = ExecutionGraph::build(stages)
                .context("Failed to build execution graph from stage files")?;
            let sandbox = work_dir::read_plan_sandbox(work_dir_path)
                .context("Failed to read persisted plan sandbox config")?
                .unwrap_or_default();
            return Ok((graph, sandbox));
        }
    }

    // Fall back to reading from plan file
    load_graph_from_plan_file(work_dir_path, work_dir)
}

/// Load execution graph from the plan file referenced in config.toml.
fn load_graph_from_plan_file(
    work_dir_path: &Path,
    work_dir: Option<&WorkDir>,
) -> Result<(ExecutionGraph, SandboxConfig)> {
    // Load config - use WorkDir method if available, otherwise use fs module directly
    let config = if let Some(wd) = work_dir {
        wd.load_config_required()?
    } else {
        crate::fs::load_config_required(work_dir_path)?
    };

    let source_path = config
        .source_path()
        .ok_or_else(|| anyhow::anyhow!("No 'plan.source_path' found in config.toml"))?;

    if !source_path.exists() {
        bail!(
            "Plan file not found: {}\nThe plan may have been moved or deleted.\n\nNote: Stage files in .work/stages/ can be used instead of the plan file.",
            source_path.display()
        );
    }

    let parsed_plan = parse_plan(&source_path)
        .with_context(|| format!("Failed to parse plan: {}", source_path.display()))?;

    let sandbox = parsed_plan.metadata.loom.sandbox.clone();
    let graph =
        ExecutionGraph::build(parsed_plan.stages).context("Failed to build execution graph")?;
    Ok((graph, sandbox))
}

/// Load stage definitions from .work/stages/ directory.
///
/// This function reads all .md files in the stages directory, extracts their YAML frontmatter,
/// and converts them to StageDefinition objects.
fn load_stages_from_stages_dir(stages_dir: &Path) -> Result<Vec<StageDefinition>> {
    // Delegate to the shared implementation in fs module
    crate::fs::load_stages_from_work_dir(stages_dir)
}
