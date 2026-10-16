use super::*;

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
    },
    /// Active main-repo merge attributed to this stage but the stage's status
    /// is not yet `MergeConflict | MergeBlocked`. Caller MUST persist
    /// `Completed → MergeConflict + merged=false + merge_conflict=true`
    /// before invoking `spawn_merge_resolver`.
    RevertAndSpawnResolver {
        conflicting_files: Vec<String>,
        target_branch: String,
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

    if let Some(route) = worktree_merge_route(stage, repo_root) {
        return Ok(route);
    }
    let attribution = attribute_main_repo_merge(repo_root, work_dir, all_stages, sessions)?;
    if let Some(route) = foreign_merge_route(stage, &attribution) {
        return Ok(route);
    }
    let attributed_to_this_stage = matches!(
        &attribution, MergeAttribution::Attributed { stage_id, .. } if stage_id == &stage.id
    );
    if force_unsafe {
        return force_route(
            stage,
            assume_merged,
            attributed_to_this_stage,
            repo_root,
            work_dir,
        );
    }
    status_route(stage, &attribution, daemon_running, repo_root, work_dir)
}

fn worktree_merge_route(stage: &Stage, repo_root: &Path) -> Option<CompleteConflictRoute> {
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
            return Some(CompleteConflictRoute::Refuse {
                message: format!(
                    "Worktree `{location}` has an active merge. Run `cd {location} && \
                     git merge --abort` (or commit) before completing the stage."
                ),
            });
        }
    }

    None
}

fn foreign_merge_route(
    stage: &Stage,
    attribution: &MergeAttribution,
) -> Option<CompleteConflictRoute> {
    if let MergeAttribution::Attributed {
        stage_id: attributed_id,
        source,
        ..
    } = attribution
    {
        if attributed_id != &stage.id {
            return Some(CompleteConflictRoute::Refuse {
                message: format!(
                    "An active merge in the main repo is attributed to stage '{}' \
                     (via {:?}); refusing to complete stage '{}'. Resolve that merge \
                     first.",
                    attributed_id, source, stage.id
                ),
            });
        }
    }
    if let MergeAttribution::GlobalUnattributed(merge) = attribution {
        let location = match &merge.location {
            MergeLocation::MainRepo { repo_path, .. } => repo_path.display().to_string(),
            MergeLocation::Worktree { worktree_path, .. } => worktree_path.display().to_string(),
        };
        return Some(CompleteConflictRoute::Refuse {
            message: format!(
                "Active merge at `{location}` cannot be attributed to any known stage \
                 (or is a base-branch merge). Resolve or abort it manually before \
                 completing any stage."
            ),
        });
    }

    None
}

fn force_route(
    stage: &Stage,
    assume_merged: bool,
    attributed_to_this_stage: bool,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<CompleteConflictRoute> {
    if assume_merged {
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
    Ok(CompleteConflictRoute::ForceUnsafeAllowedStaleFlag)
}

fn status_route(
    stage: &Stage,
    attribution: &MergeAttribution,
    daemon_running: bool,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<CompleteConflictRoute> {
    let merge = match attribution {
        MergeAttribution::Attributed { merge, .. } => Some(merge),
        _ => None,
    };
    let conflict_status = matches!(
        stage.status,
        StageStatus::MergeConflict | StageStatus::MergeBlocked
    );
    if !conflict_status && merge.is_none() {
        return Ok(CompleteConflictRoute::Proceed);
    }
    if daemon_running {
        return Ok(CompleteConflictRoute::DaemonManaged {
            stage_id: stage.id.clone(),
        });
    }
    let target_branch = crate::fs::resolve_target_branch_from_config(work_dir, repo_root)?;
    let conflicting_files = match merge.map(|m| &m.state) {
        Some(ActiveMergeState::HasUnmergedPaths(paths)) => paths.clone(),
        _ => Vec::new(),
    };
    if conflict_status {
        Ok(CompleteConflictRoute::SpawnResolver {
            conflicting_files,
            target_branch,
        })
    } else {
        Ok(CompleteConflictRoute::RevertAndSpawnResolver {
            conflicting_files,
            target_branch,
        })
    }
}

/// Shared "the daemon owns merge resolution; the stage is not yet completed"
/// notice. `complete()`'s direct `CompleteConflictRoute::DaemonManaged` route
/// and `spawn_resolver_for_route`'s `MergeResolverResult::DaemonManaged` arm
/// both land here — different callers reaching the same state, an active
/// merge already redirected to the daemon — so both print exactly this.
pub(super) fn print_daemon_managed_notice(stage_id: &str) {
    println!(
        "Daemon is handling merge resolution for stage '{stage_id}'. The stage is \
         NOT completed yet — do not treat this as completion. Run `loom status` to \
         monitor; completion applies once the daemon finishes resolving the merge."
    );
}

/// Spawn a CLI-side merge resolver for a route that already satisfies the
/// `MergeConflict | MergeBlocked` status contract on disk.
pub(super) fn spawn_resolver_for_route(
    stage: &Stage,
    conflicting_files: &[String],
    target_branch: &str,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<()> {
    // None of the three arms below complete the stage — they only report on
    // resolver status and return Ok(()). Each message says so explicitly so a
    // session (or agent) reading exit 0 here does not mistake it for
    // completion.
    match spawn_merge_resolver(stage, conflicting_files, target_branch, repo_root, work_dir)? {
        MergeResolverResult::DaemonManaged => {
            print_daemon_managed_notice(&stage.id);
        }
        MergeResolverResult::Spawned(id) => {
            println!(
                "Spawned merge resolver session: {id}. Stage '{}' is NOT completed yet \
                 — completion happens once the resolver session finishes and the merge \
                 lands. Do not treat this as completion.",
                stage.id
            );
        }
        MergeResolverResult::AlreadyRunning { session_id } => {
            println!(
                "A merge resolver session is already running for stage '{}': {session_id}. \
                 Wait for it to complete, or run `loom sessions kill {session_id}` to abort. \
                 The stage is NOT completed yet — do not treat this as completion.",
                stage.id
            );
        }
    }
    Ok(())
}
