//! Locked semantic upsert for exact-session handoffs.

use std::path::{Path, PathBuf};

use anyhow::{ensure, Result};

use crate::fs::locking::locked_dir_update;
use crate::handoff::completion::attestation_key;
use crate::handoff::git_handoff::GitHistory;
use crate::handoff::schema::{
    CommitRef, CompletedTask, CompletionCheckpoint, HandoffOrigin, HandoffV2,
};
use crate::models::session::Session;
use crate::models::stage::Stage;

use super::lookup::{fold_authenticated, fold_checkpoints, session_handoffs};
use super::{generate_handoff_document_locked, HandoffContent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeOutcome {
    Created,
    Unchanged,
}

pub fn merge_session_handoff(
    session: &Session,
    stage: &Stage,
    origin: Option<HandoffOrigin>,
    incoming: HandoffContent,
    work_dir: &Path,
) -> Result<(PathBuf, MergeOutcome)> {
    ensure!(
        incoming.session_id == session.id,
        "handoff session mismatch"
    );
    ensure!(incoming.stage_id == stage.id, "handoff stage mismatch");
    ensure!(incoming.origin == origin, "handoff origin mismatch");

    let handoffs_dir = work_dir.join("handoffs");
    locked_dir_update(&handoffs_dir, || {
        let prior = session_handoffs(&stage.id, &session.id, work_dir)?;
        let merged = fold_handoffs(&prior, &incoming, work_dir)?;
        merged.validate()?;
        if let Some((path, latest)) = prior.last() {
            if latest == &merged {
                return Ok((path.clone(), MergeOutcome::Unchanged));
            }
        }
        let content = HandoffContent::from_v2(merged.clone());
        let path = generate_handoff_document_locked(stage, &content, &merged, work_dir)?;
        Ok((path, MergeOutcome::Created))
    })
}

fn fold_handoffs(
    prior: &[(PathBuf, HandoffV2)],
    incoming: &HandoffContent,
    work_dir: &Path,
) -> Result<HandoffV2> {
    let mut merged = incoming.to_v2();
    for (_, handoff) in prior.iter().rev() {
        merge_accumulating(handoff, &mut merged);
    }
    merge_navigation(prior, &mut merged);
    merge_commits(prior, &mut merged);
    merge_snapshots(prior, incoming, &mut merged);
    let incoming_checkpoint = merged.completion_checkpoint.take();
    merged.completion_checkpoint = fold_completion(prior, incoming_checkpoint.as_ref(), work_dir)?;
    Ok(merged)
}

/// New evidence exists in no other artifact, so a lossy merge with unsigned
/// state would destroy it: only signed components are folded with it. A handoff
/// bringing none leaves every signed record in place in the prior artifacts and
/// signs nothing. It needs no key, since session hooks write handoffs where the
/// sandbox denies that file, and the trusted loader authenticates what it carries.
fn fold_completion(
    prior: &[(PathBuf, HandoffV2)],
    incoming: Option<&CompletionCheckpoint>,
    work_dir: &Path,
) -> Result<Option<CompletionCheckpoint>> {
    let prior = prior
        .iter()
        .filter_map(|(_, handoff)| handoff.completion_checkpoint.as_ref());
    match incoming {
        Some(incoming) => {
            fold_authenticated(prior.chain(Some(incoming)), &attestation_key(work_dir)?)
        }
        None => fold_checkpoints(prior),
    }
}

fn merge_accumulating(prior: &HandoffV2, merged: &mut HandoffV2) {
    // Existing records lead so a checkpoint-only update retains their typed detail.
    prepend_tasks(&mut merged.completed_tasks, &prior.completed_tasks);
    prepend_unique(&mut merged.key_decisions, &prior.key_decisions);
    prepend_unique(&mut merged.discovered_facts, &prior.discovered_facts);
    prepend_unique(&mut merged.open_questions, &prior.open_questions);
    prepend_unique(&mut merged.files_read, &prior.files_read);
}

fn merge_navigation(prior: &[(PathBuf, HandoffV2)], merged: &mut HandoffV2) {
    let latest = prior.last().map(|(_, handoff)| handoff);
    let Some(latest) = latest else { return };
    if merged.next_actions.is_empty() {
        merged.next_actions = prior
            .iter()
            .rev()
            .find(|(_, handoff)| !handoff.next_actions.is_empty())
            .map_or_else(Vec::new, |(_, handoff)| handoff.next_actions.clone());
    }
    if merged
        .branch
        .as_deref()
        .is_none_or(|branch| branch == "unknown")
    {
        merged.branch = latest.branch.clone();
    }
    if merged.context_tokens == 0 {
        merged.context_tokens = latest.context_tokens;
    }
    if merged.base_branch.is_none() {
        merged.base_branch = latest.base_branch.clone();
    }
    if merged.origin.is_none() {
        merged.origin = latest.origin;
    }
    if merged.plan_id.is_none() {
        merged.plan_id = latest.plan_id.clone();
    }
    if merged.goals.is_empty() {
        merged.goals = latest.goals.clone();
    }
    if merged.test_status.is_none() {
        merged.test_status = latest.test_status.clone();
    }
    if merged.memory_content.is_none() {
        merged.memory_content = latest.memory_content.clone();
    }
}

fn merge_commits(prior: &[(PathBuf, HandoffV2)], merged: &mut HandoffV2) {
    let incoming = std::mem::take(&mut merged.commits);
    for (_, handoff) in prior {
        for commit in &handoff.commits {
            push_commit(&mut merged.commits, commit.clone());
        }
    }
    for commit in incoming {
        push_commit(&mut merged.commits, commit);
    }
}

/// The uncommitted-file lists are snapshots of the working tree. A producer
/// that looked replaces them, an empty list meaning the tree is clean now; one
/// that did not look says nothing about the tree and leaves the latest lists.
fn merge_snapshots(
    prior: &[(PathBuf, HandoffV2)],
    incoming: &HandoffContent,
    merged: &mut HandoffV2,
) {
    let history_observed = incoming
        .git_history
        .as_ref()
        .is_some_and(GitHistory::observed_worktree);
    if history_observed {
        return;
    }
    if incoming.files_observed {
        merged.uncommitted_files = merged.files_modified.clone();
    } else if let Some((_, latest)) = prior.last() {
        merged.uncommitted_files = latest.uncommitted_files.clone();
        merged.files_modified = latest.files_modified.clone();
    }
}

fn prepend_unique<T: Clone + PartialEq>(target: &mut Vec<T>, older: &[T]) {
    let mut combined = older.to_vec();
    for item in target.drain(..) {
        if !combined.contains(&item) {
            combined.push(item);
        }
    }
    *target = combined;
}

fn prepend_tasks(target: &mut Vec<CompletedTask>, older: &[CompletedTask]) {
    let mut combined = older.to_vec();
    for item in target.drain(..) {
        if let Some(existing) = combined
            .iter_mut()
            .find(|task| task.description == item.description)
        {
            for file in item.files {
                if !existing.files.contains(&file) {
                    existing.files.push(file);
                }
            }
        } else {
            combined.push(item);
        }
    }
    *target = combined;
}

fn push_commit(target: &mut Vec<CommitRef>, commit: CommitRef) {
    if !target.iter().any(|existing| existing.hash == commit.hash) {
        target.push(commit);
    }
}
