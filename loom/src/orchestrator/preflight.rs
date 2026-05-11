//! Shared `loom run` preflight checks.
//!
//! Both the foreground runner (`commands::run::foreground`) and the
//! daemon (`daemon::server::orchestrator`) need to:
//!
//!   1. Read the persisted project-level backend from
//!      `.work/config.toml::[project_execution]`.
//!   2. Reject a `pending` image digest (means `loom init --backend
//!      container` was never run).
//!   3. Validate every stage's resolved backend up front so a misconfigured
//!      plan fails before the first poll instead of mid-flight.
//!
//! This module exposes those checks as a single
//! [`resolve_project_backend`] entry point so both runners share one
//! implementation.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::fs::work_dir as work_dir_api;
use crate::orchestrator::terminal::dispatcher::resolve_stage_backend;
use crate::orchestrator::terminal::BackendType;

/// Resolve the project-level backend from `.work/config.toml`.
///
/// Returns `BackendType::Native` if no `[project_execution]` section
/// exists (back-compat: untouched projects keep working with the native
/// backend). Returns an error if the container backend is selected but
/// no image has been pinned, or if any stage's backend override is
/// rejected by [`resolve_stage_backend`].
pub fn resolve_project_backend(work_dir: &Path) -> Result<BackendType> {
    let Some(project) = work_dir_api::read_project_execution(work_dir)
        .context("Failed to read [project_execution] from .work/config.toml")?
    else {
        return Ok(BackendType::Native);
    };

    if project.backend == BackendType::Container {
        let container = project.container.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Project backend is set to container but [project_execution.container] is missing. \
                 Run `loom init --backend container` to provision the container image."
            )
        })?;
        let digest = container.image_digest.trim();
        if digest.is_empty() || digest == "pending" {
            bail!(
                "Project container backend has no pinned image digest \
                 (image_digest = '{digest}'). Run `loom init --backend container` to \
                 build and pin the image before `loom run`."
            );
        }
    }

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
