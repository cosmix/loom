//! Stage completion logic

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::daemon::DaemonServer;
use crate::fs::permissions::sync_worktree_permissions_with_working_dir;
use crate::fs::session_files::find_session_for_stage;
use crate::git::merge::{detect_in_progress_merge_at_worktree, ActiveMergeState, MergeLocation};
use crate::git::worktree::find_repo_root_from_cwd;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::merge_attribution::{attribute_main_repo_merge, MergeAttribution};
use crate::verify::transitions::{list_all_stages, load_stage, update_stage};

use super::acceptance_runner::{
    resolve_stage_execution_paths, run_acceptance_with_display, AcceptanceDisplayOptions,
};
use super::knowledge_complete::{complete_knowledge_stage, verify_knowledge_for_broker};
use super::merge_resolver::{spawn_merge_resolver, MergeResolverResult};
use super::merge_verify::verify_or_derive_completed_commit;
use super::progressive_complete::complete_with_merge;
use super::session::cleanup_session_resources;

#[path = "complete_authorization.rs"]
pub(super) mod complete_authorization;
#[path = "complete_cache.rs"]
mod complete_cache;
#[path = "complete_force.rs"]
mod complete_force;
#[path = "complete_verification.rs"]
mod complete_verification;
#[path = "control_complete.rs"]
pub(super) mod control_complete;
#[path = "control_session.rs"]
mod control_session;

use complete_authorization::authorize_privileged_completion;
use complete_cache::bypass_acceptance_cache;
use complete_force::handle_force_unsafe_completion;
use control_session::{handle_broker_request, sandbox_control_session};

pub(crate) use super::admin_proof::{
    mint_completion_proof_from_env, strip_privileged_env_for_runtime,
};

#[path = "complete_resolved.rs"]
mod complete_resolved;
pub(crate) use complete_resolved::verify_resolved_completion;

#[path = "complete_route.rs"]
mod complete_route;
use complete_route::{print_daemon_managed_notice, spawn_resolver_for_route};
pub use complete_route::{route_complete_for_conflicts, CompleteConflictRoute};
#[path = "complete_dispatch.rs"]
mod complete_dispatch;
#[path = "complete_pipeline.rs"]
mod complete_pipeline;

pub fn complete(
    stage_id: String,
    session_id: Option<String>,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
    no_cache: bool,
    admin_proof: Option<String>,
) -> Result<()> {
    let work_dir_buf = crate::commands::common::work_dir_path()?;
    let work_dir: &Path = &work_dir_buf;
    let is_privileged = no_verify || force_unsafe || assume_merged;
    if handle_broker_request(&stage_id, session_id.as_deref(), is_privileged, work_dir)? {
        return Ok(());
    }
    authorize_privileged_completion(
        &stage_id,
        no_verify,
        force_unsafe,
        assume_merged,
        admin_proof.as_deref(),
        work_dir,
    )?;

    let (stage, control_session) =
        prepare_completion(&stage_id, session_id.as_deref(), no_cache, work_dir)?;
    if stage.stage_type == StageType::Knowledge {
        return complete_knowledge(
            &stage,
            session_id.as_deref(),
            control_session.as_deref(),
            no_verify,
            force_unsafe,
            is_privileged,
            work_dir,
        );
    }

    complete_pipeline::complete_stage(
        stage,
        session_id,
        no_verify,
        force_unsafe,
        assume_merged,
        control_session,
        work_dir,
    )
}

fn prepare_completion(
    stage_id: &str,
    session_id: Option<&str>,
    no_cache: bool,
    work_dir: &Path,
) -> Result<(Stage, Option<String>)> {
    let stage = load_stage(stage_id, work_dir)?;
    bypass_acceptance_cache(no_cache);
    let control_session = sandbox_control_session(&stage, stage_id, session_id, work_dir)?;
    Ok((stage, control_session))
}

fn complete_knowledge(
    stage: &Stage,
    session_id: Option<&str>,
    control_session: Option<&str>,
    no_verify: bool,
    force_unsafe: bool,
    is_privileged: bool,
    work_dir: &Path,
) -> Result<()> {
    if let Some(control) = control_session {
        return verify_knowledge_for_broker(stage, control, is_privileged, work_dir);
    }
    complete_knowledge_stage(&stage.id, session_id, no_verify, force_unsafe)
}

