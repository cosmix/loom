//! Orchestrator loop liveness ("is the scheduler still turning?").
//!
//! The daemon supervises the orchestrator and host jobs alongside its socket
//! server. An orchestrator error or panic triggers shutdown, but a loop blocked
//! in a subprocess or git operation can leave the server responsive without
//! scheduling stages.
//!
//! Session heartbeats (`monitor::heartbeat`) answer "is the *agent* alive?".
//! This answers "is the *scheduler* alive?", which is a different question and
//! can be inspected from outside the process.
//!
//! The loop stamps this file with a timestamp and the phase it is entering.
//! When the file goes stale while the daemon process is up, the loop is stuck,
//! and the recorded phase says where.
//!
//! Format: line 1 is the RFC 3339 timestamp, line 2 the phase. A long operation
//! wrapped by [`crate::orchestrator::busy::while_busy`] adds line 3,
//! `busy <op> <started_at> <stage_id>`, and a helper thread keeps re-stamping the
//! file while the operation runs. A third line that fails validation reads as
//! "not busy"; it never hides the tick. Every phase entry rewrites two lines, which
//! clears the busy line.

use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

use super::busy::{Busy, BusyOp};

/// File name inside `.loom/work/` holding the last loop tick.
const TICK_FILE: &str = "orchestrator.tick";

/// Scratch name each write goes through, so a reader sees the old tick or the
/// new one, never a truncated file.
const TICK_TMP_FILE: &str = "orchestrator.tick.tmp";

/// Longest stage id, in bytes, a busy line may carry.
const MAX_STAGE_ID_BYTES: usize = 128;

/// How far behind the tick may fall before the loop is considered stalled.
///
/// The poll interval is 5s, so this is twelve missed iterations. The threshold
/// applies to unwrapped work only: operations known to run longer re-stamp the
/// tick from a helper thread through `busy::while_busy`, and are judged against
/// their own ceiling. It is short enough that an operator notices a real stall
/// within a coffee break rather than overnight.
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
    /// The wrapped operation running when this tick was written, if any.
    pub busy: Option<Busy>,
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
/// (read-only `.loom/work/`, full disk) would otherwise flood the log with the same
/// line every few seconds, and those conditions surface far more loudly
/// elsewhere.
pub fn record(work_dir: &Path, phase: Phase) {
    write_tick(work_dir, phase, None);
}

/// Stamp the current time and phase plus the busy line for `busy`. Only
/// `busy::while_busy` calls this; best-effort like [`record`].
pub fn record_busy(work_dir: &Path, phase: Phase, busy: &Busy) {
    debug_assert!(
        valid_stage_id(&busy.stage_id),
        "a busy line with stage id {:?} would read back as not busy",
        busy.stage_id
    );
    write_tick(work_dir, phase, Some(busy));
}

/// Write the tick through [`TICK_TMP_FILE`] and rename it into place.
fn write_tick(work_dir: &Path, phase: Phase, busy: Option<&Busy>) {
    let mut content = format!("{}\n{}\n", Utc::now().to_rfc3339(), phase.as_str());
    if let Some(busy) = busy {
        content.push_str(&format!(
            "busy {} {} {}\n",
            busy.op.as_str(),
            busy.started_at.to_rfc3339(),
            busy.stage_id
        ));
    }
    let tmp = work_dir.join(TICK_TMP_FILE);
    if std::fs::write(&tmp, content).is_ok() {
        let _ = std::fs::rename(&tmp, tick_path(work_dir));
    }
}

/// Whether `id` may appear in a busy line: non-empty, at most
/// [`MAX_STAGE_ID_BYTES`], and only ASCII letters, digits, `-`, `_` and `.`.
fn valid_stage_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_STAGE_ID_BYTES
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Parse `busy <op> <started_at> <stage_id>`; any rule failure is `None`.
fn parse_busy(line: &str) -> Option<Busy> {
    let mut parts = line.strip_prefix("busy ")?.splitn(3, ' ');
    let op = BusyOp::parse(parts.next()?)?;
    let started_at = DateTime::parse_from_rfc3339(parts.next()?).ok()?;
    let stage_id = parts.next().filter(|id| valid_stage_id(id))?;
    Some(Busy {
        op,
        stage_id: stage_id.to_string(),
        started_at: started_at.with_timezone(&Utc),
    })
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

    let phase = lines.next().and_then(|p| Phase::from_str(p.trim()));
    Ok(Some(Tick {
        recorded_at: recorded_at.with_timezone(&Utc),
        phase,
        busy: lines.next().and_then(parse_busy),
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
            busy: None,
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
            busy: None,
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
            busy: None,
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

    fn busy_for(stage_id: &str) -> Busy {
        Busy {
            op: BusyOp::MergeVerification,
            stage_id: stage_id.to_string(),
            started_at: Utc::now(),
        }
    }

    #[test]
    fn test_record_busy_roundtrips_the_busy_line() {
        let temp = TempDir::new().unwrap();
        let busy = busy_for("stage-a.1_x");

        record_busy(temp.path(), Phase::Events, &busy);
        let tick = read(temp.path()).unwrap().expect("tick should be written");

        assert_eq!(tick.phase, Some(Phase::Events));
        assert_eq!(tick.busy, Some(busy));
    }

    #[test]
    fn test_record_after_record_busy_clears_the_busy_line() {
        let temp = TempDir::new().unwrap();

        record_busy(temp.path(), Phase::Events, &busy_for("stage-a"));
        record(temp.path(), Phase::Idle);

        assert_eq!(read(temp.path()).unwrap().unwrap().busy, None);
    }

    #[test]
    fn test_bad_busy_lines_read_as_not_busy() {
        let now = Utc::now().to_rfc3339();
        let bad = [
            format!("busy provisioning {now}"),
            format!("busy provisioning {now} "),
            format!("busy provisioning {now} a b"),
            format!("busy provisioning {now} {}", "a".repeat(129)),
            format!("busy nope {now} a"),
            "busy provisioning yesterday a".to_string(),
            format!("busy  provisioning {now} a"),
            format!("busy provisioning {now} \u{e9}"),
            format!("note provisioning {now} a"),
        ];
        for line in bad {
            let temp = TempDir::new().unwrap();
            let content = format!("{now}\nspawning\n{line}\n");
            std::fs::write(temp.path().join(TICK_FILE), content).unwrap();

            let tick = read(temp.path()).unwrap().expect("the tick still reads");
            assert_eq!(tick.phase, Some(Phase::Spawning), "{line:?}");
            assert_eq!(tick.busy, None, "{line:?}");
        }
    }
}
