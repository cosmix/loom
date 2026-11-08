//! Long operations on the scheduler thread, reported as "busy" instead of "stalled".
//!
//! [`while_busy`] runs a closure on the calling thread and records what it is doing
//! in the orchestrator tick (a third line, see `tick`). A helper thread re-stamps
//! the tick every [`REFRESH_INTERVAL`], so readers keep seeing a fresh tick for as
//! long as the operation runs, and [`loop_state`] judges the operation against its
//! own ceiling rather than against `tick::STALL_THRESHOLD_SECS`.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use super::tick::{self, Phase, Tick, STALL_THRESHOLD_SECS};

pub const BRIEF_GENERATION_CEILING_SECS: i64 = 300;
pub const MERGE_VERIFICATION_CEILING_SECS: i64 = 1_200;
pub const PROVISIONING_CEILING_SECS: i64 = 600;
/// Any wrapped operation running this long is reported as a stalled loop.
pub const GLOBAL_BUSY_CEILING_SECS: i64 = 1_800;
/// How often the helper thread re-stamps the tick; well under `STALL_THRESHOLD_SECS`.
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(10);

/// Longest the helper sleeps between checks of its stop flag.
const MAX_PARK: Duration = Duration::from_secs(1);

/// A scheduler-thread operation long enough to need its own tick line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyOp {
    BriefGeneration,
    MergeVerification,
    Provisioning,
}

impl BusyOp {
    /// The wire name written to the tick and sent to the web client.
    pub fn as_str(self) -> &'static str {
        match self {
            BusyOp::BriefGeneration => "brief-generation",
            BusyOp::MergeVerification => "merge-verification",
            BusyOp::Provisioning => "provisioning",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            BusyOp::BriefGeneration,
            BusyOp::MergeVerification,
            BusyOp::Provisioning,
        ]
        .into_iter()
        .find(|op| op.as_str() == s)
    }

    /// Seconds after which this operation is longer than expected.
    pub fn ceiling_secs(self) -> i64 {
        match self {
            BusyOp::BriefGeneration => BRIEF_GENERATION_CEILING_SECS,
            BusyOp::MergeVerification => MERGE_VERIFICATION_CEILING_SECS,
            BusyOp::Provisioning => PROVISIONING_CEILING_SECS,
        }
    }

    /// The operation as a phrase for status lines and alerts.
    pub fn describe(self, stage_id: &str) -> String {
        match self {
            BusyOp::BriefGeneration => format!("generating the brief for {stage_id}"),
            BusyOp::MergeVerification => format!("verifying the merge of {stage_id}"),
            BusyOp::Provisioning => format!("provisioning {stage_id}"),
        }
    }
}

/// The operation a tick says is running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Busy {
    pub op: BusyOp,
    pub stage_id: String,
    pub started_at: DateTime<Utc>,
}

impl Busy {
    /// Seconds the operation has run as of `now`. Saturates at 0 so a clock
    /// adjustment cannot report a negative duration.
    pub fn elapsed_secs(&self, now: DateTime<Utc>) -> i64 {
        (now - self.started_at).num_seconds().max(0)
    }
}

/// Run `f` on the calling thread while the tick names the operation.
///
/// The calling thread writes the busy tick before `f` starts, so a reader inside
/// `f` always finds it; a helper thread then re-stamps it every
/// [`REFRESH_INTERVAL`] with the same `started_at`. When `f` returns or unwinds the
/// helper is stopped and joined, and only then is the plain two-line tick for
/// `phase` written.
pub fn while_busy<T>(
    work_dir: &Path,
    phase: Phase,
    op: BusyOp,
    stage_id: &str,
    f: impl FnOnce() -> T,
) -> T {
    while_busy_every(work_dir, phase, op, stage_id, REFRESH_INTERVAL, f)
}

