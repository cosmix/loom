//! Stage completion logic

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::daemon::DaemonServer;
use crate::fs::permissions::sync_worktree_permissions_with_working_dir;
use crate::fs::session_files::find_session_for_stage;
use crate::git::merge::{
    detect_in_progress_merge_at_worktree, ActiveMergeState, InProgressMerge, MergeLocation,
};
use crate::git::worktree::find_repo_root_from_cwd;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::merge_attribution::{attribute_main_repo_merge, MergeAttribution};
use crate::verify::transitions::{list_all_stages, load_stage, trigger_dependents, update_stage};

use super::acceptance_runner::{
    resolve_stage_execution_paths, run_acceptance_with_display, AcceptanceDisplayOptions,
};
use super::admin_proof::{verify_and_consume_admin_proof, AdminProofRequest};
use super::knowledge_complete::complete_knowledge_stage;
use super::merge_resolver::{spawn_merge_resolver, MergeResolverResult};
use super::merge_verify::verify_or_derive_completed_commit;
use super::progressive_complete::complete_with_merge;
use super::session::cleanup_session_resources;

#[path = "complete_verification.rs"]
mod complete_verification;
#[path = "control_complete.rs"]
mod control_complete;

pub(crate) use super::admin_proof::{
    mint_completion_proof_from_env, strip_privileged_env_for_runtime, take_admin_proof_from_env,
};

/// Verify and consume the caller-supplied proof for an exact completion request.
pub fn require_admin_capability(
    work_dir: &Path,
    stage_id: &str,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
    proof: Option<&str>,
) -> Result<()> {
    let request = AdminProofRequest::completion(stage_id, no_verify, force_unsafe, assume_merged);
    verify_and_consume_admin_proof(work_dir, request, proof)
}

/// Where `complete()` should dispatch after the active-merge / status / force
/// rules have been applied.
///
/// All variants are pure data — the router is read-only. The caller persists
/// any state changes ONLY on the success path so refusal preserves stage file
/// state.
#[derive(Debug, PartialEq, Eq)]
pub enum CompleteConflictRoute {
    /// Run the normal completion pipeline (acceptance, verify, progressive merge).
    Proceed,
    /// `--force-unsafe --assume-merged` with verified ancestry. If
    /// `derived_commit` is `Some`, the caller MUST persist it before calling
    /// `handle_force_unsafe_completion`.
    ForceUnsafeAssumeMergedVerified { derived_commit: Option<String> },
    /// `--force-unsafe` with no `--assume-merged` and no active merge — drop
    /// to `Completed + !merged` with the stale-flag warnings.
    ForceUnsafeAllowedStaleFlag,
    /// Daemon is running and owns merge resolution.
    DaemonManaged { stage_id: String },
    /// Stage is already in a conflict status; CLI should spawn (or report on)
    /// a resolver. Status contract for `spawn_merge_resolver` is satisfied.
    SpawnResolver {
        conflicting_files: Vec<String>,
        target_branch: String,
        in_progress: Option<InProgressMerge>,
    },
    /// Active main-repo merge attributed to this stage but the stage's status
    /// is not yet `MergeConflict | MergeBlocked`. Caller MUST persist
    /// `Completed → MergeConflict + merged=false + merge_conflict=true`
    /// before invoking `spawn_merge_resolver`.
    RevertAndSpawnResolver {
        conflicting_files: Vec<String>,
        target_branch: String,
        in_progress: InProgressMerge,
    },
    /// Refuse the operation. Caller prints `message` and exits non-zero.
    Refuse { message: String },
}

