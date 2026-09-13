//! Best-effort orchestration and context telemetry.
//!
//! Events normally append to `.loom/work/telemetry/events.jsonl`. Sandboxed
//! stage sessions cannot write through the worktree's state-root symlink, so a
//! denied direct write falls back to a per-worktree spool which the daemon
//! drains into the canonical event file.

use std::fs::OpenOptions;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::relay::emit::{mode, EnvSnapshot, RelayContext, RelayMode, RelaySink, StdSink};
use crate::relay::RequestKind;

pub mod spool;
pub mod summary;
pub use spool::TELEMETRY_SPOOL_RELPATH;

/// One recorded orchestration fact. Counts are estimates, never savings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TelemetryEvent {
    ContextDelivered {
        stage_id: String,
        session_id: String,
        context_epoch: String,
        items: usize,
    },
    ContextUnavailable {
        stage_id: String,
        session_id: String,
        reason: String,
    },
    PromptBrief {
        stage_id: Option<String>,
        session_id: Option<String>,
        items: usize,
        estimated_tokens: usize,
        omitted: usize,
    },
    PromptAbstained {
        stage_id: Option<String>,
        session_id: Option<String>,
        reason: String,
    },
    ContextPulled {
        stage_id: Option<String>,
        session_id: Option<String>,
        query_chars: usize,
        budget_tokens: usize,
        items: usize,
        estimated_tokens: usize,
        unmet_required: usize,
    },
}

/// A timestamped event as stored in the telemetry JSON-lines files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryRecord {
    pub at: DateTime<Utc>,
    #[serde(flatten)]
    pub event: TelemetryEvent,
}

pub(crate) fn events_path(work_dir: &Path) -> PathBuf {
    work_dir.join("telemetry").join("events.jsonl")
}

/// Append `event` to the canonical event file, or its worktree spool when the
/// state root is write-denied — or, in Relay mode, relay it instead.
///
/// Best-effort by contract: telemetry must never fail a caller, so failures on
/// every path are logged at debug level (or, in Relay mode, silently
/// swallowed) and returned as success. Reads the process environment exactly
/// once, then delegates to `emit_with_mode` — the seam tests drive directly
/// with an explicit [`RelayMode`] and an in-memory sink, since tests must
/// never mutate process-wide environment.
pub fn emit(work_dir: &Path, event: &TelemetryEvent) -> Result<()> {
    let relay_mode = mode(&EnvSnapshot::from_process_env());
    let cwd = std::env::current_dir().unwrap_or_default();
    emit_with_mode(work_dir, event, relay_mode, &cwd, &mut StdSink::default())
}

fn emit_with_mode(
    work_dir: &Path,
    event: &TelemetryEvent,
    relay_mode: RelayMode,
    cwd: &Path,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    if let RelayMode::Relay(context) = relay_mode {
        relay_telemetry(event, &context, cwd, sink);
        return Ok(());
    }

    let record = TelemetryRecord {
        at: Utc::now(),
        event: event.clone(),
    };
    if let Err(error) = append_record(work_dir, &record) {
        if spool::is_write_denied(&error) {
            if let Err(spool_error) = spool_denied_record(&record) {
                tracing::debug!(%error, %spool_error, "failed to spool telemetry event");
            }
        } else {
            tracing::debug!(%error, "failed to record telemetry event");
        }
    }
    Ok(())
}

/// Best-effort relay of a telemetry event: a `check` refusal or a
/// serialization/emit error is swallowed, matching `emit`'s contract that a
/// telemetry failure never undoes the work that already succeeded.
fn relay_telemetry(
    event: &TelemetryEvent,
    context: &RelayContext,
    cwd: &Path,
    sink: &mut dyn RelaySink,
) {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    if context
        .check(RequestKind::Telemetry, None, cwd, uid)
        .is_err()
    {
        return;
    }
    let Ok(payload) = serde_json::to_value(event) else {
        return;
    };
    let _ = context.emit_quiet(RequestKind::Telemetry, payload, sink);
}

/// Append one already-timestamped record under an exclusive lock.
pub(crate) fn append_record(work_dir: &Path, record: &TelemetryRecord) -> Result<()> {
    let path = events_path(work_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create telemetry directory: {}", parent.display())
        })?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("Failed to open telemetry events: {}", path.display()))?;
    file.lock_exclusive()
        .with_context(|| format!("Failed to lock telemetry events: {}", path.display()))?;
    let line = serde_json::to_string(record).context("Failed to serialize telemetry record")?;
    writeln!(file, "{line}")
        .with_context(|| format!("Failed to append telemetry event: {}", path.display()))?;
    Ok(())
}

fn spool_denied_record(record: &TelemetryRecord) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let worktree_root = crate::git::worktree::find_worktree_root_from_cwd(&cwd)
        .context("Telemetry write was denied outside a stage worktree")?;
    spool::append_to_spool(&worktree_root, record)
}

/// Read every well-formed event record, skipping malformed lines.
pub fn read_events(work_dir: &Path) -> Result<Vec<TelemetryRecord>> {
    let path = events_path(work_dir);
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to open telemetry events: {}", path.display()))
        }
    };
    file.lock_shared()
        .with_context(|| format!("Failed to lock telemetry events: {}", path.display()))?;
    let mut content = String::new();
    (&file)
        .read_to_string(&mut content)
        .with_context(|| format!("Failed to read telemetry events: {}", path.display()))?;
    Ok(content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
