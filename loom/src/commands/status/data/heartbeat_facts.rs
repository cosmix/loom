//! Heartbeat-derived facts folded into a stage's `StageSummary`.

use std::path::Path;

use chrono::Utc;

use crate::commands::subagents::OpenToolCall;
use crate::fs::work_dir::WorkDir;
use crate::models::constants::STALENESS_THRESHOLD_SECS;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::{Stage, StageStatus, StatusBucket};
use crate::orchestrator::monitor::heartbeat::{judge_heartbeat_path, read_heartbeat, Heartbeat};
use crate::orchestrator::monitor::progress::age_secs;
use crate::provider_data::resolve::projects_root;

use super::busy::open_tool;
use super::sanitize::valid_stage_id;
use super::ActivityStatus;

/// Read one of a stage's heartbeat files from the heartbeat directory.
///
/// Both callers join `stage_id` into the path, and the id comes from a stage
/// file's frontmatter, so it is validated here: an id carrying `../` would
/// otherwise make the daemon read an arbitrary `*.json` and put its strings in
/// the payload every subscriber renders.
fn read_stage_heartbeat(stage_id: &str, path: &std::path::Path) -> Option<Heartbeat> {
    if !valid_stage_id(stage_id) || !path.exists() {
        return None;
    }
    read_heartbeat(path).ok()
}

fn read_heartbeat_for_stage(stage_id: &str, work_dir: &WorkDir) -> Option<Heartbeat> {
    let path = work_dir
        .root()
        .join("heartbeat")
        .join(format!("{stage_id}.json"));
    read_stage_heartbeat(stage_id, &path)
}

fn read_judge_heartbeat_for_stage(stage_id: &str, work_dir: &WorkDir) -> Option<Heartbeat> {
    read_stage_heartbeat(stage_id, &judge_heartbeat_path(work_dir.root(), stage_id))
}

/// Seconds of silence after which a running session is `Stale`: the stage's
/// own budget, the same threshold the orchestrator's hung detection applies
/// (`Stage::effective_subagent_timeout_secs`), so the badge never says stale
/// while the monitor still considers the session healthy.
///
/// A budget of 0 would make every stage stale on its first poll. The monitor
/// reads 0 as "warnings only" (`is_stall_escalation`); a display badge that
/// never went stale would hide a hung stage, so 0 falls back to the default
/// display threshold instead.
fn stale_after_secs(stage: &Stage) -> u64 {
    match stage.effective_subagent_timeout_secs() {
        0 => STALENESS_THRESHOLD_SECS,
        budget => budget,
    }
}

/// Calculate activity status from session state, heartbeat silence, and the
/// stage's own status. `stage.status` matters for the no-session case: no
/// session while the stage sits somewhere idle is unremarkable, but no
/// session while the stage claims `Executing` means the tracking data itself
/// is missing (killed daemon, lost session file) — that is `Orphaned`, not
/// `Idle`, and the dashboard must not render it as a quiet agent. It also
/// matters once the stage is finished: a `Completed`/`Skipped` stage has no
/// agent working it regardless of what its last session file says. Likewise a
/// session that ended without crashing (`SessionStatus::is_terminal` and not
/// `Crashed`) is neither working nor hung, whatever the stage's own status.
///
/// A running session whose heartbeat is silent past the quiet threshold is
/// `Busy` while a tool call is open, else `Stale` past the stage's budget and
/// `Quiet` before it. `tool_open` is only called then, so fresh stages cost no
/// transcript reads.
fn determine_activity_status(
    session: Option<&Session>,
    staleness_secs: Option<u64>,
    stage: &Stage,
    tool_open: impl FnOnce() -> bool,
) -> ActivityStatus {
    let stage_status = &stage.status;
    let stale_after = stale_after_secs(stage);
    match (session, staleness_secs) {
        // No session, but the stage claims to be running - the session
        // record is missing, not merely quiet.
        (None, _) if *stage_status == StageStatus::Executing => ActivityStatus::Orphaned,
        // No session and the stage isn't claiming to run - idle.
        (None, _) => ActivityStatus::Idle,
        // Session crashed
        (Some(s), _) if s.status == SessionStatus::Crashed => ActivityStatus::Error,
        // A finished stage has no agent: whatever its last session file says,
        // the work is over.
        (_, _) if stage_status.bucket() == StatusBucket::Completed => ActivityStatus::Idle,
        // A session that ended without crashing is not working and not hung.
        (Some(s), _) if s.status.is_terminal() => ActivityStatus::Idle,
        // Session running, silent past the quiet threshold
        (Some(_), Some(secs)) if secs > STALENESS_THRESHOLD_SECS.min(stale_after) => {
            if tool_open() {
                ActivityStatus::Busy
            } else if secs > stale_after {
                ActivityStatus::Stale
            } else {
                ActivityStatus::Quiet
            }
        }
        // Session running with recent heartbeat
        (Some(_), _) => ActivityStatus::Working,
    }
}

