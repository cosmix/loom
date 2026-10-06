//! `loom stage commit`, relayed: the daemon commits a session's staged index.
//!
//! A session cannot sign, so none runs `git commit`. It stages its change,
//! runs its own `pre-commit` and `commit-msg` hooks, and relays the message
//! with the HEAD and the tree it saw. The daemon maps the session to the one
//! branch it may move and commits that tree with plumbing only
//! (`git::stage_commit`):
//!
//! - a `Stage` session commits its stage branch in its worktree, while the
//!   stage is `Executing` and the session owns it;
//! - a `Knowledge` session commits the target branch in the main checkout,
//!   under the knowledge prefix only. The merge lock is held across the
//!   commit and its target-guard attestation: loom's git runs no hooks, so the
//!   `reference-transaction` hook never attests a daemon move;
//! - a `Merge` session, which is not the stage's own session, commits the
//!   resolved merge on the stage branch while it resolves that stage's merge.
//!
//! Git in a stage worktree runs pinned to the worktree's registered git
//! directory, because the session can rewrite the worktree's `.git` file. A
//! signing failure leaves the ref unmoved and stops the work where only the
//! operator can fix it: a stage or knowledge commit blocks the stage, and a
//! merge commit holds the merge for review.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;

use crate::daemon::{handle_block_stage, Response};
use crate::fs::resolve_target_branch_from_config;
use crate::git::branch::{branch_name_for_stage, branch_ref};
use crate::git::merge::lock::MergeLock;
use crate::git::stage_commit::{CommitRefusal, CommitRequest, CommitScope, Committer};
use crate::git::target_guard::{append_attestation, guarded_refs, knowledge_prefix};
use crate::git::worktree::{stage_worktree_path, WorktreeGit};
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::relay::CommitPayload;
use crate::verify::transitions::load_stage;

use super::apply::require_owner;
use super::{InboxHost, Settle};

/// How long a knowledge commit waits for the merge lock.
const MERGE_LOCK_WAIT: Duration = Duration::from_secs(10);

/// Where a relayed commit is applied from.
pub(super) struct CommitSite<'a> {
    pub(super) work_dir: &'a Path,
    pub(super) repo_root: &'a Path,
    pub(super) stage_id: &'a str,
    pub(super) record: &'a Session,
}

/// What one commit runs under: git in the checkout it commits, the branch it
/// may move, for a merge commit the target its `MERGE_HEAD` must be on, and
/// for a knowledge commit the merge lock, held until the commit and its
/// attestation are done.
struct Prepared {
    git: WorktreeGit,
    scope: CommitScope,
    merge_target: Option<String>,
    _merge_lock: Option<MergeLock>,
}

/// Commit the staged tree `payload` names for the session at `site`. Every
/// outcome is a [`Settle`]; a refusal says why and leaves the branch unmoved.
pub(super) fn apply_commit(
    host: &mut dyn InboxHost,
    site: &CommitSite<'_>,
    payload: &CommitPayload,
) -> Settle {
    let prepared = match prepare(site) {
        Ok(prepared) => prepared,
        Err(reason) => return Settle::Refused(reason),
    };
    let request = CommitRequest {
        message: payload.message.clone(),
        expected_head: payload.expected_head.clone(),
        expected_tree: payload.expected_tree.clone(),
    };
    let mut committer = Committer::new(&prepared.git, site.repo_root);
    if let Some(target) = &prepared.merge_target {
        committer = committer.merging_into(target);
    }
    match committer.commit_staged(&prepared.scope, &request) {
        Ok(id) => {
            attest_knowledge_commit(site.work_dir, &prepared.scope, &payload.expected_head, &id);
            Settle::Applied(Some(format!("committed {id}")))
        }
        Err(CommitRefusal::Signing { detail }) => {
            signing_failed(host, site, &prepared.scope, &detail)
        }
        Err(CommitRefusal::Refused { reason }) => Settle::Refused(reason),
    }
}

/// The checkout, branch and lock the session's kind commits with, after the
/// checks that kind needs.
fn prepare(site: &CommitSite<'_>) -> Result<Prepared, String> {
    match site.record.session_type {
        SessionType::Stage => prepare_stage(site),
        SessionType::Knowledge => prepare_knowledge(site),
        SessionType::Merge => prepare_merge(site),
        other @ (SessionType::Contract | SessionType::Adjudication | SessionType::BaseConflict) => {
            Err(format!(
                "a {other} session may not relay a 'commit' request"
            ))
        }
    }
}

/// The owning session's commit to its stage branch, in its worktree.
fn prepare_stage(site: &CommitSite<'_>) -> Result<Prepared, String> {
    let stage = require_owner(site.work_dir, site.stage_id, site.record)?;
    require_executing(&stage)?;
    Ok(Prepared {
        git: pinned_worktree(site, &stage)?,
        scope: CommitScope::StageBranch {
            stage_id: site.stage_id.to_string(),
        },
        merge_target: None,
        _merge_lock: None,
    })
}

/// The owning knowledge session's commit to the target branch, in the main
/// checkout, under the merge lock.
fn prepare_knowledge(site: &CommitSite<'_>) -> Result<Prepared, String> {
    let stage = require_owner(site.work_dir, site.stage_id, site.record)?;
    require_executing(&stage)?;
    let target_branch = target_branch(site)?;
    let merge_lock = MergeLock::acquire(site.work_dir, MERGE_LOCK_WAIT).map_err(|error| {
        format!("the merge lock could not be taken ({error:#}); run the commit again")
    })?;
    Ok(Prepared {
        git: WorktreeGit::discovered(site.repo_root),
        scope: CommitScope::Knowledge {
            target_branch,
            prefix: PathBuf::from(knowledge_prefix().trim_end_matches('/')),
        },
        merge_target: None,
        _merge_lock: Some(merge_lock),
    })
}

