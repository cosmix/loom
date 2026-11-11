use anyhow::Result;
use chrono::Utc;
use std::fs;

use crate::fs::work_dir::{load_config, resolve_context_ceiling_tokens, WorkDir};
use crate::models::session::{Session, SessionType};
use crate::models::stage::{Stage, StageStatus, StageType, StatusBucket};
use crate::orchestrator::coherence::executing_stage_incoherence;
use crate::orchestrator::get_merge_point;
use crate::plan::parser::extract_plan_name;
use crate::status::merge_status::build_merge_report;
use crate::verify::transitions::list_all_stages;

use super::completion_view::collect_completion_view;
use super::criteria_results::failing_criteria;
use super::heartbeat_facts::{heartbeat_facts, HeartbeatFacts};
use super::merge_note::{merge_block_text, stash_warning};
use super::resolver::{merge_resolver_facts, speaking_for};
use super::sanitize::{flow_is_fresh, neutralize, sanitize_stage_summary, sanitize_token_flow};
use super::stopped::{blocked_acceptance, blocked_close_reason, terminal_history_session};
use super::timing::{elapsed_secs_live, execution_secs_live};
use super::{
    execution_models_for_stage, BlockDetail, CompletionBlockerSummary, ContractTesting,
    MergeSummary, ProgressSummary, SessionExitReason, StageSummary, StatusData,
};

#[cfg(test)]
use super::SessionSummary;
#[cfg(test)]
use crate::process::is_process_alive;

pub use crate::provider_data::sessions::load_all_sessions;

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
///
/// The exceptions to "the named session wins outright" are two, each only once
/// the named session has ended: a stage in a merge state is spoken for by its
/// live merge resolver, and a `NeedsAdjudication` stage by its live adjudication
/// judge ([`speaking_for`]).
fn session_for_stage<'a>(stage: &Stage, sessions: &'a [Session]) -> Option<&'a Session> {
    let own = || {
        sessions
            .iter()
            .filter(|s| s.stage_id.as_ref() == Some(&stage.id))
    };
    if let Some(session_id) = stage.session.as_deref() {
        if let Some(named) = own().find(|s| s.id == session_id) {
            return Some(speaking_for(stage, named, sessions));
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

/// Everything a stage's summary takes from its session record.
struct SessionFacts<'a> {
    session: Option<&'a Session>,
    session_type: Option<SessionType>,
    incoherence: Option<String>,
    context_tokens: Option<u32>,
    context_ceiling_tokens: Option<u32>,
    pid: Option<u32>,
    session_alive: bool,
    merge_resolver_session: Option<String>,
    merge_resolver_attempts: Option<u32>,
    /// The assigned session's exit reason and the stage's completion blocker.
    completion_view: (Option<SessionExitReason>, Option<CompletionBlockerSummary>),
    heartbeat: HeartbeatFacts,
}

fn session_facts<'a>(
    stage: &Stage,
    sessions: &'a [Session],
    work_dir: &WorkDir,
) -> SessionFacts<'a> {
    let session = session_for_stage(stage, sessions);
    let assigned = assigned_session(stage, sessions);
    // A resolver or judge speaking for the stage is the session a person
    // attaches to, so its type is the one reported; otherwise the assigned
    // session's.
    let session_type = session
        .filter(|s| {
            matches!(
                s.session_type,
                SessionType::Merge | SessionType::Adjudication
            )
        })
        .or(assigned)
        .or(session)
        .map(|s| s.session_type);
    let incoherence = executing_stage_incoherence(stage, assigned);
    let reading = reported_reading(session);
    let context_tokens = reading.map(|s| s.context_tokens);
    let context_ceiling_tokens = reading
        .map(|_| resolve_context_ceiling_tokens(work_dir.root(), stage.context_ceiling_tokens));
    let pid = session.and_then(|s| s.pid);
    let session_alive = pid.map(crate::process::is_process_alive).unwrap_or(false);
    let (merge_resolver_session, merge_resolver_attempts) =
        merge_resolver_facts(stage, sessions, work_dir);

    SessionFacts {
        session,
        session_type,
        incoherence,
        context_tokens,
        context_ceiling_tokens,
        pid,
        session_alive,
        merge_resolver_session,
        merge_resolver_attempts,
        completion_view: collect_completion_view(stage, assigned, work_dir),
        heartbeat: heartbeat_facts(stage, session, work_dir),
    }
}

/// The model this stage's session actually launches on.
///
/// `Stage::effective_model()` stops at the plan-field/stage-type tier and
/// ignores both config tiers, so it would show the dashboard a name the real
/// spawn (`native::launch::model_and_effort`) has already overridden — use
/// the same resolver that spawn calls instead.
fn resolved_model(stage: &Stage, work_dir: &WorkDir) -> String {
    crate::fs::work_dir::resolve_stage_model_effort(
        work_dir.root(),
        stage.stage_type,
        stage.model.as_deref(),
        stage.reasoning_effort.as_deref(),
    )
    .0
}