/// [`while_busy`] with a caller-chosen re-stamp interval. Public only so the
/// contract tests can prove the re-stamp without waiting [`REFRESH_INTERVAL`].
#[doc(hidden)]
pub fn while_busy_every<T>(
    work_dir: &Path,
    phase: Phase,
    op: BusyOp,
    stage_id: &str,
    interval: Duration,
    f: impl FnOnce() -> T,
) -> T {
    let busy = Busy {
        op,
        stage_id: stage_id.to_string(),
        started_at: Utc::now(),
    };
    tick::record_busy(work_dir, phase, &busy);
    // Declared outside the scope: its drop runs after the scope has joined the
    // helper, on return and on unwind alike, so no late re-stamp can bring the
    // busy line back after the clearing write.
    let _clear = ClearBusy { work_dir, phase };
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        // A failed spawn leaves the synchronous stamp standing and runs `f` anyway:
        // the scheduler must not panic over a diagnostic thread.
        let helper = std::thread::Builder::new()
            .spawn_scoped(scope, || restamp(work_dir, phase, &busy, interval, &done));
        // Stops the helper even when `f` panics; otherwise the scope would wait on
        // it forever and hang the daemon.
        let _stop = helper.as_ref().ok().map(|helper| StopHelper {
            done: &done,
            helper: helper.thread(),
        });
        f()
    })
}

/// Writes the two-line tick that ends the busy line when dropped.
struct ClearBusy<'a> {
    work_dir: &'a Path,
    phase: Phase,
}

impl Drop for ClearBusy<'_> {
    fn drop(&mut self) {
        tick::record(self.work_dir, self.phase);
    }
}

/// Sets the helper's stop flag and wakes it when dropped.
struct StopHelper<'a> {
    done: &'a AtomicBool,
    helper: &'a std::thread::Thread,
}

impl Drop for StopHelper<'_> {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        self.helper.unpark();
    }
}

/// Re-stamp the busy tick each time `interval` has passed, until `done` is set.
/// The caller already wrote the first stamp.
fn restamp(work_dir: &Path, phase: Phase, busy: &Busy, interval: Duration, done: &AtomicBool) {
    let park = interval.min(MAX_PARK);
    let mut stamped = Instant::now();
    while !done.load(Ordering::Acquire) {
        std::thread::park_timeout(park);
        if !done.load(Ordering::Acquire) && stamped.elapsed() >= interval {
            tick::record_busy(work_dir, phase, busy);
            stamped = Instant::now();
        }
    }
}

/// What the tick says the scheduler loop is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopState {
    /// No tick file, or an unreadable one.
    Unknown,
    /// The tick is fresh and nothing is wrapped.
    Ticking { age_secs: i64 },
    /// A wrapped operation runs and the tick is fresh.
    Busy {
        busy: Busy,
        elapsed_secs: i64,
        over_ceiling: bool,
    },
    /// The tick is `STALL_THRESHOLD_SECS` or older (busy or not), or a wrapped
    /// operation has run `GLOBAL_BUSY_CEILING_SECS` or longer.
    Stalled {
        age_secs: i64,
        busy: Option<Busy>,
        elapsed_secs: Option<i64>,
    },
}

/// Classify `tick` as of `now`. First matching rule wins: no tick is `Unknown`; a
/// tick at or past the stall threshold is `Stalled`; a wrapped operation at or past
/// the global ceiling is `Stalled`; a wrapped operation is `Busy`; else `Ticking`.
pub fn loop_state(tick: Option<&Tick>, now: DateTime<Utc>) -> LoopState {
    let Some(tick) = tick else {
        return LoopState::Unknown;
    };
    let age_secs = tick.age_secs(now);
    let elapsed_secs = tick.busy.as_ref().map(|busy| busy.elapsed_secs(now));
    let past_global = elapsed_secs.is_some_and(|secs| secs >= GLOBAL_BUSY_CEILING_SECS);
    if age_secs >= STALL_THRESHOLD_SECS || past_global {
        return LoopState::Stalled {
            age_secs,
            busy: tick.busy.clone(),
            elapsed_secs,
        };
    }
    match (&tick.busy, elapsed_secs) {
        (Some(busy), Some(elapsed_secs)) => LoopState::Busy {
            over_ceiling: elapsed_secs >= busy.op.ceiling_secs(),
            busy: busy.clone(),
            elapsed_secs,
        },
        _ => LoopState::Ticking { age_secs },
    }
}

#[cfg(test)]
#[path = "busy_tests.rs"]
mod tests;
