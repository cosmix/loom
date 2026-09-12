//! Sandbox settings enforcement for a stage spawn.

use anyhow::{Context, Result};

/// Write Claude Code's OS-level sandbox settings for a stage spawn, then warn
/// about any `allow_write` grant the session sandbox will not honor because
/// the path does not exist on the host at session start (see
/// `sandbox::warn_missing_grants`). Reached by both spawn paths — worktree
/// stages and knowledge stages — so this is the single place that check runs.
pub(super) fn write_required_sandbox_settings(
    config: &crate::sandbox::MergedSandboxConfig,
    target: &std::path::Path,
    stage_id: &str,
) -> Result<()> {
    crate::sandbox::write_settings(config, target)
        .with_context(|| format!("Failed to enforce sandbox settings for stage '{stage_id}'"))?;
    crate::sandbox::warn_missing_grants(config, stage_id);
    Ok(())
}