/// Pure routing helper — read-only. All persistence happens in `complete()`
/// on the success path so refusal preserves stage file state.
#[allow(clippy::too_many_arguments)]
pub fn route_complete_for_conflicts(
    stage: &Stage,
    sessions: &[Session],
    all_stages: &[Stage],
    repo_root: &Path,
    work_dir: &Path,
    daemon_running: bool,
    force_unsafe: bool,
    assume_merged: bool,
) -> Result<CompleteConflictRoute> {
    // Rule 1: Knowledge stages always proceed (no branch/merge state).
    if stage.stage_type == StageType::Knowledge {
        return Ok(CompleteConflictRoute::Proceed);
    }

    // Rule 2: Worktree active merge — refuse with location, never spawn.
    let worktree = repo_root.join(".worktrees").join(&stage.id);
    if worktree.exists() {
        if let Ok(Some(merge)) = detect_in_progress_merge_at_worktree(&worktree) {
            let location = match &merge.location {
                MergeLocation::Worktree { worktree_path, .. } => {
                    worktree_path.display().to_string()
                }
                MergeLocation::MainRepo { repo_path, .. } => repo_path.display().to_string(),
            };
            return Ok(CompleteConflictRoute::Refuse {
                message: format!(
                    "Worktree `{location}` has an active merge. Run `cd {location} && \
                     git merge --abort` (or commit) before completing the stage."
                ),
            });
        }
    }

    // Rule 3+4: Main-repo active merge attribution.
    let attribution = attribute_main_repo_merge(repo_root, work_dir, all_stages, sessions)?;

    if let MergeAttribution::Attributed {
        stage_id: attributed_id,
        source,
        ..
    } = &attribution
    {
        if attributed_id != &stage.id {
            return Ok(CompleteConflictRoute::Refuse {
                message: format!(
                    "An active merge in the main repo is attributed to stage '{}' \
                     (via {:?}); refusing to complete stage '{}'. Resolve that merge \
                     first.",
                    attributed_id, source, stage.id
                ),
            });
        }
    }
    if let MergeAttribution::GlobalUnattributed(merge) = &attribution {
        let location = match &merge.location {
            MergeLocation::MainRepo { repo_path, .. } => repo_path.display().to_string(),
            MergeLocation::Worktree { worktree_path, .. } => worktree_path.display().to_string(),
        };
        return Ok(CompleteConflictRoute::Refuse {
            message: format!(
                "Active merge at `{location}` cannot be attributed to any known stage \
                 (or is a base-branch merge). Resolve or abort it manually before \
                 completing any stage."
            ),
        });
    }

    let attributed_to_this_stage = matches!(
        &attribution,
        MergeAttribution::Attributed { stage_id, .. } if stage_id == &stage.id
    );

    // Rule 5: --force-unsafe --assume-merged dominates the status reroute so
    // verified force-completes still work on MergeConflict stages.
    if force_unsafe && assume_merged {
        let target_branch = crate::fs::resolve_target_branch_from_config(work_dir, repo_root)?;
        let verified = verify_or_derive_completed_commit(stage, &target_branch, repo_root)
            .map_err(|e| anyhow::anyhow!("--assume-merged refused: {e}"));
        return match verified {
            Ok(v) => Ok(CompleteConflictRoute::ForceUnsafeAssumeMergedVerified {
                derived_commit: v.persist_commit,
            }),
            Err(e) => Ok(CompleteConflictRoute::Refuse {
                message: e.to_string(),
            }),
        };
    }

    // Rule 6: --force-unsafe alone — refuse if there's an active merge for
    // THIS stage (the merge hasn't actually happened); else allow stale-flag
    // drop.
    if force_unsafe {
        if attributed_to_this_stage {
            return Ok(CompleteConflictRoute::Refuse {
                message: format!(
                    "--force-unsafe refused: stage '{}' has an active merge in progress. \
                     Bypassing here would orphan MERGE_HEAD. Use --assume-merged with \
                     a verified commit, or resolve the merge first.",
                    stage.id
                ),
            });
        }
        return Ok(CompleteConflictRoute::ForceUnsafeAllowedStaleFlag);
    }

    let attributed_merge: Option<&InProgressMerge> = match &attribution {
        MergeAttribution::Attributed { merge, .. } => Some(merge),
        _ => None,
    };

    // Rule 7: stage status already in conflict status — daemon-managed or spawn.
    if matches!(
        stage.status,
        StageStatus::MergeConflict | StageStatus::MergeBlocked
    ) {
        if daemon_running {
            return Ok(CompleteConflictRoute::DaemonManaged {
                stage_id: stage.id.clone(),
            });
        }
        let target_branch = crate::fs::resolve_target_branch_from_config(work_dir, repo_root)?;
        let conflicting_files = match attributed_merge.map(|m| &m.state) {
            Some(ActiveMergeState::HasUnmergedPaths(paths)) => paths.clone(),
            _ => Vec::new(),
        };
        return Ok(CompleteConflictRoute::SpawnResolver {
            conflicting_files,
            target_branch,
            in_progress: attributed_merge.cloned(),
        });
    }

    // Rule 8: status not yet in conflict but an attributed main-repo merge is
    // active — daemon will reconcile, otherwise CLI must do the revert.
    if attributed_to_this_stage {
        if daemon_running {
            return Ok(CompleteConflictRoute::DaemonManaged {
                stage_id: stage.id.clone(),
            });
        }
        let merge = attributed_merge
            .cloned()
            .expect("attributed_to_this_stage implies merge");
        let target_branch = crate::fs::resolve_target_branch_from_config(work_dir, repo_root)?;
        let conflicting_files = match &merge.state {
            ActiveMergeState::HasUnmergedPaths(paths) => paths.clone(),
            ActiveMergeState::ResolvedButUncommitted => Vec::new(),
        };
        return Ok(CompleteConflictRoute::RevertAndSpawnResolver {
            conflicting_files,
            target_branch,
            in_progress: merge,
        });
    }

    // Rule 9: default — proceed with the normal completion pipeline.
    Ok(CompleteConflictRoute::Proceed)
}