/// Build a StageSummary from a Stage and optional associated Session.
///
/// `rustfmt::skip` keeps two pairs of fields that come from one value on one
/// line each (the live timers, the completion view), holding the 47-field
/// literal inside the function-length limit.
#[rustfmt::skip]
fn build_stage_summary(stage: &Stage, sessions: &[Session], work_dir: &WorkDir) -> StageSummary {
    let (facts, now) = (session_facts(stage, sessions, work_dir), Utc::now());
    StageSummary {
        id: stage.id.clone(),
        name: stage.name.clone(),
        description: stage.description.clone(),
        summary: stage.summary.clone(),
        status: stage.status.clone(),
        stage_type: stage.stage_type,
        dependencies: stage.dependencies.clone(),
        context_tokens: facts.context_tokens,
        elapsed_secs: elapsed_secs_live(stage, now), execution_secs: execution_secs_live(stage, now),
        base_branch: stage.base_branch.clone(),
        base_merged_from: stage.base_merged_from.clone(),
        failure_info: stage.failure_info.clone(),
        activity_status: facts.heartbeat.activity_status,
        last_tool: facts.heartbeat.last_tool,
        last_activity: facts.heartbeat.last_activity,
        staleness_secs: facts.heartbeat.staleness_secs,
        context_ceiling_tokens: facts.context_ceiling_tokens,
        review_reason: stage.review_reason.clone(),
        review_notes: stage.review_reason.clone(),
        merged: stage.merged,
        merge_assumed: stage.merge_assumed,
        cleanup_warning: stage.cleanup_warning.clone(),
        merge_block: merge_block_text(stage),
        stash_warning: stash_warning(stage, work_dir),
        held: stage.held,
        retry_count: stage.retry_count,
        max_retries: stage.max_retries,
        pid: facts.pid,
        session_alive: facts.session_alive,
        model: resolved_model(stage, work_dir),
        session_type: facts.session_type,
        incoherence: facts.incoherence,
        execution_models: execution_models_for_stage(work_dir, &stage.id),
        dispute_count: crate::models::stage::dispute_budgets::filed_total(stage),
        judge_heartbeat_secs: facts.heartbeat.judge_heartbeat_secs,
        session_backend: facts.session.map(|s| s.backend),
        outgoing_session_exit_reason: facts.completion_view.0, completion_blocker: facts.completion_view.1,
        merge_resolver_session: facts.merge_resolver_session,
        merge_resolver_attempts: facts.merge_resolver_attempts,
        close_reason: blocked_close_reason(stage),
        acceptance: blocked_acceptance(stage),
        terminal_history_session: terminal_history_session(stage, sessions, work_dir),
        block_detail: block_detail(stage, sessions, work_dir),
        history: super::history::stage_history(stage, work_dir),
        contract_testing: contract_testing_for(stage, facts.session_type, work_dir, now),
    }
}

/// The freeze-job state, read only for a stage in the contract phase (the
/// mirror of `attention_model::is_contract_phase`) so no other stage pays for
/// the file reads.
fn contract_testing_for(
    stage: &Stage,
    session_type: Option<SessionType>,
    work_dir: &WorkDir,
    now: chrono::DateTime<Utc>,
) -> Option<ContractTesting> {
    let in_phase = stage.status == StageStatus::Executing
        && stage.stage_type == StageType::Standard
        && session_type == Some(SessionType::Contract);
    in_phase
        .then(|| {
            super::contract_testing::contract_testing(
                work_dir.root(),
                &stage.id,
                stage.session.as_deref(),
                stage.contracts.len(),
                now,
            )
        })
        .flatten()
}

/// What a blocked stage's panel needs beyond its summary line.
fn block_detail(stage: &Stage, sessions: &[Session], work_dir: &WorkDir) -> BlockDetail {
    BlockDetail {
        failing_criteria: failing_criteria(stage, sessions, work_dir),
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
    report: &crate::status::merge_status::MergeStatusReport,
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

#[path = "collector_snapshot.rs"]
mod snapshot;
pub use snapshot::collect_status_data;

#[cfg(test)]
#[path = "collector_activity_tests.rs"]
mod activity_tests;
#[cfg(test)]
#[path = "collector_history_tests.rs"]
mod history_tests;
#[cfg(test)]
#[path = "collector_resolver_tests.rs"]
mod resolver_tests;
#[cfg(test)]
#[path = "collector_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "collector_display_tests.rs"]
mod display_tests;
#[cfg(test)]
#[path = "inventory_tests.rs"]
mod inventory_tests;