/// Explains that verification passing on the sandboxed worktree route does NOT
/// mean the stage is completed — that transition is applied out-of-band by
/// the daemon via the completion bridge, not by this process.
pub(super) fn print_sandboxed_completion_pending_notice(stage_id: &str) {
    println!();
    println!(
        "Verification passed, but stage '{stage_id}' is NOT completed yet — \
         completion is applied out-of-band by the daemon via the completion \
         bridge, which reads the EOF-delimited evidence record below. Do not \
         treat this output as completion: the confirmation to \
         look for is the bridge's own message, \"Stage '{stage_id}' completion \
         was accepted by the daemon.\" If that confirmation never appears, the \
         stage is still Executing and this work is NOT landed."
    );
}

fn ensure_acceptance_passed(result: Option<bool>, stage_id: &str) -> Result<()> {
    if result == Some(false) {
        eprintln!("Acceptance criteria FAILED for stage '{stage_id}'");
        super::acceptance_runner::print_acceptance_failure_guidance(stage_id);
        bail!("Acceptance criteria failed for stage '{stage_id}'");
    }
    Ok(())
}

fn resolve_completion_route(
    stage: &Stage,
    force_unsafe: bool,
    assume_merged: bool,
    work_dir: &Path,
) -> Result<(CompleteConflictRoute, PathBuf)> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());
    let sessions = load_all_sessions_for_router(work_dir);
    let all_stages = list_all_stages(work_dir).unwrap_or_default();
    let route = route_complete_for_conflicts(
        stage,
        &sessions,
        &all_stages,
        &repo_root,
        work_dir,
        DaemonServer::is_running(work_dir),
        force_unsafe,
        assume_merged,
    )?;
    Ok((route, repo_root))
}

/// Best-effort load of all sessions for the router. Routing must not fail on
/// transient FS errors — fall back to an empty list (attribution then uses
/// commit-based matching).
fn load_all_sessions_for_router(work_dir: &Path) -> Vec<Session> {
    use crate::parser::frontmatter::parse_from_markdown;

    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        return Vec::new();
    }
    let entries = match std::fs::read_dir(&sessions_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(session) = parse_from_markdown::<Session>(&content, "Session") {
                sessions.push(session);
            }
        }
    }
    sessions
}

/// Sync worktree permissions with main repo
///
/// Ensures permissions are synced even if acceptance fails, allowing
/// approved permissions to persist for retry attempts.
pub(super) fn sync_worktree_permissions(
    working_dir: &Option<PathBuf>,
    acceptance_dir: &Option<PathBuf>,
) {
    if let Some(ref dir) = working_dir {
        // Find the main repo root from the worktree path
        let repo_root = find_repo_root_from_cwd(dir);

        if let Some(ref root) = repo_root {
            match sync_worktree_permissions_with_working_dir(dir, root, acceptance_dir.as_deref()) {
                Ok(result) => {
                    if result.allow_added > 0 || result.deny_added > 0 {
                        let mut msg = format!(
                            "Synced permissions from worktree: {} allow, {} deny",
                            result.allow_added, result.deny_added
                        );
                        if result.worktrees_updated > 0 {
                            msg.push_str(&format!(
                                " (propagated to {} other worktree{})",
                                result.worktrees_updated,
                                if result.worktrees_updated == 1 {
                                    ""
                                } else {
                                    "s"
                                }
                            ));
                        }
                        println!("{}", msg);
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Failed to sync worktree permissions: {e}");
                }
            }
        }
    }
}

/// Run acceptance criteria phase
///
/// Returns Some(true) if criteria passed, Some(false) if failed, None if skipped.
fn run_acceptance_phase(
    stage: &crate::models::stage::Stage,
    stage_id: &str,
    no_verify: bool,
    acceptance_dir: Option<&Path>,
    work_dir: &Path,
) -> Result<Option<bool>> {
    // Track whether acceptance criteria passed (None = skipped via --no-verify)
    let acceptance_result: Option<bool> = if no_verify {
        // --no-verify means we skip criteria entirely (deliberate skip)
        None
    } else {
        Some(run_acceptance_with_display(
            stage,
            stage_id,
            acceptance_dir,
            work_dir,
            AcceptanceDisplayOptions {
                stage_label: Some("stage"),
                show_empty_message: false,
            },
        )?)
    };

    Ok(acceptance_result)
}