/// Spawn a CLI-side merge resolver for a route that already satisfies the
/// `MergeConflict | MergeBlocked` status contract on disk.
fn spawn_resolver_for_route(
    stage: &Stage,
    conflicting_files: &[String],
    target_branch: &str,
    in_progress: Option<InProgressMerge>,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<()> {
    match spawn_merge_resolver(
        stage,
        conflicting_files,
        target_branch,
        in_progress,
        repo_root,
        work_dir,
    )? {
        MergeResolverResult::DaemonManaged => {
            println!(
                "Daemon is handling merge resolution for stage '{}'.",
                stage.id
            );
        }
        MergeResolverResult::Spawned(id) => {
            println!("Spawned merge resolver session: {id}");
        }
        MergeResolverResult::AlreadyRunning { session_id } => {
            println!(
                "A merge resolver session is already running for stage '{}': {session_id}. \
                 Wait for it to complete, or run `loom sessions kill {session_id}` to abort.",
                stage.id
            );
        }
    }
    Ok(())
}

/// Mark a stage as complete, optionally running acceptance criteria.
/// If acceptance criteria pass, auto-verifies the stage and triggers dependents.
/// If --no-verify is used or criteria fail, marks as CompletedWithFailures for retry.
/// If --force-unsafe is used, bypasses state machine and marks stage as Completed from any state.
pub fn complete(
    stage_id: String,
    session_id: Option<String>,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
    admin_proof: Option<String>,
) -> Result<()> {
    let work_dir = Path::new(".work");
    if handle_broker_request(
        &stage_id,
        session_id.as_deref(),
        no_verify || force_unsafe || assume_merged,
        work_dir,
    )? {
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

    let mut stage = load_stage(&stage_id, work_dir)?;

    let control_session =
        sandbox_control_session(&stage, &stage_id, session_id.as_deref(), work_dir)?;

    // Route knowledge stages to specialized completion (no merge required).
    // Knowledge stages have no branch and no merge state, so the conflict
    // router is irrelevant.
    if stage.stage_type == StageType::Knowledge {
        if control_session.is_some() {
            bail!("sandboxed completion supports worktree stages only");
        }
        return complete_knowledge_stage(&stage_id, session_id.as_deref(), no_verify, force_unsafe);
    }

    let (route, repo_root) =
        resolve_completion_route(&stage, force_unsafe, assume_merged, work_dir)?;

    // A sandboxed wrapper may perform verification only. Resolver spawning,
    // phantom-merge repair, and administrative completion all mutate trusted
    // state before the verification marker and therefore belong to the host
    // orchestrator, not this route.
    if control_session.is_some() && !matches!(&route, CompleteConflictRoute::Proceed) {
        bail!("sandboxed completion cannot perform conflict or recovery operations");
    }

    match route {
        CompleteConflictRoute::Proceed => {
            // Fall through to the normal completion pipeline below.
        }
        CompleteConflictRoute::ForceUnsafeAssumeMergedVerified { derived_commit } => {
            // Carry the derived commit in-memory; handle_force_unsafe_completion
            // re-applies it (and the forced status/merged) onto the fresh on-disk
            // stage via update_stage. Persisting only on this success path keeps
            // the "refusal preserves stage file state" invariant (A-5: no early
            // whole-Stage save that could revert a concurrent writer).
            if let Some(commit) = derived_commit {
                stage.completed_commit = Some(commit);
            }
            return handle_force_unsafe_completion(stage, &stage_id, true, work_dir);
        }
        CompleteConflictRoute::ForceUnsafeAllowedStaleFlag => {
            return handle_force_unsafe_completion(stage, &stage_id, false, work_dir);
        }
        CompleteConflictRoute::DaemonManaged {
            stage_id: managed_id,
        } => {
            println!(
                "Daemon is handling merge resolution for stage '{managed_id}'. \
                 Run `loom status` to monitor."
            );
            return Ok(());
        }
        CompleteConflictRoute::SpawnResolver {
            conflicting_files,
            target_branch,
            in_progress,
        } => {
            return spawn_resolver_for_route(
                &stage,
                &conflicting_files,
                &target_branch,
                in_progress,
                &repo_root,
                work_dir,
            );
        }
        CompleteConflictRoute::RevertAndSpawnResolver {
            conflicting_files,
            target_branch,
            in_progress,
        } => {
            // Phantom-merge revert (CLI parity with daemon's
            // reconcile_main_repo_active_merge): persist BEFORE spawn so the
            // resolver-spawn status contract is satisfied.
            // force_status_with_reason is appropriate here: Completed is a
            // terminal state, so try_mark_merge_conflict() would refuse; but this
            // is a legitimate forced revert when an active merge is detected.
            // Re-apply only the revert-owned fields (status, merged,
            // merge_conflict) onto the fresh on-disk stage so the revert does not
            // clobber concurrent writes to unrelated fields (A-5).
            stage.force_status_with_reason(
                StageStatus::MergeConflict,
                "phantom-merge revert: active merge detected for stage in non-conflict status",
            );
            stage.merged = false;
            stage.merge_conflict = true;
            update_stage(&stage_id, work_dir, |s| {
                s.force_status_with_reason(
                    StageStatus::MergeConflict,
                    "phantom-merge revert: active merge detected for stage in non-conflict status",
                );
                s.merged = false;
                s.merge_conflict = true;
                Ok(())
            })?;
            return spawn_resolver_for_route(
                &stage,
                &conflicting_files,
                &target_branch,
                Some(in_progress),
                &repo_root,
                work_dir,
            );
        }
        CompleteConflictRoute::Refuse { message } => bail!("{message}"),
    }

    // ----- Proceed path: normal completion pipeline below -----

    // Resolve session_id: CLI arg > stage.session field > scan sessions directory
    let session_id = session_id
        .or_else(|| stage.session.clone())
        .or_else(|| find_session_for_stage(&stage_id, work_dir));

    // Resolve worktree and acceptance execution paths using shared logic
    let execution_paths = resolve_stage_execution_paths(&stage)?;
    let working_dir: Option<PathBuf> = execution_paths.worktree_root;
    let acceptance_dir: Option<PathBuf> = execution_paths.acceptance_dir;

    // Permission fold-back writes the main repository and sibling worktrees.
    // Keep that host-side behavior for ordinary completion, but never perform
    // it from a sandboxed wrapper whose only authority is verification.
    if control_session.is_none() {
        sync_worktree_permissions(&working_dir, &acceptance_dir);
    }

    // Run acceptance criteria phase
    let acceptance_result =
        run_acceptance_phase(&stage, &stage_id, no_verify, acceptance_dir.as_deref())?;

    ensure_acceptance_passed(acceptance_result, &stage_id)?;

    // Run verification and merge phase
    run_verification_phase(VerificationPhase {
        stage: &mut stage,
        stage_id: &stage_id,
        no_verify,
        acceptance_dir: &acceptance_dir,
        worktree_root: &working_dir,
        session_id: session_id.as_deref(),
        control_session: control_session.as_deref(),
        work_dir,
    })?;

    Ok(())
}

fn handle_broker_request(
    stage_id: &str,
    session_id: Option<&str>,
    has_privileged_flags: bool,
    work_dir: &Path,
) -> Result<bool> {
    if !control_complete::broker_requested() {
        return Ok(false);
    }
    if has_privileged_flags {
        bail!("trusted completion broker does not accept privileged flags");
    }
    let session_id = session_id.context("trusted completion broker requires --session")?;
    require_wrapper_identity(stage_id, session_id)?;
    control_complete::send_completion(stage_id, session_id, work_dir)?;
    Ok(true)
}

fn ensure_acceptance_passed(result: Option<bool>, stage_id: &str) -> Result<()> {
    if result == Some(false) {
        eprintln!("Acceptance criteria FAILED for stage '{stage_id}'");
        eprintln!("  Fix the issues and run 'loom stage complete {stage_id}' again");
        bail!("Acceptance criteria failed for stage '{stage_id}'");
    }
    Ok(())
}

fn authorize_privileged_completion(
    stage_id: &str,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
    proof: Option<&str>,
    work_dir: &Path,
) -> Result<()> {
    if no_verify || force_unsafe || assume_merged {
        require_admin_capability(
            work_dir,
            stage_id,
            no_verify,
            force_unsafe,
            assume_merged,
            proof,
        )?;
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

fn require_wrapper_identity(stage_id: &str, session_id: &str) -> Result<()> {
    let env_stage = std::env::var("LOOM_STAGE_ID").context("LOOM_STAGE_ID is missing")?;
    let env_session = std::env::var("LOOM_SESSION_ID").context("LOOM_SESSION_ID is missing")?;
    if env_stage != stage_id || env_session != session_id {
        bail!("trusted completion broker identity does not match wrapper identity");
    }
    Ok(())
}

fn sandbox_control_session(
    stage: &Stage,
    stage_id: &str,
    requested_session: Option<&str>,
    work_dir: &Path,
) -> Result<Option<String>> {
    let (Ok(env_stage), Ok(env_session), Ok(worktree)) = (
        std::env::var("LOOM_STAGE_ID"),
        std::env::var("LOOM_SESSION_ID"),
        std::env::var("LOOM_WORKTREE_PATH"),
    ) else {
        return Ok(None);
    };
    if env_stage != stage_id || stage.session.as_deref() != Some(env_session.as_str()) {
        bail!("completion request does not match the active wrapper stage/session");
    }
    if requested_session.is_some_and(|requested| requested != env_session) {
        bail!("--session does not match the active wrapper session");
    }
    let cwd = std::env::current_dir().context("failed to resolve completion working directory")?;
    let worktree = PathBuf::from(worktree)
        .canonicalize()
        .context("failed to resolve LOOM_WORKTREE_PATH")?;
    let cwd = cwd
        .canonicalize()
        .context("failed to resolve current directory")?;
    if !cwd.starts_with(&worktree) {
        bail!("wrapper completion must run inside its assigned worktree");
    }
    if !DaemonServer::is_running(work_dir) {
        bail!("sandboxed worktree completion requires the loom daemon to be running");
    }
    Ok(Some(env_session))
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

/// Handle force-unsafe completion mode.
///
/// Bypasses state machine validation and marks stage as completed directly.
/// This is a manual recovery command for administrative use only.
///
/// # Invariant
///
/// **Callers MUST invoke `route_complete_for_conflicts` first.** This function
/// performs no ancestry check on its own — the verified-route guarantees the
/// router already established that the commit is in the target branch's
/// history (when `assume_merged=true`) or that no active merge would be
/// orphaned (when `assume_merged=false`).
fn handle_force_unsafe_completion(
    mut stage: crate::models::stage::Stage,
    stage_id: &str,
    assume_merged: bool,
    work_dir: &Path,
) -> Result<()> {
    eprintln!();
    eprintln!("⚠️  WARNING: Using --force-unsafe bypasses state machine validation!");
    eprintln!("⚠️  This can corrupt dependency tracking and cause unexpected behavior.");
    eprintln!("⚠️  Use only for manual recovery scenarios.");
    eprintln!();

    // Best-effort permission sync before force-completing
    // Uses resolve_stage_execution_paths to get worktree paths, same as normal completion
    if let Ok(execution_paths) = resolve_stage_execution_paths(&stage) {
        sync_worktree_permissions(
            &execution_paths.worktree_root,
            &execution_paths.acceptance_dir,
        );
    }

    println!(
        "Force-completing stage '{}' (was: {:?})",
        stage_id, stage.status
    );

    // Forced status assignment: --force-unsafe is an explicit administrative
    // override that may be invoked from any source status. Use
    // force_status_with_reason so the bypass is logged and visible.
    stage.force_status_with_reason(
        StageStatus::Completed,
        "--force-unsafe: administrative force-completion from any state",
    );

    // Only set merged=true if explicitly requested via --assume-merged
    if assume_merged {
        stage.merged = true;
        println!("  → Stage marked as merged (manual merge assumed)");
    } else {
        stage.merged = false;
        eprintln!();
        eprintln!("⚠️  WARNING: Stage NOT marked as merged (--assume-merged not provided).");
        eprintln!("⚠️  Dependent stages will NOT be automatically triggered.");
        eprintln!("⚠️  If you manually merged the branch, re-run with --assume-merged to trigger dependents.");
        eprintln!();
    }

    // Re-apply only the force-completion-owned fields (forced status, merged,
    // completed_commit) onto the FRESH on-disk stage so an administrative
    // force-complete does not revert concurrent daemon/dispute writes to
    // unrelated fields (A-5). force_status_with_reason bypasses transition
    // validation by design — this is the documented administrative override.
    let forced_merged = stage.merged;
    let forced_commit = stage.completed_commit.clone();
    update_stage(stage_id, work_dir, |s| {
        s.force_status_with_reason(
            StageStatus::Completed,
            "--force-unsafe: administrative force-completion from any state",
        );
        s.merged = forced_merged;
        s.completed_commit = forced_commit.clone();
        Ok(())
    })?;
    println!("Stage '{stage_id}' force-completed!");

    // Only trigger dependent stages if merged=true (i.e., --assume-merged was used)
    if stage.merged {
        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());
        let target_branch = crate::fs::resolve_target_branch_from_config(work_dir, &repo_root)?;
        let triggered = trigger_dependents(stage_id, work_dir, &repo_root, &target_branch)
            .context("Failed to trigger dependent stages")?;

        if !triggered.is_empty() {
            println!("Triggered {} dependent stage(s):", triggered.len());
            for dep_id in &triggered {
                println!("  → {dep_id}");
            }
        }
    }

    Ok(())
}

/// Sync worktree permissions with main repo
///
/// Ensures permissions are synced even if acceptance fails, allowing
/// approved permissions to persist for retry attempts.
fn sync_worktree_permissions(working_dir: &Option<PathBuf>, acceptance_dir: &Option<PathBuf>) {
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
            AcceptanceDisplayOptions {
                stage_label: Some("stage"),
                show_empty_message: false,
            },
        )?)
    };

    Ok(acceptance_result)
}

/// Run verification phase (goal-backward verification and change impact comparison)
///
/// If verifications pass, performs progressive merge. If --no-verify is used, skips all checks.
struct VerificationPhase<'a> {
    stage: &'a mut Stage,
    stage_id: &'a str,
    no_verify: bool,
    acceptance_dir: &'a Option<PathBuf>,
    worktree_root: &'a Option<PathBuf>,
    session_id: Option<&'a str>,
    control_session: Option<&'a str>,
    work_dir: &'a Path,
}

