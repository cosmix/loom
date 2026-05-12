//! Stage completion logic

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::commands::verify::load_stage_definition_from_plan;
use crate::daemon::DaemonServer;
use crate::fs::permissions::sync_worktree_permissions_with_working_dir;
use crate::fs::session_files::find_session_for_stage;
use crate::fs::work_dir::load_config;
use crate::git::merge::{
    detect_in_progress_merge_at_worktree, ActiveMergeState, InProgressMerge, MergeLocation,
};
use crate::git::worktree::find_repo_root_from_cwd;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::merge_attribution::{attribute_main_repo_merge, MergeAttribution};
use crate::plan::parser::{parse_plan, ParsedPlan};
use crate::plan::schema::{ChangeImpactConfig, ChangeImpactPolicy};
use crate::verify::baseline::compare_to_baseline;
use crate::verify::duplicate_detection::detect_duplicate_symbols;
use crate::verify::transitions::{list_all_stages, load_stage, save_stage, trigger_dependents};
use crate::verify::wiring_detection::detect_unwired_files;

use super::acceptance_runner::{
    resolve_stage_execution_paths, run_acceptance_with_display, AcceptanceDisplayOptions,
};
use super::knowledge_complete::complete_knowledge_stage;
use super::merge_resolver::{spawn_merge_resolver, MergeResolverResult};
use super::merge_verify::verify_or_derive_completed_commit;
use super::progressive_complete::complete_with_merge;
use super::session::cleanup_session_resources;

/// Load the full parsed plan from config
fn load_parsed_plan(work_dir: &Path) -> Result<Option<ParsedPlan>> {
    let source_path = match crate::fs::resolve_source_path(work_dir)? {
        Some(path) => path,
        None => return Ok(None),
    };

    // Parse the plan
    let parsed_plan = parse_plan(&source_path)
        .with_context(|| format!("Failed to parse plan: {}", source_path.display()))?;

    Ok(Some(parsed_plan))
}

/// Resolve the base branch from config, falling back to "main"
fn resolve_base_branch(work_dir: &Path) -> String {
    load_config(work_dir)
        .ok()
        .flatten()
        .and_then(|c| c.base_branch())
        .unwrap_or_else(|| "main".to_string())
}

