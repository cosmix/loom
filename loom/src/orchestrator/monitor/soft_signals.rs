//! Persistent soft signals for the monitor.
//!
//! Soft signals are advisory notices that the
//! orchestrator persists to disk so that dedup can survive daemon restarts.
//! Each signal has an `expires_at` timestamp; expired signals are ignored on
//! read. Writers append one JSON line per signal; there is no compaction.

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single soft advisory signal.
///
/// The `kind` field is embedded in the serialized JSON so future advisory
/// signals can be added without changing the on-disk envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoftSignal {
    /// Signal kind, reserved for future advisory signals.
    pub kind: String,
    /// RFC3339 timestamp after which the signal should be considered expired.
    pub expires_at: String,
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
        let expires_str = sig.expires_at.as_str();
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

/// Compact `soft-signals.jsonl` in place, keeping only non-expired rows.
///
/// The file is append-only and never pruned during normal operation, so over a
/// long daemon run it accumulates expired rows that every `read_active` call
/// must parse and discard. Calling this once at daemon startup rewrites the
/// file with only the rows that are still active relative to `now`, bounding
/// reader cost. No-op when the file is missing. The rewrite is atomic
/// (temp file + rename) so a crash mid-compaction cannot truncate the log.
pub fn compact(work_dir: &Path, now: SystemTime) -> io::Result<()> {
    let path = signals_path(work_dir);
    if !path.exists() {
        return Ok(());
    }

    let active = read_active(work_dir, now)?;

    let mut body = String::new();
    for sig in &active {
        let line = serde_json::to_string(sig)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        body.push_str(&line);
        body.push('\n');
    }

    let tmp_path = path.with_extension("jsonl.tmp");
    {
        let mut tmp = fs::File::create(&tmp_path)?;
        tmp.write_all(body.as_bytes())?;
        tmp.flush()?;
    }
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_signal(expires_offset_secs: i64) -> SoftSignal {
        let now = Utc::now();
        let expires_at = (now + chrono::Duration::seconds(expires_offset_secs)).to_rfc3339();
        SoftSignal {
            kind: "test".to_string(),
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
        let sig = make_signal(200);
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
        let sig = make_signal(-1);
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
        let valid_sig = make_signal(200);
        let valid_json = serde_json::to_string(&valid_sig).unwrap();

        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "notjson").unwrap();
        writeln!(f, "{valid_json}").unwrap();

        let active = read_active(dir.path(), SystemTime::now()).unwrap();
        assert_eq!(active.len(), 1, "malformed line should be skipped");
        assert_eq!(active[0], valid_sig);
    }

    // -----------------------------------------------------------------------
    // Test 5: compact drops expired rows, keeps active ones
    // -----------------------------------------------------------------------
    #[test]
    fn compact_drops_expired_keeps_active() {
        let dir = TempDir::new().unwrap();
        // Two expired, one active.
        append(dir.path(), &make_signal(-10)).unwrap();
        append(dir.path(), &make_signal(200)).unwrap();
        append(dir.path(), &make_signal(-5)).unwrap();

        compact(dir.path(), SystemTime::now()).unwrap();

        // After compaction only the active signal remains on disk.
        let active = read_active(dir.path(), SystemTime::now()).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].kind, "test");

        // Verify the file physically shrank to a single row.
        let path = signals_path(dir.path());
        let raw = fs::read_to_string(&path).unwrap();
        assert_eq!(raw.lines().filter(|l| !l.trim().is_empty()).count(), 1);
    }

    // -----------------------------------------------------------------------
    // Test 6: compact on missing file is a no-op
    // -----------------------------------------------------------------------
    #[test]
    fn compact_missing_file_is_noop() {
        let dir = TempDir::new().unwrap();
        compact(dir.path(), SystemTime::now()).unwrap();
        assert!(!signals_path(dir.path()).exists());
    }
}
