//! Per-stage container network management.
//!
//! **STUB.** Full implementation lands in stage 4 (firewall + allowlist
//! sidecar). For now the public entry points let stage 3 wire the call
//! sites without requiring stage-4 work, while preventing accidental use of
//! the unfinished allowlist writer.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use super::runtime::Runtime;
use crate::plan::schema::NetworkConfig;

/// Ensure a per-stage Docker/Podman network exists.
///
/// Stage 3 stub: returns Ok unconditionally so spawn paths can call this
/// without the firewall sidecar being implemented yet.
pub fn ensure_network(_runtime: &Runtime, _stage_id: &str) -> Result<()> {
    Ok(())
}

/// Materialise the host-side allowlist file that the firewall sidecar will
/// mount read-only into the container at `/etc/loom/network/allowed_domains.txt`.
///
/// Stage 3 stub: returns an explicit `bail!` so any caller that tries to
/// use the unfinished implementation fails loudly with a pointer to stage 4.
pub fn write_allowlist(_work_dir: &Path, _network: &NetworkConfig) -> Result<PathBuf> {
    bail!("network::write_allowlist not implemented until stage 4")
}

/// Remove the per-stage container network. Stage 3 stub: no-op.
pub fn remove_network(_runtime: &Runtime, _stage_id: &str) -> Result<()> {
    Ok(())
}
