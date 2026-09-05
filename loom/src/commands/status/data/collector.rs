use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;

use crate::commands::status::merge_status::build_merge_report;
use crate::fs::work_dir::{load_config, resolve_context_ceiling_tokens, WorkDir};
use crate::models::constants::STALENESS_THRESHOLD_SECS;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus, StatusBucket};
use crate::orchestrator::coherence::executing_stage_incoherence;
use crate::orchestrator::get_merge_point;
use crate::orchestrator::monitor::heartbeat::{judge_heartbeat_path, read_heartbeat, Heartbeat};
use crate::parser::frontmatter::parse_from_markdown;
use crate::plan::parser::extract_plan_name;
use crate::verify::transitions::list_all_stages;

use super::sanitize::{sanitize_stage_summary, valid_stage_id};
use super::timing::execution_secs_live;
use super::{
    execution_models_for_stage, ActivityStatus, MergeSummary, ProgressSummary, StageSummary,
    StatusData,
};

#[cfg(test)]
use super::SessionSummary;
#[cfg(test)]
use crate::process::is_process_alive;

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

/// Calculate activity status from session state, heartbeat staleness, and the
/// stage's own status. `stage_status` matters only for the no-session case:
/// no session while the stage sits somewhere idle is unremarkable, but no
/// session while the stage claims `Executing` means the tracking data itself
/// is missing (killed daemon, lost session file) — that is `Orphaned`, not
/// `Idle`, and the dashboard must not render it as a quiet agent.
fn determine_activity_status(
    session: Option<&Session>,
    staleness_secs: Option<u64>,
    stage_status: &StageStatus,
) -> ActivityStatus {
    match (session, staleness_secs) {
        // No session, but the stage claims to be running - the session
        // record is missing, not merely quiet.
        (None, _) if *stage_status == StageStatus::Executing => ActivityStatus::Orphaned,
        // No session and the stage isn't claiming to run - idle.
        (None, _) => ActivityStatus::Idle,
        // Session crashed
        (Some(s), _) if s.status == SessionStatus::Crashed => ActivityStatus::Error,
        // Session running but stale heartbeat (> 5 minutes)
        (Some(_), Some(secs)) if secs > STALENESS_THRESHOLD_SECS => ActivityStatus::Stale,
        // Session running with recent heartbeat
        (Some(_), _) => ActivityStatus::Working,
    }
}

/// Load all sessions from the state directory's sessions/ directory
pub fn load_all_sessions(work_dir: &WorkDir) -> Result<Vec<Session>> {
    let sessions_dir = work_dir.sessions_dir();
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    let entries = fs::read_dir(&sessions_dir).with_context(|| {
        format!(
            "Failed to read sessions directory: {}",
            sessions_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            match load_session_from_file(&path) {
                Ok(session) => sessions.push(session),
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to load session from {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
    }

    Ok(sessions)
}

/// Load a single session from a markdown file
fn load_session_from_file(path: &std::path::Path) -> Result<Session> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read session file: {}", path.display()))?;

    parse_from_markdown(&content, "Session")
}

/// The session that speaks for this stage.
///
/// Three steps, weakest claim last. `stage.session` is the stage's own claim
/// about which agent is executing it and wins outright — but only among the
/// sessions that name this stage back, since an id repeated or reused across
/// stages would otherwise attribute another stage's agent to this row. Failing
/// that, the newest session still alive: session files accumulate, so a retried
/// stage leaves every previous corpse on disk with `stage_id` still set, and the
/// first one `read_dir` happens to return is nobody in particular. Only if
/// nothing is alive does a terminal session speak, so that a stage whose agent
/// crashed still renders as `Error` rather than as an orphan with no session at
/// all; [`reported_reading`] is what keeps its frozen token count off the
/// screen.
fn session_for_stage<'a>(stage: &Stage, sessions: &'a [Session]) -> Option<&'a Session> {
    let own = || {
        sessions
            .iter()
            .filter(|s| s.stage_id.as_ref() == Some(&stage.id))
    };
    if let Some(session_id) = stage.session.as_deref() {
        if let Some(named) = own().find(|s| s.id == session_id) {
            return Some(named);
        }
    }
    own()
        .filter(|s| !s.status.is_terminal())
        .max_by_key(|s| s.created_at)
        .or_else(|| own().max_by_key(|s| s.created_at))
}

/// The session a token reading may be taken from, if any.
///
/// Two readings must never reach the column. A session that has not reported
/// yet would render a confident `0 / 150000` where nothing at all is known. A
/// TERMINAL session's reading stopped tracking the stage the moment its agent
/// died, so showing it against the live stage's ceiling states a number that
/// has not been true since. The stage's ACTIVITY still comes from that session
/// either way — only its number is dropped.
fn reported_reading(session: Option<&Session>) -> Option<&Session> {
    session.filter(|s| s.context_tokens > 0 && !s.status.is_terminal())
}

/// The record `stage.session` names, if any — an exact identity match, not
/// `session_for_stage`'s "best guess among sessions naming this stage back"
/// fallback. Coherence judgments must see exactly what the stage POINTS AT,
/// including a session that does not name the stage back at all.
fn assigned_session<'a>(stage: &Stage, sessions: &'a [Session]) -> Option<&'a Session> {
    let session_id = stage.session.as_deref()?;
    sessions.iter().find(|s| s.id == session_id)
}