/// The merge target, resolved from the configuration as the merge machinery
/// resolves it.
fn target_branch(site: &CommitSite<'_>) -> Result<String, String> {
    resolve_target_branch_from_config(site.work_dir, site.repo_root)
        .map_err(|error| format!("the target branch could not be read: {error:#}"))
}

/// A merge session's commit of the merge it resolves. The merge session is
/// not the stage's `session`, so it is attributed by the branch it was
/// spawned for, as `merge-resolved` is. Its `MERGE_HEAD` must be on the
/// target: the session can write that file, and the commit records it.
fn prepare_merge(site: &CommitSite<'_>) -> Result<Prepared, String> {
    let record = site.record;
    if record.status != SessionStatus::Running {
        return Err(format!("session '{}' is no longer running", record.id));
    }
    let branch = branch_name_for_stage(site.stage_id);
    if record.merge_source_branch.as_deref() != Some(branch.as_str()) {
        return Err(format!(
            "session '{}' is not resolving the merge of {branch}",
            record.id
        ));
    }
    let stage = load_stage(site.stage_id, site.work_dir)
        .map_err(|error| format!("stage '{}' could not be loaded: {error:#}", site.stage_id))?;
    if !matches!(
        stage.status,
        StageStatus::MergeConflict | StageStatus::MergeBlocked
    ) {
        return Err(format!(
            "stage '{}' is {}, not MergeConflict or MergeBlocked",
            site.stage_id, stage.status
        ));
    }
    Ok(Prepared {
        git: pinned_worktree(site, &stage)?,
        scope: CommitScope::Merge {
            stage_id: site.stage_id.to_string(),
        },
        merge_target: Some(target_branch(site)?),
        _merge_lock: None,
    })
}

fn require_executing(stage: &Stage) -> Result<(), String> {
    if stage.status == StageStatus::Executing {
        return Ok(());
    }
    Err(format!(
        "stage '{}' is {}, not Executing",
        stage.id, stage.status
    ))
}

/// Git pinned to the stage's worktree, the worktree id validated first.
fn pinned_worktree(site: &CommitSite<'_>, stage: &Stage) -> Result<WorktreeGit, String> {
    let path = stage_worktree_path(stage, site.repo_root).map_err(|error| format!("{error:#}"))?;
    WorktreeGit::pinned(site.repo_root, &path).map_err(|error| {
        format!(
            "{} is not a registered worktree of this repository: {error:#}",
            path.display()
        )
    })
}

/// A signature the daemon could not make. The ref is unmoved; only the
/// operator can fix signing, so the work stops where they will see it.
fn signing_failed(
    host: &mut dyn InboxHost,
    site: &CommitSite<'_>,
    scope: &CommitScope,
    detail: &str,
) -> Settle {
    if let CommitScope::Merge { .. } = scope {
        // No block: `MergeBlocked -> Blocked` is not a legal edge, a blocked
        // `MergeConflict` stage keeps its resolver alive, and `loom stage
        // retry` would re-run the whole stage rather than the merge.
        let outcome = host.hold_merge_for_signing(site.stage_id, detail);
        return Settle::Refused(format!("signing failed: {detail}; {outcome}"));
    }
    let reason = format!(
        "commit signing failed: {detail}; fix the signing setup (gpg-agent passphrase cache, \
         GUI pinentry or ssh-agent key), then run loom stage retry {}",
        site.stage_id
    );
    settle_signing_block(site.stage_id, detail, &reason, |reason| {
        handle_block_stage(site.work_dir, site.stage_id, reason)
    })
}

/// Settle a stage or knowledge signing failure by how blocking the stage with
/// `reason` went. `block` refuses a transition with `Ok(Response::Error)` as
/// well as failing with `Err`; only `Ok(Response::Ok)` is reported as a block.
pub(super) fn settle_signing_block(
    stage_id: &str,
    detail: &str,
    reason: &str,
    block: impl FnOnce(&str) -> Result<Response>,
) -> Settle {
    let why = match block(reason) {
        Ok(Response::Ok) => {
            return Settle::Refused(format!(
                "signing failed: {detail}; the stage is blocked for the operator"
            ));
        }
        Ok(Response::Error { message }) => message,
        Ok(other) => format!("unexpected daemon answer: {other:?}"),
        Err(error) => format!("{error:#}"),
    };
    tracing::warn!(
        stage_id = %stage_id,
        why = %why,
        "Could not block the stage after a commit signing failure"
    );
    Settle::Refused(format!(
        "signing failed: {detail}; the stage could not be blocked: {why}"
    ))
}

/// A knowledge commit moved the target branch from `from` to `to`: attest
/// the move when the target guard guards that ref. A failure leaves the
/// commit unattested, and the guard then holds it for the operator as it holds
/// any move it cannot attribute.
fn attest_knowledge_commit(work_dir: &Path, scope: &CommitScope, from: &str, to: &str) {
    let CommitScope::Knowledge { target_branch, .. } = scope else {
        return;
    };
    let reference = branch_ref(target_branch);
    let attested = guarded_refs(work_dir).and_then(|guarded| {
        if guarded.contains(&reference) {
            append_attestation(work_dir, &reference, from, to)
        } else {
            Ok(())
        }
    });
    if let Err(error) = attested {
        tracing::warn!(
            reference = %reference,
            error = %format!("{error:#}"),
            "Could not attest a knowledge commit; the target guard holds it for the operator"
        );
    }
}
