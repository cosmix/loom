use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::context::untrusted::inline_safe;
use crate::fs::session_files::load_session_exact;
use crate::models::session::{Session, SessionType};
use crate::models::stage::Stage;
use crate::process::is_process_alive;

use super::helpers;
use super::types::MergeSignalContent;

/// Generate a signal file for a merge conflict resolution session.
///
/// The signal directs the session to merge the target branch into the stage
/// branch inside the stage worktree, resolve the conflicts there, rerun the
/// stage's acceptance criteria, and report with `loom stage merge --resolved`.
/// It never works in the main checkout.
pub fn generate_merge_signal(
    session: &Session,
    stage: &Stage,
    source_branch: &str,
    target_branch: &str,
    conflicting_files: &[String],
    work_dir: &Path,
) -> Result<PathBuf> {
    let content = format_merge_signal_content(
        session,
        stage,
        source_branch,
        target_branch,
        conflicting_files,
    );
    helpers::write_signal_file(&session.id, &content, work_dir)
}

/// Find a live merge resolver session for the given stage by scanning
/// `.loom/work/signals/` for merge signals.
///
/// For each signal of `stage_id`, loads the session record it names and
/// checks PID liveness. If alive -> returns `Some(session_id)`. If dead (or
/// the record is missing) -> removes the stale signal file and continues
/// scanning. A signal outlives its record in ordinary operation: a failed
/// resolver spawn in `spawn_merge_resolver` leaves its signal with no record,
/// and orphan recovery and `loom sessions kill` remove a record before its
/// signal. A record that exists but cannot be read
/// leaves liveness unknown, so its error is returned.
///
/// A signal that cannot be read is attributed through its filename, which is
/// its session's id: a session record naming another stage, or a session that
/// is not a merge resolver, puts it out of this stage's concern; a merge
/// session of this stage is judged alive or dead like any other. With no
/// readable record, the signal could belong to a live resolver of this stage,
/// so the read error is returned.
///
/// Returns `Ok(None)` if no live merge session exists for the stage.
pub fn find_live_merge_session_for_stage(
    stage_id: &str,
    work_dir: &Path,
) -> Result<Option<String>> {
    let signal_ids = super::crud::list_signals(work_dir)?;
    for signal_id in &signal_ids {
        let (session_id, record) = match read_merge_signal(signal_id, work_dir) {
            Ok(Some(signal)) if signal.stage_id == stage_id => {
                let id = signal.session_id;
                let record = load_session_exact(work_dir, &id).with_context(|| {
                    format!("the session record of merge resolver {id} cannot be read")
                })?;
                (id, record)
            }
            Ok(_) => continue,
            Err(error) => {
                match attribute_unreadable_signal(signal_id, stage_id, work_dir, error)? {
                    Some(record) => (signal_id.clone(), Some(record)),
                    None => continue,
                }
            }
        };

        let alive = record
            .and_then(|session| session.pid)
            .is_some_and(is_process_alive);
        if alive {
            return Ok(Some(session_id));
        }
        // Dead session: clean up the stale signal and keep scanning.
        if let Err(e) = super::crud::remove_signal(signal_id, work_dir) {
            tracing::warn!(
                signal_id = %signal_id,
                error = %e,
                "Failed to remove stale merge signal"
            );
        }
    }
    Ok(None)
}

/// The session record of the unreadable signal `signal_id` when that record
/// is a merge session of `stage_id` (or names no stage), `None` when it
/// belongs elsewhere, and `error` when no readable record attributes the
/// signal at all.
fn attribute_unreadable_signal(
    signal_id: &str,
    stage_id: &str,
    work_dir: &Path,
    error: anyhow::Error,
) -> Result<Option<Session>> {
    let Some(record) = load_session_exact(work_dir, signal_id).ok().flatten() else {
        return Err(error.context(format!(
            "merge signal {signal_id} cannot be read, and no session record attributes it to a \
             stage"
        )));
    };
    let ours = record.session_type == SessionType::Merge
        && record
            .stage_id
            .as_deref()
            .is_none_or(|owner| owner == stage_id);
    Ok(ours.then_some(record))
}