/// Load change impact config from the active plan
fn load_change_impact_config(work_dir: &Path) -> Result<Option<ChangeImpactConfig>> {
    let parsed_plan = match load_parsed_plan(work_dir)? {
        Some(plan) => plan,
        None => return Ok(None),
    };

    Ok(parsed_plan.metadata.loom.change_impact)
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
        let target_branch = crate::git::branch::resolve_target_branch(
            &Some(resolve_base_branch(work_dir)),
            repo_root,
        );
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
        let target_branch = crate::git::branch::resolve_target_branch(
            &Some(resolve_base_branch(work_dir)),
            repo_root,
        );
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
        let target_branch = crate::git::branch::resolve_target_branch(
            &Some(resolve_base_branch(work_dir)),
            repo_root,
        );
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
) -> Result<()> {
    let work_dir = Path::new(".work");

    // Stage 4 (isolated-git-architecture): when running inside a
    // container-backed session, `loom stage complete` does NOT mutate
    // host stage state directly. Instead it delegates to the daemon
    // over the existing Unix-socket RPC; the daemon extracts a git
    // bundle from the LIVE container, validates it, imports it into
    // the host repo, runs auto_merge, and only then kills the
    // container and finalizes stage state. This is the architectural
    // fix for Codex blockers B1 (no .git mount), B2 (no in-container
    // stage_file mutation) and B6 (container stays alive until
    // extraction succeeds).
    //
    // `--force-unsafe` bypasses delegation and goes through the
    // legacy local path so administrators can recover stuck states
    // even when the daemon is unhealthy.
    if !force_unsafe && is_container_completion() {
        return delegate_completion_to_daemon(&stage_id, session_id.as_deref(), work_dir);
    }

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Route knowledge stages to specialized completion (no merge required).
    // Knowledge stages have no branch and no merge state, so the conflict
    // router is irrelevant.
    if stage.stage_type == StageType::Knowledge {
        return complete_knowledge_stage(&stage_id, session_id.as_deref(), no_verify, force_unsafe);
    }

    // Determine routing based on git/state inspection.
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());
    let daemon_running = DaemonServer::is_running(work_dir);
    let sessions = load_all_sessions_for_router(work_dir);
    let all_stages = list_all_stages(work_dir).unwrap_or_default();

    let route = route_complete_for_conflicts(
        &stage,
        &sessions,
        &all_stages,
        &repo_root,
        work_dir,
        daemon_running,
        force_unsafe,
        assume_merged,
    )?;

    match route {
        CompleteConflictRoute::Proceed => {
            // Fall through to the normal completion pipeline below.
        }
        CompleteConflictRoute::ForceUnsafeAssumeMergedVerified { derived_commit } => {
            // Persist derived commit only on the success path so refusal
            // preserves stage file state.
            if let Some(commit) = derived_commit {
                stage.completed_commit = Some(commit);
                save_stage(&stage, work_dir)?;
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
            tracing::error!(
                stage_id = %stage.id,
                prior_status = ?stage.status,
                "Detected active merge for stage in non-conflict status; \
                 reverting to MergeConflict + merged=false (phantom-merge revert)."
            );
            stage.status = StageStatus::MergeConflict;
            stage.merged = false;
            stage.merge_conflict = true;
            save_stage(&stage, work_dir)?;
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

    // Sync worktree permissions before running acceptance criteria
    sync_worktree_permissions(&working_dir, &acceptance_dir);

    // Run acceptance criteria phase
    let acceptance_result =
        run_acceptance_phase(&stage, &stage_id, no_verify, acceptance_dir.as_deref())?;

    // Handle acceptance failure - keep stage in Executing, agent can fix and retry
    // Do NOT transition state - stage stays Executing so agent can fix and re-run
    // Do NOT clean up session - agent is still alive
    if acceptance_result == Some(false) {
        eprintln!("Acceptance criteria FAILED for stage '{stage_id}'");
        eprintln!("  Fix the issues and run 'loom stage complete {stage_id}' again");
        anyhow::bail!("Acceptance criteria failed for stage '{stage_id}'");
    }

    // Run verification and merge phase
    run_verification_phase(
        &mut stage,
        &stage_id,
        no_verify,
        &acceptance_dir,
        session_id.as_deref(),
        work_dir,
    )?;

    Ok(())
}

/// Detect whether `loom stage complete` is running inside a
/// container-backed session.
///
/// The container backend sets `LOOM_BACKEND=container` in
/// [`ContainerBackend::build_env_for_session`]; everywhere else
/// (native sessions, host operator shells, CI) the variable is unset.
/// The check is deliberately just on this single env var — no
/// filesystem probing, no socket probing, no inheritance from parent
/// shells beyond what the orchestrator itself sets.
pub fn is_container_completion() -> bool {
    std::env::var("LOOM_BACKEND")
        .map(|v| v.eq_ignore_ascii_case("container"))
        .unwrap_or(false)
}

/// Send a [`Request::CompleteStageContainer`](crate::daemon::Request::CompleteStageContainer)
/// to the host daemon and surface its response.
///
/// `expected_base_oid` and `target_branch` are read from
/// `LOOM_BASE_OID` and `LOOM_BRANCH`, both populated by the
/// orchestrator at container spawn time. Missing either is a HARD
/// error — without them the daemon cannot validate the bundle.
fn delegate_completion_to_daemon(
    stage_id: &str,
    session_id: Option<&str>,
    work_dir: &Path,
) -> Result<()> {
    use crate::daemon::{read_message, read_user_token, write_message, Request, Response};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let target_branch = std::env::var("LOOM_BRANCH").context(
        "LOOM_BRANCH not set — required for container-mode completion. \
         The orchestrator should set this when spawning a container session.",
    )?;
    let expected_base_oid = std::env::var("LOOM_BASE_OID").context(
        "LOOM_BASE_OID not set — required for container-mode completion. \
         The orchestrator should set this when spawning a container session.",
    )?;
    let session_id = session_id
        .map(|s| s.to_string())
        .or_else(|| std::env::var("LOOM_SESSION_ID").ok())
        .context("No session_id and LOOM_SESSION_ID is not set")?;
    let auth_token = read_user_token(work_dir)
        .context("Failed to read .work/user.token for daemon authentication")?;

    let req = Request::CompleteStageContainer {
        auth_token,
        stage_id: stage_id.to_string(),
        session_id,
        target_branch,
        expected_base_oid,
    };

    let socket_path = work_dir.join("orchestrator.sock");
    let mut stream = UnixStream::connect(&socket_path)
        .with_context(|| format!("Failed to connect to daemon at {}", socket_path.display()))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .context("Failed to set socket read timeout")?;

    write_message(&mut stream, &req).context("Failed to send CompleteStageContainer request")?;
    let response: Response = read_message(&mut stream).context("Failed to read daemon response")?;

    match response {
        Response::Ok => {
            println!("Stage '{stage_id}' completed via daemon (host-authoritative).");
            Ok(())
        }
        Response::Error { message } => bail!("Daemon refused stage completion: {message}"),
        Response::AuthenticationFailed => {
            bail!("Daemon authentication failed — check .work/user.token")
        }
        other => bail!("Unexpected daemon response to CompleteStageContainer: {other:?}"),
    }
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

    // INTENTIONAL STATE MACHINE BYPASS: This is a manual recovery command
    // that allows administrators to force completion from any state.
    // This is the ONLY place where direct status assignment is acceptable.
    stage.status = StageStatus::Completed;

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

    save_stage(&stage, work_dir)?;
    println!("Stage '{stage_id}' force-completed!");

    // Only trigger dependent stages if merged=true (i.e., --assume-merged was used)
    if stage.merged {
        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());
        let target_branch = resolve_base_branch(work_dir);
        let target_branch =
            crate::git::branch::resolve_target_branch(&Some(target_branch), &repo_root);
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

/// Check if a stage has downstream dependents (other stages that depend on it)
fn has_downstream_dependents(stage_id: &str, work_dir: &Path) -> bool {
    // Load all stages and check if any depend on this one
    match crate::verify::transitions::list_all_stages(work_dir) {
        Ok(stages) => stages
            .iter()
            .any(|s| s.dependencies.contains(&stage_id.to_string())),
        Err(_) => false, // Conservative: assume no dependents
    }
}

/// Check if memory entries mention unwired files
fn check_memory_covers_unwired(
    stage_id: &str,
    unwired_files: &[crate::verify::wiring_detection::UnwiredFile],
    work_dir: &Path,
) -> bool {
    let memory_path = work_dir.join("memory").join(format!("{stage_id}.md"));
    let memory_content = match std::fs::read_to_string(&memory_path) {
        Ok(content) => content.to_lowercase(),
        Err(_) => return false,
    };

    // Check for general wiring-related terms in memory
    let wiring_keywords = [
        "wire",
        "wiring",
        "register",
        "mount",
        "import",
        "downstream",
        "integrate",
    ];
    let has_wiring_context = wiring_keywords.iter().any(|kw| memory_content.contains(kw));

    // Check if memory mentions any of the unwired files by name or path
    let mentions_any_file = unwired_files.iter().any(|uf| {
        let name = uf.importable_name.to_lowercase();
        let path = uf.path.to_lowercase();
        memory_content.contains(&name) || memory_content.contains(&path)
    });

    has_wiring_context || mentions_any_file
}

/// Run aggregated wiring re-verification across all completed stages
///
/// When completing an integration-verify stage, re-runs all wiring checks
/// from all prior stages on the merged codebase.
fn run_aggregated_wiring_reverification(
    _stage_id: &str,
    verification_dir: &Path,
    work_dir: &Path,
) -> Result<()> {
    // Load all stages
    let stages = crate::verify::transitions::list_all_stages(work_dir)?;

    // Load plan to get stage definitions with wiring checks
    let parsed_plan = load_parsed_plan(work_dir)?;
    let plan = match parsed_plan {
        Some(plan) => plan,
        None => {
            eprintln!("Warning: Could not load plan for aggregated wiring verification");
            return Ok(());
        }
    };

    let mut all_gaps = Vec::new();

    for stage in &stages {
        // Skip non-completed stages and stages without wiring checks in the plan
        if stage.status != StageStatus::Completed {
            continue;
        }

        // Find the stage definition in the plan
        if let Some(stage_def) = plan.metadata.loom.stages.iter().find(|s| s.id == stage.id) {
            // Re-run wiring checks from this stage on the merged codebase.
            // Wiring source paths are authored relative to the originating
            // stage's working_dir, NOT the integration-verify stage's
            // working_dir — resolve against `verification_dir + working_dir`.
            if !stage_def.wiring.is_empty() {
                println!("  Re-verifying wiring from stage '{}'...", stage.id);
                let stage_working_dir = if stage_def.working_dir == "." {
                    verification_dir.to_path_buf()
                } else {
                    verification_dir.join(&stage_def.working_dir)
                };
                let gaps = crate::verify::goal_backward::verify_wiring(
                    &stage_def.wiring,
                    &stage_working_dir,
                )?;
                if !gaps.is_empty() {
                    for gap in &gaps {
                        eprintln!("    ✗ {}: {}", stage.id, gap.description);
                    }
                }
                all_gaps.extend(gaps);
            }
        }
    }

    if all_gaps.is_empty() {
        println!("Aggregated wiring re-verification passed!");
    } else {
        eprintln!();
        eprintln!(
            "Aggregated wiring re-verification found {} issue(s)",
            all_gaps.len()
        );
        eprintln!("Fix wiring issues in the merged codebase before completing integration-verify.");
        anyhow::bail!("Aggregated wiring re-verification failed");
    }

    Ok(())
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
fn run_verification_phase(
    stage: &mut crate::models::stage::Stage,
    stage_id: &str,
    no_verify: bool,
    acceptance_dir: &Option<PathBuf>,
    session_id: Option<&str>,
    work_dir: &Path,
) -> Result<()> {
    if !no_verify {
        // Resolve the base branch once for all detection calls
        let base_branch = resolve_base_branch(work_dir);

        // Load stage definition once for verification checks
        let stage_def = load_stage_definition_from_plan(stage_id, work_dir)?;

        // Run goal-backward verification (artifacts, wiring, wiring_tests, dead_code)
        if let Some(ref stage_def) = stage_def {
            if stage_def.has_any_goal_checks() {
                println!("Running goal-backward verification...");
                let verification_dir = acceptance_dir.as_deref().unwrap_or(Path::new("."));

                // Use shared helper for verification
                let goal_result = crate::commands::verify::run_and_verify_stage_goal(
                    stage_id,
                    verification_dir,
                    work_dir,
                )?;

                if !goal_result.is_passed() {
                    // Print gaps
                    for gap in goal_result.gaps() {
                        eprintln!("  ✗ {:?}: {}", gap.gap_type, gap.description);
                        eprintln!("    → {}", gap.suggestion);
                    }

                    eprintln!();
                    eprintln!("Goal-backward verification FAILED for stage '{stage_id}'");
                    eprintln!("  Fix the issues and run 'loom stage complete {stage_id}' again");
                    anyhow::bail!("Goal-backward verification failed for stage '{stage_id}'");
                }
                println!("Goal-backward verification passed!");
            }
        }

        // Run after-stage verification (post-condition checks)
        if let Some(ref stage_def) = stage_def {
            if !stage_def.after_stage.is_empty() {
                println!("Running after-stage verification...");
                let verification_dir = acceptance_dir.as_deref().unwrap_or(Path::new("."));
                let after_gaps = crate::verify::before_after::run_after_stage_checks(
                    &stage_def.after_stage,
                    verification_dir,
                )?;

                if !after_gaps.is_empty() {
                    for gap in &after_gaps {
                        eprintln!("  ✗ After-stage: {}", gap.description);
                        eprintln!("    → {}", gap.suggestion);
                    }

                    eprintln!();
                    eprintln!("After-stage verification FAILED for stage '{stage_id}'");
                    eprintln!("  Fix the issues and run 'loom stage complete {stage_id}' again");
                    anyhow::bail!("After-stage verification failed for stage '{stage_id}'");
                }
                println!("After-stage verification passed!");
            }
        }

        // Unwired file detection (2a + 2b)
        if let Some(ref verification_dir) = *acceptance_dir {
            match detect_unwired_files(verification_dir, &base_branch) {
                Ok(result) => {
                    if !result.unwired_files.is_empty() {
                        // Check if this stage has downstream dependents
                        let has_dependents = has_downstream_dependents(stage_id, work_dir);

                        if has_dependents {
                            // Warning + memory check (2b)
                            eprintln!(
                                "Warning: {} potentially unwired file(s):",
                                result.unwired_files.len()
                            );
                            for uf in &result.unwired_files {
                                eprintln!(
                                    "  - {} (importable as '{}')",
                                    uf.path, uf.importable_name
                                );
                            }

                            // Check if memory entries mention the unwired files
                            let has_memory_coverage = check_memory_covers_unwired(
                                stage_id,
                                &result.unwired_files,
                                work_dir,
                            );

                            if !has_memory_coverage {
                                eprintln!();
                                eprintln!(
                                    "New files not yet wired and no memory notes explain downstream wiring."
                                );
                                eprintln!("Record memory notes explaining what needs to happen:");
                                for uf in &result.unwired_files {
                                    eprintln!(
                                        "  loom memory note \"{} needs to be wired in by downstream stage\"",
                                        uf.path
                                    );
                                }
                                anyhow::bail!(
                                    "Unwired files detected with no memory notes for downstream wiring. \
                                     Record memory notes explaining the wiring plan."
                                );
                            }
                            println!("Unwired files found but memory notes cover downstream wiring plan.");
                        } else {
                            // No downstream dependents = leaf stage = ERROR (blocking)
                            eprintln!(
                                "ERROR: Unwired files in leaf stage (no downstream dependents):"
                            );
                            for uf in &result.unwired_files {
                                eprintln!(
                                    "  - {} (importable as '{}')",
                                    uf.path, uf.importable_name
                                );
                            }
                            eprintln!();
                            eprintln!(
                                "Leaf stages must wire all new files. Import/register them or remove them."
                            );
                            anyhow::bail!(
                                "Unwired files detected in leaf stage '{}'. Wire them or remove them.",
                                stage_id
                            );
                        }
                    }
                }
                Err(e) => {
                    // Non-fatal - detection is best-effort
                    eprintln!("Warning: Wiring detection skipped: {e}");
                }
            }
        }

        // Duplicate symbol detection (2c - advisory only)
        if let Some(ref verification_dir) = *acceptance_dir {
            match detect_duplicate_symbols(verification_dir, &base_branch) {
                Ok(duplicates) => {
                    if !duplicates.is_empty() {
                        println!("Potential duplicate symbols detected:");
                        for dup in &duplicates {
                            println!(
                                "  Warning: New {} '{}' in {}:{} may duplicate existing '{}' in {}:{}",
                                dup.symbol_type,
                                dup.symbol_name,
                                dup.new_file,
                                dup.new_line,
                                dup.symbol_name,
                                dup.existing_file,
                                dup.existing_line
                            );
                        }
                        println!("  (These are advisory warnings - verify they are intentional)");
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Duplicate detection skipped: {e}");
                }
            }
        }

        // Aggregated wiring re-verification for integration-verify stages (3d)
        if stage.stage_type == StageType::IntegrationVerify {
            if let Some(ref verification_dir) = *acceptance_dir {
                println!("Running aggregated wiring re-verification...");
                run_aggregated_wiring_reverification(stage_id, verification_dir, work_dir)?;
            }
        }

        // Run change impact comparison if configured
        if let Some(change_impact_config) = load_change_impact_config(work_dir)? {
            if change_impact_config.policy != ChangeImpactPolicy::Skip {
                println!("Running change impact comparison...");
                let comparison_dir = acceptance_dir.as_deref();

                match compare_to_baseline(stage_id, &change_impact_config, comparison_dir, work_dir)
                {
                    Ok(impact) => {
                        if !impact.comparison_succeeded {
                            eprintln!(
                                "Warning: Change impact comparison failed to run, continuing anyway"
                            );
                        } else {
                            // Print summary
                            println!("  {}", impact.summary());

                            // Print details if there are new failures
                            if impact.has_new_failures() {
                                println!("  New failures detected:");
                                for failure in &impact.new_failures {
                                    println!("    - {}", failure);
                                }
                            }

                            if !impact.fixed_failures.is_empty() {
                                println!("  Fixed failures:");
                                for fixed in &impact.fixed_failures {
                                    println!("    + {}", fixed);
                                }
                            }

                            // Check policy and fail if necessary
                            if impact.has_new_failures()
                                && change_impact_config.policy == ChangeImpactPolicy::Fail
                            {
                                eprintln!("Change impact check FAILED for stage '{stage_id}' - new failures introduced");
                                eprintln!("  Fix the issues and run 'loom stage complete {stage_id}' again");
                                anyhow::bail!("Change impact check failed for stage '{stage_id}' - new failures introduced");
                            }

                            if impact.has_new_failures()
                                && change_impact_config.policy == ChangeImpactPolicy::Warn
                            {
                                eprintln!("⚠️  Warning: New failures introduced, but continuing due to 'warn' policy");
                            }
                        }
                    }
                    Err(e) => {
                        // No baseline exists or comparison failed - just warn and continue
                        eprintln!("Warning: Change impact comparison skipped: {e}");
                    }
                }
            }
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
        let target_branch = crate::git::branch::resolve_target_branch(
            &Some(resolve_base_branch(work_dir)),
            &repo_root,
        );
        // Skip the guard if the branch doesn't exist on the host — that's the
        // shape unit tests (no real git repo) and isolated-git stages
        // (commits live in the container's mirror, not on host) take. The
        // phantom-merge class of bug requires an EXISTING empty branch:
        // attempt_auto_merge happily fast-forwards to itself.
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
                         agent commits real work, run `loom stage retry --kill-session \
                         {stage_id}`, or use `loom stage complete --force-unsafe` if \
                         you genuinely intend to mark an empty stage complete."
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

        // The orchestrator daemon will auto-merge and trigger dependents
        stage.try_complete(None)?;
        save_stage(stage, work_dir)?;
        println!("Stage '{stage_id}' completed (skipped verification).");
        println!("The orchestrator will handle merge and dependent triggering.");
    }

    Ok(())
}
