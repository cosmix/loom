//! The confinement level and network a stage's plan-authored commands get:
//! stage overrides over plan defaults.

use std::path::Path;

use crate::models::stage::{CommandConfinement, Stage};

use super::host::NetworkAccess;

/// Whether the plan's `[plan_sandbox]` snapshot in `work_dir` sets
/// `network.allow_local_binding`. A missing or unreadable snapshot yields
/// `false`, the fail-safe level, as [`plan_confinement`] does.
pub fn plan_local_binding(work_dir: &Path) -> bool {
    crate::fs::work_dir::read_plan_sandbox(work_dir)
        .ok()
        .flatten()
        .is_some_and(|sandbox| sandbox.network.allow_local_binding)
}

/// The network a stage's confined verification commands get: loopback when
/// the stage's merged `sandbox.network.allow_local_binding` is set, else none.
/// A stage's own `network` section replaces the plan's whole, as
/// [`crate::sandbox::merge_config`] merges it; `plan_local_binding` is the
/// plan's value ([`plan_local_binding`]).
pub(crate) fn stage_network(stage: &Stage, plan_local_binding: bool) -> NetworkAccess {
    let allowed = stage
        .sandbox
        .network
        .as_ref()
        .map_or(plan_local_binding, |network| network.allow_local_binding);
    if allowed {
        NetworkAccess::Loopback
    } else {
        NetworkAccess::Denied
    }
}

/// Effective confinement for one stage's plan-authored commands.
///
/// An explicit stage-level override wins over the plan-level default; with
/// neither set, commands are [`CommandConfinement::Confined`]. This is the
/// confinement-only projection of [`crate::sandbox::merge_config`]'s
/// stage-over-plan precedence, for the callers that hold the two values
/// without a full `SandboxConfig`.
pub fn resolve_confinement(
    stage_override: Option<CommandConfinement>,
    plan_default: Option<CommandConfinement>,
) -> CommandConfinement {
    stage_override.or(plan_default).unwrap_or_default()
}

/// Plan-level confinement default, read from the `[plan_sandbox]` snapshot
/// that `loom init`/`loom run` persist in `.loom/work/config.toml`.
///
/// A missing or unreadable snapshot yields `None`, leaving callers at the
/// `Confined` default — ambiguity resolves toward the fail-safe level.
pub fn plan_confinement(work_dir: &Path) -> Option<CommandConfinement> {
    crate::fs::work_dir::read_plan_sandbox(work_dir)
        .ok()
        .flatten()
        .map(|sandbox| sandbox.command_confinement)
}