/// Build a StageSummary from a Stage and optional associated Session.
///
fn build_stage_summary(stage: &Stage, sessions: &[Session], work_dir: &WorkDir) -> StageSummary {
    let session = session_for_stage(stage, sessions);
    let assigned = assigned_session(stage, sessions);
    let session_type = assigned.or(session).map(|s| s.session_type);
    let incoherence = executing_stage_incoherence(stage, assigned);
    let reading = reported_reading(session);
    let context_tokens = reading.map(|s| s.context_tokens);
    let context_ceiling_tokens = reading
        .map(|_| resolve_context_ceiling_tokens(work_dir.root(), stage.context_ceiling_tokens));

    let pid = session.and_then(|s| s.pid);
    let session_alive = pid.map(crate::process::is_process_alive).unwrap_or(false);

    let now = Utc::now();

    let heartbeat = heartbeat_facts(stage, session, work_dir);
    let extras = stage_extras(stage, work_dir);

    StageSummary {
        id: stage.id.clone(),
        name: stage.name.clone(),
        status: stage.status.clone(),
        stage_type: stage.stage_type,
        dependencies: stage.dependencies.clone(),
        context_tokens,
        elapsed_secs: Some((now - stage.created_at).num_seconds()),
        execution_secs: execution_secs_live(stage, now),
        base_branch: stage.base_branch.clone(),
        base_merged_from: stage.base_merged_from.clone(),
        failure_info: stage.failure_info.clone(),
        activity_status: heartbeat.activity_status,
        last_tool: heartbeat.last_tool,
        last_activity: heartbeat.last_activity,
        staleness_secs: heartbeat.staleness_secs,
        context_ceiling_tokens,
        review_reason: stage.review_reason.clone(),
        merged: stage.merged,
        cleanup_warning: stage.cleanup_warning.clone(),
        held: stage.held,
        retry_count: stage.retry_count,
        max_retries: stage.max_retries,
        pid,
        session_alive,
        model: stage.effective_model().to_string(),
        session_type,
        incoherence,
        execution_models: extras.execution_models,
        dispute_count: stage.dispute_count,
        judge_heartbeat_secs: extras.judge_heartbeat_secs,
        session_backend: session.map(|s| s.backend),
    }
}

/// Heartbeat-derived facts for a stage's [`StageSummary`]: staleness, current
/// activity, and the last recorded tool/activity strings. Extracted from
/// `build_stage_summary` to keep that function within the line limit.
struct HeartbeatFacts {
    staleness_secs: Option<u64>,
    activity_status: ActivityStatus,
    last_tool: Option<String>,
    last_activity: Option<String>,
}

struct StageExtras {
    execution_models: Vec<String>,
    judge_heartbeat_secs: Option<u64>,
}

fn stage_extras(stage: &Stage, work_dir: &WorkDir) -> StageExtras {
    let judge_heartbeat_secs = read_judge_heartbeat_for_stage(&stage.id, work_dir).map(|hb| {
        Utc::now()
            .signed_duration_since(hb.timestamp)
            .num_seconds()
            .max(0) as u64
    });
    StageExtras {
        execution_models: execution_models_for_stage(work_dir, &stage.id),
        judge_heartbeat_secs,
    }
}

