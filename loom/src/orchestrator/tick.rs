//! Orchestrator loop liveness ("is the scheduler still turning?").
//!
//! # Why this exists
//!
//! The daemon is two threads: a socket server that answers `loom status`, and
//! the orchestrator poll loop that actually schedules stages. Nothing tied the
//! two together, so a loop that stopped turning — blocked in a subprocess,
//! wedged on a git operation — left the daemon answering "● daemon running"
//! while no stage would ever start again. The only visible symptom was a stage
//! sitting in `Queued` forever, which looks identical to a dependency problem.
//!
//! Session heartbeats (`monitor::heartbeat`) answer "is the *agent* alive?".
//! This answers "is the *scheduler* alive?", which is a different question and
//! was previously unanswerable from outside the process.
//!
//! The loop stamps this file with a timestamp and the phase it is entering.
//! When the file goes stale while the daemon process is up, the loop is stuck,
//! and the recorded phase says where.

use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

/// File name inside `.work/` holding the last loop tick.
const TICK_FILE: &str = "orchestrator.tick";

/// How far behind the tick may fall before the loop is considered stalled.
///
/// The poll interval is 5s, so this is twelve missed iterations — long enough
/// that a slow git operation or a bounded 5s teardown never trips it, short
/// enough that an operator notices within a coffee break rather than
/// overnight.
pub const STALL_THRESHOLD_SECS: i64 = 60;

/// Phase of the poll loop, recorded alongside the tick so a stall report can
/// say *where* the loop stopped instead of only *that* it stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Reconciling git state and syncing stage files into the graph.
    Sync,
    /// Creating worktrees and spawning sessions for ready stages.
    Spawning,
    /// Polling the monitor and handling events (completion, crash, handoff).
    Events,
    /// Sleeping until the next poll.
    Idle,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Sync => "sync",
            Phase::Spawning => "spawning",
            Phase::Events => "events",
            Phase::Idle => "idle",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "sync" => Some(Phase::Sync),
            "spawning" => Some(Phase::Spawning),
            "events" => Some(Phase::Events),
            "idle" => Some(Phase::Idle),
            _ => None,
        }
    }
}

/// A tick read back from disk.
#[derive(Debug, Clone)]
pub struct Tick {
    pub recorded_at: DateTime<Utc>,
    pub phase: Option<Phase>,
}

impl Tick {
    /// Seconds since this tick was recorded, relative to `now`. Saturates at 0
    /// so a clock adjustment cannot report a negative age.
    pub fn age_secs(&self, now: DateTime<Utc>) -> i64 {
        (now - self.recorded_at).num_seconds().max(0)
    }

    /// Whether the loop has been silent long enough to call it stalled.
    pub fn is_stalled(&self, now: DateTime<Utc>) -> bool {
        self.age_secs(now) >= STALL_THRESHOLD_SECS
    }
}

fn tick_path(work_dir: &Path) -> PathBuf {
    work_dir.join(TICK_FILE)
}

/// Stamp the current time and phase.
///
/// Best-effort by design: this is a diagnostic, and a failed write must never
/// interfere with scheduling. Errors are swallowed rather than logged — the
/// write happens several times per poll interval, so a persistent failure
/// (read-only `.work/`, full disk) would otherwise flood the log with the same
/// line every few seconds, and those conditions surface far more loudly
/// elsewhere.
pub fn record(work_dir: &Path, phase: Phase) {
    let content = format!("{}\n{}\n", Utc::now().to_rfc3339(), phase.as_str());
    let _ = std::fs::write(tick_path(work_dir), content);
}

/// Read the last recorded tick, if any.
///
/// Returns `Ok(None)` when no tick file exists — either the daemon has never
/// run, or it predates this mechanism. Callers must treat "no tick" as
/// "unknown", never as "stalled".
pub fn read(work_dir: &Path) -> Result<Option<Tick>> {
    let path = tick_path(work_dir);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)?;
    let mut lines = content.lines();

    let Some(timestamp) = lines.next() else {
        return Ok(None);
    };
    let Ok(recorded_at) = DateTime::parse_from_rfc3339(timestamp.trim()) else {
        return Ok(None);
    };

    Ok(Some(Tick {
        recorded_at: recorded_at.with_timezone(&Utc),
        phase: lines.next().and_then(|p| Phase::from_str(p.trim())),
    }))
}

/// Remove the tick file.
///
/// Called on daemon shutdown so a stopped daemon does not leave a stale tick
/// that a later `loom status` could misread.
pub fn clear(work_dir: &Path) {
    let _ = std::fs::remove_file(tick_path(work_dir));
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_record_then_read_roundtrips_phase() {
        let temp = TempDir::new().unwrap();

        record(temp.path(), Phase::Spawning);
        let tick = read(temp.path()).unwrap().expect("tick should be written");

        assert_eq!(tick.phase, Some(Phase::Spawning));
        assert!(tick.age_secs(Utc::now()) < 5);
    }

    #[test]
    fn test_read_returns_none_when_never_recorded() {
        let temp = TempDir::new().unwrap();
        assert!(read(temp.path()).unwrap().is_none());
    }

    #[test]
    fn test_fresh_tick_is_not_stalled() {
        let now = Utc::now();
        let tick = Tick {
            recorded_at: now - Duration::seconds(STALL_THRESHOLD_SECS - 1),
            phase: Some(Phase::Idle),
        };

        assert!(!tick.is_stalled(now));
    }

    #[test]
    fn test_tick_older_than_threshold_is_stalled() {
        let now = Utc::now();
        // The failure this guards against: the loop froze during teardown and
        // never turned again, while the daemon kept reporting "running".
        let tick = Tick {
            recorded_at: now - Duration::hours(10),
            phase: Some(Phase::Events),
        };

        assert!(tick.is_stalled(now));
        assert_eq!(tick.age_secs(now), 36_000);
    }

    #[test]
    fn test_clock_skew_does_not_produce_negative_age() {
        let now = Utc::now();
        let tick = Tick {
            recorded_at: now + Duration::minutes(5),
            phase: None,
        };

        assert_eq!(tick.age_secs(now), 0);
        assert!(!tick.is_stalled(now));
    }

    #[test]
    fn test_unparseable_tick_reads_as_unknown_not_stalled() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join(TICK_FILE), "not-a-timestamp\n").unwrap();

        assert!(read(temp.path()).unwrap().is_none());
    }

    #[test]
    fn test_clear_removes_tick() {
        let temp = TempDir::new().unwrap();

        record(temp.path(), Phase::Sync);
        clear(temp.path());

        assert!(read(temp.path()).unwrap().is_none());
    }
}
