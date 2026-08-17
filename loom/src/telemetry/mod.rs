//! Best-effort orchestration telemetry.
//!
//! One append-only JSON-lines file (`.work/telemetry/events.jsonl`) recording
//! orchestration facts — currently, whether a spawned session received a
//! context brief. Telemetry is an optimisation surface, never state the run
//! depends on: [`emit`] must never fail a spawn, and [`read_events`] must
//! never let one malformed line make the whole file unreadable.
//!
//! Every count here is an item count, never a token estimate framed as a
//! saving — see the `TelemetryEvent` variants below.
//!
//! [`read_events`] is the store's read half. It has no production call site
//! today — only its own unit tests exercise it — and `.work/` is deleted at
//! plan completion, so every event currently written here goes unread. A
//! future diagnostic (e.g. a `loom status`/`loom map` report on how often
//! stages spawn without a context brief) would read events back through
//! this function.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// One recorded orchestration fact. Counts are estimates, never savings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TelemetryEvent {
    /// A stage session was spawned with a context brief attached.
    ContextDelivered {
        stage_id: String,
        session_id: String,
        context_epoch: String,
        /// Units delivered. A count of items, never a claim about tokens.
        items: usize,
    },
    /// A stage session was spawned with no brief, and why.
    ContextUnavailable {
        stage_id: String,
        session_id: String,
        reason: String,
    },
}

fn events_path(work_dir: &Path) -> PathBuf {
    work_dir.join("telemetry").join("events.jsonl")
}

/// Append `event` to `.work/telemetry/events.jsonl`.
///
/// Best-effort by contract: telemetry must never fail a spawn, so any I/O
/// problem is logged at `tracing::debug` and reported as success.
pub fn emit(work_dir: &Path, event: &TelemetryEvent) -> Result<()> {
    if let Err(error) = emit_inner(work_dir, event) {
        tracing::debug!(%error, "failed to record telemetry event");
    }
    Ok(())
}

fn emit_inner(work_dir: &Path, event: &TelemetryEvent) -> Result<()> {
    let path = events_path(work_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(event)?;
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(file, "{line}")?;
    Ok(())
}

/// Read every well-formed event back, skipping malformed lines.
pub fn read_events(work_dir: &Path) -> Result<Vec<TelemetryEvent>> {
    let path = events_path(work_dir);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    Ok(content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn delivered(stage_id: &str) -> TelemetryEvent {
        TelemetryEvent::ContextDelivered {
            stage_id: stage_id.to_string(),
            session_id: "session-1".to_string(),
            context_epoch: "abc123".to_string(),
            items: 3,
        }
    }

    #[test]
    fn round_trips_a_single_event() {
        let temp = TempDir::new().unwrap();
        let event = delivered("stage-a");
        emit(temp.path(), &event).unwrap();

        let events = read_events(temp.path()).unwrap();
        assert_eq!(events, vec![event]);
    }

    #[test]
    fn appends_multiple_events_in_order() {
        let temp = TempDir::new().unwrap();
        let first = delivered("stage-a");
        let second = TelemetryEvent::ContextUnavailable {
            stage_id: "stage-b".to_string(),
            session_id: "session-2".to_string(),
            reason: "no delivery record for this session".to_string(),
        };
        emit(temp.path(), &first).unwrap();
        emit(temp.path(), &second).unwrap();

        let events = read_events(temp.path()).unwrap();
        assert_eq!(events, vec![first, second]);
    }

    #[test]
    fn read_events_returns_empty_when_file_missing() {
        let temp = TempDir::new().unwrap();
        assert_eq!(read_events(temp.path()).unwrap(), Vec::new());
    }

    #[test]
    fn a_malformed_line_is_skipped_not_fatal() {
        let temp = TempDir::new().unwrap();
        let event = delivered("stage-a");
        emit(temp.path(), &event).unwrap();

        // Append a corrupt tail line by hand.
        let path = events_path(temp.path());
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{{not valid json").unwrap();

        let events = read_events(temp.path()).unwrap();
        assert_eq!(
            events,
            vec![event],
            "the malformed tail line must be skipped, not fail the whole read"
        );
    }

    #[test]
    fn emit_on_an_unwritable_directory_returns_ok() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let original = std::fs::metadata(temp.path()).unwrap().permissions();
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o500)).unwrap();

        let result = emit(temp.path(), &delivered("stage-a"));

        // Restore write permission before the TempDir tries to clean itself up,
        // regardless of what the assertion below does.
        std::fs::set_permissions(temp.path(), original).unwrap();

        assert!(
            result.is_ok(),
            "telemetry must never fail a spawn, even when its directory is unwritable"
        );
    }
}
