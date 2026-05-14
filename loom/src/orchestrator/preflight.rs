//! Shared `loom run` preflight checks.
//!
//! Both the foreground runner (`commands::run::foreground`) and the
//! daemon (`daemon::server::orchestrator`) need to:
//!
//!   1. Read the persisted project-level backend from
//!      `.work/config.toml::[project_execution]`.
//!   2. Validate every stage's resolved backend up front so a misconfigured
//!      plan fails before the first poll instead of mid-flight.
//!
//! This module exposes those checks as a single
//! [`resolve_project_backend`] entry point so both runners share one
//! implementation.

use anyhow::{Context, Result};
use std::path::Path;

use crate::fs::work_dir as work_dir_api;
use crate::orchestrator::terminal::dispatcher::resolve_stage_backend;
use crate::orchestrator::terminal::BackendType;

/// Resolve the project-level backend from `.work/config.toml`.
///
/// Returns `BackendType::Native` if no `[project_execution]` section
/// exists (untouched projects keep working with the native backend).
/// Returns an error if any stage's backend override is rejected by
/// [`resolve_stage_backend`].
pub fn resolve_project_backend(work_dir: &Path) -> Result<BackendType> {
    let Some(project) = work_dir_api::read_project_execution(work_dir)
        .context("Failed to read [project_execution] from .work/config.toml")?
    else {
        return Ok(BackendType::Native);
    };

    // Validate every stage's resolved backend. We load each stage file
    // (not the graph nodes — graph doesn't carry execution_backend) and
    // exercise the narrowing matrix.
    let stages = crate::verify::transitions::list_all_stages(work_dir)?;
    for stage in stages {
        resolve_stage_backend(project.backend, stage.execution_backend())
            .with_context(|| format!("Stage '{}': backend selection is invalid", stage.id))?;
    }

    Ok(project.backend)
}