/// Read and parse a merge signal file.
///
/// Returns `None` if the signal file doesn't exist or isn't a merge signal.
pub fn read_merge_signal(session_id: &str, work_dir: &Path) -> Result<Option<MergeSignalContent>> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));

    if !signal_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&signal_path).context("Failed to read signal file")?;

    // Check if this is a merge signal by looking for the merge-specific header
    if !content.contains("# Merge Signal:") {
        return Ok(None);
    }

    let parsed = parse_merge_signal_content(session_id, &content)?;
    Ok(Some(parsed))
}

pub(super) fn format_merge_signal_content(
    session: &Session,
    stage: &Stage,
    source_branch: &str,
    target_branch: &str,
    conflicting_files: &[String],
) -> String {
    let mut content = format!("# Merge Signal: {}\n\n", session.id);
    content.push_str(&format_merge_context(stage, source_branch, target_branch));
    content.push_str(&helpers::format_execution_rules_section("BOTH branches"));
    content.push_str(&helpers::format_target_section(
        &session.id,
        &stage.id,
        Some(source_branch),
        target_branch,
    ));
    content.push_str(&helpers::format_stage_context_section(stage));
    content.push_str(&helpers::format_conflicting_files_section(
        conflicting_files,
    ));
    content.push_str(&format_failure_section(
        stage,
        target_branch,
        conflicting_files,
    ));
    content.push_str(&format_merge_task(stage, target_branch));
    content.push_str(&format_acceptance_section(stage));
    content.push_str(&format_merge_important());
    content.push_str(&format_inherited_responsibilities(
        &stage.id,
        source_branch,
        target_branch,
    ));
    content
}

fn format_merge_context(stage: &Stage, source_branch: &str, target_branch: &str) -> String {
    format!(
        "## Merge Context\n\n\
         You are resolving a **merge conflict** between `{source_branch}` and `{target_branch}`.\n\n\
         - You work in the stage worktree `.worktrees/{}` (your current directory), on branch \
         `{source_branch}`\n\
         - You merge `{target_branch}` INTO the stage branch here\n\
         - The main checkout belongs to the operator: never run git or edit files there\n\
         - Loom lands the merge on `{target_branch}` itself after `--resolved`\n\n",
        stage.id
    )
}

fn format_merge_task(stage: &Stage, target_branch: &str) -> String {
    let criteria = if stage.acceptance.is_empty() {
        "the stage's acceptance criteria"
    } else {
        "the stage's acceptance criteria (listed below)"
    };
    let commit_rule = helpers::commit_command_rule("<specific-files>");
    format!(
        "## Your Task\n\n\
         1. If `git status` shows a merge already in progress in this worktree, continue it; \
         do not start a new `git merge`. Otherwise merge the target WITHOUT committing: \
         `git merge --no-commit --no-ff {target_branch}` (a bare `git merge {target_branch}` \
         commits by itself and cannot be signed in this sandbox) (stage the resolution; never \
         run `git merge --continue`, which commits)\n\
         2. Resolve the conflicts in the files listed above, preserving intent from both sides\n\
         3. Rerun {criteria} in this worktree and fix what the merge broke\n\
         4. Finish the merge: {commit_rule} The worktree must end clean with no merge in \
         progress\n\
         5. Run: `loom stage merge {} --resolved`\n\n",
        stage.id
    )
}

/// How many evidence lines of the last failed merge attempt the signal quotes.
const MAX_FAILURE_LINES: usize = 5;