fn heartbeat_facts(stage: &Stage, session: Option<&Session>, work_dir: &WorkDir) -> HeartbeatFacts {
    let heartbeat = read_heartbeat_for_stage(&stage.id, work_dir);

    // Calculate staleness (seconds since last heartbeat)
    let staleness_secs = heartbeat.as_ref().map(|hb| {
        let age = Utc::now().signed_duration_since(hb.timestamp);
        age.num_seconds().max(0) as u64
    });

    // Determine activity status based on session, heartbeat, and stage status
    let activity_status = determine_activity_status(session, staleness_secs, &stage.status);

    let last_tool = heartbeat.as_ref().and_then(|hb| hb.last_tool.clone());
    let last_activity = heartbeat.as_ref().and_then(|hb| hb.activity.clone());

    HeartbeatFacts {
        staleness_secs,
        activity_status,
        last_tool,
        last_activity,
    }
}

/// Build a SessionSummary from a Session
#[cfg(test)]
fn build_session_summary(session: &Session) -> SessionSummary {
    let uptime_secs = (Utc::now() - session.created_at).num_seconds();
    let is_alive = session.pid.map(is_process_alive).unwrap_or(false);

    SessionSummary {
        id: session.id.clone(),
        stage_id: session.stage_id.clone(),
        pid: session.pid,
        context_tokens: session.context_tokens,
        uptime_secs,
        is_alive,
    }
}

/// Calculate progress summary from stages
fn calculate_progress(stages: &[Stage]) -> ProgressSummary {
    let total = stages.len();
    let mut completed = 0;
    let mut executing = 0;
    let mut pending = 0;
    let mut blocked = 0;

    for stage in stages {
        // Use canonical bucket() to categorise statuses — prevents the three
        // status-count copies diverging (D-5). Skipped maps to Completed bucket
        // but we exclude it from the visible completed count.
        match stage.status.bucket() {
            StatusBucket::Executing => executing += 1,
            StatusBucket::Pending => pending += 1,
            StatusBucket::Completed => {
                if stage.status != StageStatus::Skipped {
                    completed += 1;
                }
            }
            StatusBucket::Blocked => blocked += 1,
        }
    }

    ProgressSummary {
        total,
        completed,
        executing,
        pending,
        blocked,
    }
}

/// Build a MergeSummary from merge report
fn build_merge_summary_from_report(
    report: &crate::commands::status::merge_status::MergeStatusReport,
) -> MergeSummary {
    MergeSummary {
        merged: report.merged.clone(),
        pending: report.pending.clone(),
        conflicts: report.conflicts.clone(),
    }
}

/// Load the plan name from config.toml and the plan file (best-effort).
fn load_plan_name(work_dir: &WorkDir) -> Option<String> {
    let config = load_config(work_dir.root()).ok()??;
    let source_path = config.source_path()?;
    let project_root = work_dir.project_root()?;
    let plan_path = project_root.join(&source_path);
    let content = fs::read_to_string(plan_path).ok()?;
    extract_plan_name(&content).ok()
}

/// Collect all status data from the work directory
pub fn collect_status_data(work_dir: &WorkDir) -> Result<StatusData> {
    // Load all stages
    let stages = list_all_stages(work_dir.root())?;

    // Load all sessions
    let sessions = load_all_sessions(work_dir)?;

    // Build stage summaries, then flatten the untrusted strings on them before
    // they reach a subscriber's terminal (see `sanitize`).
    let mut stage_summaries: Vec<StageSummary> = stages
        .iter()
        .map(|stage| build_stage_summary(stage, &sessions, work_dir))
        .collect();
    stage_summaries.iter_mut().for_each(sanitize_stage_summary);

    // Get merge point for merge report
    let merge_point = if let Some(project_root) = work_dir.project_root() {
        get_merge_point(project_root).unwrap_or_else(|_| "main".to_string())
    } else {
        "main".to_string()
    };

    // Build merge report
    let merge_report = if let Some(project_root) = work_dir.project_root() {
        build_merge_report(&stages, &merge_point, project_root)?
    } else {
        crate::commands::status::merge_status::MergeStatusReport::new()
    };

    let merge_summary = build_merge_summary_from_report(&merge_report);

    // Calculate progress
    let progress = calculate_progress(&stages);

    // Load plan name (best-effort, don't fail status if unavailable)
    let plan_name = load_plan_name(work_dir);

    Ok(StatusData {
        stages: stage_summaries,
        merge: merge_summary,
        progress,
        plan_name,
        quota: crate::quota::read_snapshot(work_dir.root()),
    })
}

#[cfg(test)]
#[path = "collector_tests.rs"]
mod tests;
