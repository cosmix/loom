//! Best-effort orchestration and context telemetry.
//!
//! Events normally append to `.loom/work/telemetry/events.jsonl`. Sandboxed
//! stage sessions cannot write through the worktree's state-root symlink, so a
//! denied direct write falls back to a per-worktree spool which the daemon
//! drains into the canonical event file.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::fs::safe_fs::safe_open_append;

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

const TELEMETRY_DIR: &str = "telemetry";
const EVENTS_FILE: &str = "events.jsonl";

pub(crate) fn events_path(work_dir: &Path) -> PathBuf {
    work_dir.join(TELEMETRY_DIR).join(EVENTS_FILE)
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

/// Append one already-timestamped record under an exclusive lock. The events
/// file is created with mode 0600 and must be a plain, single-link file; the
/// append is anchored at the telemetry directory, so a worktree's
/// `.loom/work` symlink above it is followed.
pub(crate) fn append_record(work_dir: &Path, record: &TelemetryRecord) -> Result<()> {
    let dir = work_dir.join(TELEMETRY_DIR);
    let path = dir.join(EVENTS_FILE);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create telemetry directory: {}", dir.display()))?;
    let mut line = serde_json::to_vec(record).context("Failed to serialize telemetry record")?;
    line.push(b'\n');
    let mut file = safe_open_append(&dir, Path::new(EVENTS_FILE))
        .with_context(|| format!("Failed to open telemetry events: {}", path.display()))?;
    file.write_all(&line)
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
    // Uncapped and locked, unlike `jsonl_spool::read_complete_lines`: this log only grows, so a cap would turn its size into a read failure.
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