/// The call the heartbeat's transcript, or a subagent's, has open.
fn probe_open_tool(
    heartbeat: Option<&Heartbeat>,
    projects_root: Option<&Path>,
) -> Option<OpenToolCall> {
    let heartbeat = heartbeat?;
    let transcript = heartbeat.transcript_path.as_deref()?;
    open_tool(
        transcript,
        heartbeat.effective_progress_at(),
        projects_root?,
    )
}

/// Heartbeat-derived facts for a stage's [`StageSummary`]: staleness, current
/// activity, the last recorded tool and activity strings, and the judge
/// heartbeat's age. Extracted from `build_stage_summary` to keep that function
/// within the line limit.
pub(super) struct HeartbeatFacts {
    pub(super) staleness_secs: Option<u64>,
    pub(super) activity_status: ActivityStatus,
    /// The heartbeat's last tool; while `activity_status` is `Busy`, the tool
    /// call that is open instead (`None` when its name is unknown).
    pub(super) last_tool: Option<String>,
    pub(super) last_activity: Option<String>,
    pub(super) judge_heartbeat_secs: Option<u64>,
}

pub(super) fn heartbeat_facts(
    stage: &Stage,
    session: Option<&Session>,
    work_dir: &WorkDir,
) -> HeartbeatFacts {
    heartbeat_facts_under(stage, session, work_dir, projects_root().as_deref())
}

/// [`heartbeat_facts`] with the Claude projects root the transcripts must sit
/// under; `None` when it cannot be determined, which reads as nothing open.
pub(super) fn heartbeat_facts_under(
    stage: &Stage,
    session: Option<&Session>,
    work_dir: &WorkDir,
    projects_root: Option<&Path>,
) -> HeartbeatFacts {
    let heartbeat = read_heartbeat_for_stage(&stage.id, work_dir);
    // A merge resolver writes the stage's heartbeat file after the worker that
    // finished before it. Only the resolver's own heartbeat describes it, and
    // until it makes a tool call its silence runs from its spawn.
    let resolver = session.filter(|s| s.session_type == SessionType::Merge);
    let heartbeat = match resolver {
        Some(resolver) => heartbeat.filter(|hb| hb.session_id == resolver.id),
        None => heartbeat,
    };

    // Liveness follows useful progress; activity strings remain observational.
    let now = Utc::now();
    let staleness_secs = heartbeat
        .as_ref()
        .map(|hb| age_secs(now, hb.effective_progress_at()))
        .or_else(|| {
            resolver
                .filter(|resolver| !resolver.status.is_terminal())
                .map(|resolver| age_secs(now, resolver.created_at))
        });

    // Determine activity status based on session, heartbeat, and stage status
    let mut open = None;
    let activity_status = determine_activity_status(session, staleness_secs, stage, || {
        open = probe_open_tool(heartbeat.as_ref(), projects_root);
        open.is_some()
    });

    let last_tool = match activity_status {
        ActivityStatus::Busy => open.and_then(|call| call.name),
        _ => heartbeat.as_ref().and_then(|hb| hb.last_tool.clone()),
    };
    let last_activity = heartbeat.as_ref().and_then(|hb| hb.activity.clone());
    let judge_heartbeat_secs = read_judge_heartbeat_for_stage(&stage.id, work_dir)
        .map(|hb| age_secs(Utc::now(), hb.effective_progress_at()));

    HeartbeatFacts {
        staleness_secs,
        activity_status,
        last_tool,
        last_activity,
        judge_heartbeat_secs,
    }
}

#[cfg(test)]
#[path = "heartbeat_facts_tests.rs"]
mod tests;
