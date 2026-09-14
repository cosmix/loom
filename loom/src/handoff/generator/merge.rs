//! Locked semantic upsert for exact-session handoffs.

use std::path::{Path, PathBuf};

use anyhow::{ensure, Result};

use crate::fs::locking::locked_dir_update;
use crate::handoff::git_handoff::{CommitInfo, GitHistory};
use crate::handoff::schema::{HandoffOrigin, HandoffV2};
use crate::models::session::Session;
use crate::models::stage::Stage;

use super::lookup::{fold_checkpoints, session_handoffs};
use super::{generate_handoff_locked, HandoffContent};

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
        let merged = fold_handoffs(&prior, incoming)?;
        merged.to_v2().validate()?;
        if let Some((path, latest)) = prior.last() {
            if semantic_eq(latest, &merged, origin) {
                return Ok((path.clone(), MergeOutcome::Unchanged));
            }
        }
        let path = generate_handoff_locked(stage, &merged, work_dir)?;
        Ok((path, MergeOutcome::Created))
    })
}

fn fold_handoffs(
    prior: &[(PathBuf, HandoffV2)],
    mut incoming: HandoffContent,
) -> Result<HandoffContent> {
    fold_collections(prior, &mut incoming);
    fold_navigation(prior, &mut incoming);
    fold_git(prior, &mut incoming);
    let checkpoint = incoming.completion_checkpoint.take();
    incoming.completion_checkpoint = fold_checkpoints(
        prior
            .iter()
            .filter_map(|(_, handoff)| handoff.completion_checkpoint.as_ref())
            .chain(checkpoint.as_ref()),
    )?;
    Ok(incoming)
}

fn fold_collections(prior: &[(PathBuf, HandoffV2)], incoming: &mut HandoffContent) {
    let mut work = Vec::new();
    let mut decisions = Vec::new();
    let mut files = Vec::new();
    for (_, handoff) in prior {
        stable_extend(
            &mut work,
            handoff
                .completed_tasks
                .iter()
                .map(|v| v.description.clone()),
        );
        stable_extend(
            &mut decisions,
            handoff
                .key_decisions
                .iter()
                .map(|v| (v.decision.clone(), v.rationale.clone())),
        );
        stable_extend(&mut files, handoff.files_modified.iter().cloned());
    }
    stable_extend(&mut work, std::mem::take(&mut incoming.completed_work));
    stable_extend(&mut decisions, std::mem::take(&mut incoming.decisions));
    stable_extend(&mut files, std::mem::take(&mut incoming.files_modified));
    incoming.completed_work = work;
    incoming.decisions = decisions;
    incoming.files_modified = files;
}

fn fold_navigation(prior: &[(PathBuf, HandoffV2)], incoming: &mut HandoffContent) {
    if incoming.next_steps.is_empty() {
        incoming.next_steps = prior
            .iter()
            .rev()
            .find(|(_, handoff)| !handoff.next_actions.is_empty())
            .map(|(_, handoff)| handoff.next_actions.clone())
            .unwrap_or_default();
    }
    if incoming
        .current_branch
        .as_deref()
        .is_none_or(|branch| branch == "unknown")
    {
        incoming.current_branch = prior
            .iter()
            .rev()
            .find_map(|(_, handoff)| handoff.branch.clone().filter(|branch| branch != "unknown"));
    }
}

fn fold_git(prior: &[(PathBuf, HandoffV2)], incoming: &mut HandoffContent) {
    let mut commits = Vec::new();
    for (_, handoff) in prior {
        extend_commits(
            &mut commits,
            handoff.commits.iter().map(|commit| CommitInfo {
                hash: commit.hash.clone(),
                message: commit.message.clone(),
            }),
        );
    }
    let incoming_history = incoming.git_history.take();
    if let Some(history) = &incoming_history {
        extend_commits(&mut commits, history.commits.iter().cloned());
    }
    let prior_uncommitted = newest_uncommitted(prior);
    if commits.is_empty() && prior_uncommitted.is_empty() && incoming_history.is_none() {
        return;
    }
    let branch = incoming
        .current_branch
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    incoming.git_history = Some(build_history(
        incoming_history,
        branch,
        commits,
        prior_uncommitted,
    ));
}

fn build_history(
    incoming: Option<GitHistory>,
    branch: String,
    commits: Vec<CommitInfo>,
    prior_uncommitted: Vec<String>,
) -> GitHistory {
    match incoming {
        Some(mut history) => {
            history.branch = branch;
            history.commits = commits;
            history
        }
        None => GitHistory {
            branch,
            base_branch: String::new(),
            commits,
            uncommitted_changes: prior_uncommitted,
        },
    }
}

fn newest_uncommitted(prior: &[(PathBuf, HandoffV2)]) -> Vec<String> {
    prior
        .iter()
        .rev()
        .find(|(_, handoff)| !handoff.uncommitted_files.is_empty())
        .map(|(_, handoff)| handoff.uncommitted_files.clone())
        .unwrap_or_default()
}

fn semantic_eq(latest: &HandoffV2, merged: &HandoffContent, origin: Option<HandoffOrigin>) -> bool {
    let candidate = merged.to_v2();
    latest.completed_tasks == candidate.completed_tasks
        && latest.key_decisions == candidate.key_decisions
        && latest.next_actions == candidate.next_actions
        && latest.commits == candidate.commits
        && latest.uncommitted_files == candidate.uncommitted_files
        && latest.files_modified == candidate.files_modified
        && latest.completion_checkpoint == candidate.completion_checkpoint
        && (origin.is_none() || latest.origin == origin)
        && (origin != Some(HandoffOrigin::RedBand)
            || latest.context_tokens == candidate.context_tokens)
}

fn stable_extend<T: PartialEq>(target: &mut Vec<T>, values: impl IntoIterator<Item = T>) {
    for value in values {
        if !target.contains(&value) {
            target.push(value);
        }
    }
}

fn extend_commits(target: &mut Vec<CommitInfo>, values: impl IntoIterator<Item = CommitInfo>) {
    for value in values {
        if !target.iter().any(|commit| commit.hash == value.hash) {
            target.push(value);
        }
    }
}
