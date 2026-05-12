//! Persistent soft signals for the monitor.
//!
//! Soft signals are advisory notices (e.g. "possibly stuck") that the
//! orchestrator persists to disk so that dedup can survive daemon restarts.
//! Each signal has an `expires_at` timestamp; expired signals are ignored on
//! read. Writers append one JSON line per signal; there is no compaction.

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// How long (in seconds) a soft signal remains active before it expires.
pub const DECAY_WINDOW_SECS: u64 = 120;

/// A single soft advisory signal.
///
/// The `kind` tag is embedded in the serialized JSON, allowing new variants to
/// be added without breaking existing readers (unknown kinds are silently
/// skipped via the malformed-line filter in `read_active`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SoftSignal {
    PossiblyStuck {
        session_id: String,
        stage_id: String,
        recent_events: usize,
        failure_count: u32,
        failure_ratio: f32,
        /// RFC3339 timestamp of when the signal was emitted.
        emitted_at: String,
        /// RFC3339 timestamp after which the signal should be considered expired.
        expires_at: String,
    },
}

fn signals_path(work_dir: &Path) -> std::path::PathBuf {
    work_dir.join("monitor").join("soft-signals.jsonl")
}

/// Append a soft signal as one JSON line to `<work_dir>/monitor/soft-signals.jsonl`.
///
/// The parent directory is created on the first write.
pub fn append(work_dir: &Path, sig: &SoftSignal) -> io::Result<()> {
    let path = signals_path(work_dir);
    // Ensure the parent directory exists.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let line =
        serde_json::to_string(sig).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(file, "{line}")
}

/// Read all non-expired soft signals from `<work_dir>/monitor/soft-signals.jsonl`.
///
/// - If the file does not exist, returns `Ok(vec![])`.
/// - Blank lines and malformed JSON are silently skipped.
/// - Signals whose `expires_at` is ≤ `now` are filtered out.
pub fn read_active(work_dir: &Path, now: SystemTime) -> io::Result<Vec<SoftSignal>> {
    let path = signals_path(work_dir);
    if !path.exists() {
        return Ok(vec![]);
    }

    let now_dt: DateTime<Utc> = now.into();
    let file = fs::File::open(&path)?;
    let reader = io::BufReader::new(file);
    let mut active = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let sig: SoftSignal = match serde_json::from_str(trimmed) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("soft-signals.jsonl: skipping malformed line: {e}");
                continue;
            }
        };
        // Keep only non-expired signals.
        let expires_str = match &sig {
            SoftSignal::PossiblyStuck { expires_at, .. } => expires_at.as_str(),
        };
        match DateTime::parse_from_rfc3339(expires_str) {
            Ok(expires_dt) => {
                if now_dt < expires_dt {
                    active.push(sig);
                }
            }
            Err(e) => {
                tracing::warn!(
                    "soft-signals.jsonl: skipping signal with unparseable expires_at '{}': {e}",
                    expires_str
                );
            }
        }
    }

    Ok(active)
}

/// Read all non-expired soft signals for a specific session.
pub fn read_active_for_session(
    work_dir: &Path,
    now: SystemTime,
    session_id: &str,
) -> io::Result<Vec<SoftSignal>> {
    let all = read_active(work_dir, now)?;
    Ok(all
        .into_iter()
        .filter(|s| match s {
            SoftSignal::PossiblyStuck {
                session_id: sid, ..
            } => sid == session_id,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_signal(session_id: &str, stage_id: &str, expires_offset_secs: i64) -> SoftSignal {
        let now = Utc::now();
        let emitted_at = now.to_rfc3339();
        let expires_at = (now + chrono::Duration::seconds(expires_offset_secs)).to_rfc3339();
        SoftSignal::PossiblyStuck {
            session_id: session_id.to_string(),
            stage_id: stage_id.to_string(),
            recent_events: 10,
            failure_count: 9,
            failure_ratio: 0.9,
            emitted_at,
            expires_at,
        }
    }

    // -----------------------------------------------------------------------
    // Test 1: round-trip append → read_active
    // -----------------------------------------------------------------------
    #[test]
    fn append_and_read_round_trip() {
        let dir = TempDir::new().unwrap();
        // expires in 200 seconds → should be active
        let sig = make_signal("sess-1", "stage-1", 200);
        append(dir.path(), &sig).unwrap();

        let active = read_active(dir.path(), SystemTime::now()).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0], sig);
    }

    // -----------------------------------------------------------------------
    // Test 2: expired signals are filtered out
    // -----------------------------------------------------------------------
    #[test]
    fn decay_filtering() {
        let dir = TempDir::new().unwrap();
        // expires 1 second in the past → should be filtered
        let sig = make_signal("sess-2", "stage-2", -1);
        append(dir.path(), &sig).unwrap();

        let active = read_active(dir.path(), SystemTime::now()).unwrap();
        assert!(active.is_empty(), "expired signal should be filtered out");
    }

    // -----------------------------------------------------------------------
    // Test 3: missing file returns empty vec
    // -----------------------------------------------------------------------
    #[test]
    fn missing_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let active = read_active(dir.path(), SystemTime::now()).unwrap();
        assert!(active.is_empty());
    }

    // -----------------------------------------------------------------------
    // Test 4: malformed lines are skipped
    // -----------------------------------------------------------------------
    #[test]
    fn malformed_lines_skipped() {
        let dir = TempDir::new().unwrap();

        // Write the file manually: one malformed line followed by one valid signal.
        let monitor_dir = dir.path().join("monitor");
        fs::create_dir_all(&monitor_dir).unwrap();
        let path = monitor_dir.join("soft-signals.jsonl");
        let valid_sig = make_signal("sess-3", "stage-3", 200);
        let valid_json = serde_json::to_string(&valid_sig).unwrap();

        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "notjson").unwrap();
        writeln!(f, "{valid_json}").unwrap();

        let active = read_active(dir.path(), SystemTime::now()).unwrap();
        assert_eq!(active.len(), 1, "malformed line should be skipped");
        assert_eq!(active[0], valid_sig);
    }
}