fn run_verification_phase(phase: VerificationPhase<'_>) -> Result<()> {
    let VerificationPhase {
        stage,
        stage_id,
        no_verify,
        acceptance_dir,
        worktree_root,
        session_id,
        control_session,
        work_dir,
    } = phase;
    if !no_verify {
        complete_verification::run(&complete_verification::VerificationChecks {
            stage,
            stage_id,
            acceptance_dir: acceptance_dir.as_deref(),
            worktree_root: worktree_root.as_deref(),
            control_session,
            work_dir,
        })?;

        if let Some(control_session) = control_session {
            println!(
                "{} stage={} session={}",
                control_complete::VERIFIED_MARKER,
                stage_id,
                control_session
            );
            return Ok(());
        }

        // All verifications passed - NOW clean up session resources
        if let Some(sid) = session_id {
            cleanup_session_resources(stage_id, sid, work_dir);
        }

        // Attempt progressive merge into the merge point (base_branch)
        // Find the main repo root (not the worktree root) for merge operations.
        // When running from within a worktree, we need to merge from the main repo.
        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());

        complete_with_merge(stage, &repo_root, work_dir)?;
    } else {
        // --no-verify: Skip verifications, just mark as completed.
        //
        // Phantom-merge guard: refuse if the stage's branch has zero commits
        // beyond the merge target. Otherwise the daemon's auto-merge will
        // "succeed" trivially (branch HEAD == target HEAD) and write
        // merged=true for work that was never committed. Knowledge stages
        // commit directly to base (no branch) so this check does not apply
        // — but knowledge stages are routed earlier in complete().
        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());
        let target_branch = crate::fs::resolve_target_branch_from_config(work_dir, &repo_root)?;
        // Skip the guard if the branch doesn't exist on the host — the shape
        // that shape unit tests (no real git repo) take. The phantom-merge
        // class of bug requires an EXISTING empty branch: attempt_auto_merge
        // happily fast-forwards to itself.
        let stage_branch = crate::git::branch::branch_name_for_stage(stage_id);
        let branch_exists =
            crate::git::branch::branch_exists(&stage_branch, &repo_root).unwrap_or(false);
        if branch_exists {
            match crate::git::branch::commits_ahead_of(&stage_branch, &target_branch, &repo_root) {
                Ok(0) => {
                    anyhow::bail!(
                        "Refusing to --no-verify-complete stage '{stage_id}': branch \
                         '{stage_branch}' has zero commits beyond '{target_branch}'. \
                         The agent never committed any work for this stage, so \
                         completing now would create a phantom merge (merged=true \
                         against the unchanged base). Either redo the stage so the \
                         agent commits real work, run `loom stage reset --kill-session \
                         {stage_id}` to kill the session and re-queue, or use \
                         `loom stage complete --force-unsafe` if you genuinely intend \
                         to mark an empty stage complete."
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!(
                        "Warning: failed to count commits ahead of '{target_branch}' on \
                         '{stage_branch}': {e}. Proceeding with --no-verify completion."
                    );
                }
            }
        }

        // The orchestrator daemon will auto-merge and trigger dependents.
        // Re-apply only the Completed transition onto the FRESH on-disk stage so
        // a concurrent daemon/dispute write is not reverted (A-5). merged stays
        // whatever it was on disk (normally false here; daemon auto-merges).
        stage.try_complete(None)?;
        update_stage(stage_id, work_dir, |s| s.try_complete(None))?;
        println!("Stage '{stage_id}' completed (skipped verification).");
        println!("The orchestrator will handle merge and dependent triggering.");
    }

    Ok(())
}