/// Why the last merge attempt failed, for a stage whose merge was predicted
/// to conflict nowhere: the resolver would otherwise learn nothing. Empty when
/// conflicting files are known or the stage records no failure. The evidence is
/// git or process output, so each line goes through `inline_safe`.
fn format_failure_section(
    stage: &Stage,
    target_branch: &str,
    conflicting_files: &[String],
) -> String {
    let Some(info) = stage
        .failure_info
        .as_ref()
        .filter(|_| conflicting_files.is_empty())
    else {
        return String::new();
    };
    let mut content = String::from("## Why the Last Merge Attempt Failed\n\n");
    content.push_str(
        "No conflict was predicted, so no conflicting files are listed. The last attempt \
         failed with:\n\n",
    );
    for line in info.evidence.iter().take(MAX_FAILURE_LINES) {
        content.push_str(&format!("- {}\n", inline_safe(line)));
    }
    content.push_str(&format!(
        "\nStill merge the target branch into this worktree (`git merge --no-commit --no-ff \
         {target_branch}`), rerun the acceptance criteria, commit with `loom stage commit`, \
         and run `--resolved`; loom then retries the merge.\n\n"
    ));
    content
}

/// The stage's acceptance criteria; empty when the stage has none.
fn format_acceptance_section(stage: &Stage) -> String {
    if stage.acceptance.is_empty() {
        return String::new();
    }
    let mut content = String::from("## Acceptance Criteria\n\n");
    for criterion in &stage.acceptance {
        content.push_str(&format!("- [ ] {criterion}\n"));
    }
    content.push('\n');
    content
}

fn format_merge_important() -> String {
    "## Important\n\n\
     - Do NOT change code beyond what the conflicts and the acceptance criteria require\n\
     - Preserve intent from BOTH branches where possible\n\
     - If unclear how to resolve, ask the user for guidance\n\
     - Never touch the main checkout\n\
     - Do not rebase, reset, squash, amend or force-push: the stage's existing commits must \
     stay in the branch history, so merge only. Loom refuses a worktree that lost them\n\
     - Do not run `loom worktree remove`: the orchestrator removes the worktree after this \
     session exits and the merge has landed\n\n"
        .to_string()
}

fn format_inherited_responsibilities(
    stage_id: &str,
    source_branch: &str,
    target_branch: &str,
) -> String {
    format!(
        "## Inherited Responsibilities\n\n\
         This resolution session now **owns** this stage. The original execution session has \
         exited.\n\n\
         - `loom stage merge {stage_id} --resolved` makes loom re-run the merge, which lands \
         `{source_branch}` on `{target_branch}`\n\
         - If `{target_branch}` moved meanwhile and the merge conflicts again, loom keeps the \
         stage in conflict and another resolver round starts\n\
         - If this session exits without resolving, loom spawns a new resolver, up to the \
         attempt cap\n\n"
    )
}

pub(super) fn parse_merge_signal_content(
    session_id: &str,
    content: &str,
) -> Result<MergeSignalContent> {
    let sections = helpers::parse_signal_sections(content);

    // Extract from "Target" section
    let target_lines = sections
        .get("Target")
        .map(|v| v.as_slice())
        .unwrap_or_default();
    let stage_id = helpers::extract_field_from_lines(target_lines, "Stage")
        .unwrap_or_default()
        .to_string();
    let source_branch = helpers::extract_field_from_lines(target_lines, "Source Branch")
        .unwrap_or_default()
        .to_string();
    let target_branch = helpers::extract_field_from_lines(target_lines, "Target Branch")
        .unwrap_or_default()
        .to_string();

    // Extract from "Conflicting Files" section
    let conflict_lines = sections
        .get("Conflicting Files")
        .map(|v| v.as_slice())
        .unwrap_or_default();
    let conflicting_files = helpers::extract_backtick_items(conflict_lines);

    if stage_id.is_empty() {
        bail!("Merge signal file is missing stage_id");
    }

    Ok(MergeSignalContent {
        session_id: session_id.to_string(),
        stage_id,
        source_branch,
        target_branch,
        conflicting_files,
    })
}

#[cfg(test)]
#[path = "merge_liveness_tests.rs"]
mod liveness_tests;
