//! Thin CLI client for `loom stage dispute-criteria`.
//!
//! This command no longer mutates stage state directly. It serialises
//! the dispute into a structured `Request::DisputeCriteria` and sends
//! it over the daemon's Unix socket, through the transport every
//! `loom stage dispute-*` command shares (`dispute_transport.rs`). The daemon
//! writes `<state-dir>/disputes/<stage>/<n>/request.md`, transitions the stage
//! to `NeedsAdjudication`, and returns an allocated id.
//!
//! See `loom/src/daemon/server/dispute.rs` for the server-side handler
//! and `loom/src/models/dispute.rs` for the on-disk schema.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::dispute_transport::{send, Dispute};
use crate::models::dispute::{truncate_to_byte_limit, CriterionField, FAILURE_OUTPUT_MAX_BYTES};
use crate::relay::emit::{mode, EnvSnapshot, RelayMode, RelaySink, StdSink};

/// Dispute entries of a stage's acceptance, wiring or wiring-tests list.
///
/// Reads the process environment exactly once, then delegates to
/// `dispute_criteria_with_mode` — the seam tests drive directly with an
/// explicit [`RelayMode`] and an in-memory sink, since tests must never
/// mutate process-wide environment.
pub fn dispute_criteria(
    stage_id: String,
    field: CriterionField,
    criterion_indices: Vec<usize>,
    reason: String,
    evidence_commit: Option<String>,
    failure_output_path: Option<PathBuf>,
) -> Result<()> {
    let failure_output = match failure_output_path {
        Some(path) => Some(load_and_truncate_failure_output(&path)?),
        None => None,
    };
    let relay_mode = mode(&EnvSnapshot::from_process_env());
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    dispute_criteria_with_mode(
        stage_id,
        field,
        criterion_indices,
        reason,
        evidence_commit,
        failure_output,
        relay_mode,
        &cwd,
        &mut StdSink::default(),
    )
}

/// In Relay mode the request goes to the relay hook; otherwise the daemon
/// socket, as today (see `dispute_transport::send`).
#[allow(clippy::too_many_arguments)]
fn dispute_criteria_with_mode(
    stage_id: String,
    field: CriterionField,
    criterion_indices: Vec<usize>,
    reason: String,
    evidence_commit: Option<String>,
    failure_output: Option<String>,
    relay_mode: RelayMode,
    cwd: &Path,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    let dispute = Dispute::criterion(
        stage_id,
        field,
        criterion_indices,
        reason,
        evidence_commit,
        failure_output,
    );
    send(dispute, relay_mode, cwd, sink)
}

/// Load `failure_output_path` and truncate the contents at the last
/// UTF-8 char boundary that fits within `FAILURE_OUTPUT_MAX_BYTES`
/// (4KB). Avoids the multi-byte panic documented in
/// knowledge/mistakes.md § "String Handling: UTF-8 Truncation Panic".
fn load_and_truncate_failure_output(path: &Path) -> Result<String> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read failure_output file: {}", path.display()))?;
    Ok(truncate_to_byte_limit(&raw, FAILURE_OUTPUT_MAX_BYTES))
}

#[cfg(test)]
#[path = "dispute_criteria_tests.rs"]
mod tests;
