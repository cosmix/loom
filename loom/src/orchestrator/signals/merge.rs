use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::context::untrusted::inline_safe;
use crate::git::stage_commit::guarded_restore_command;
use crate::models::session::Session;
use crate::models::stage::Stage;

use super::helpers;
use super::types::MergeSignalContent;

/// Generate a signal file for a merge conflict resolution session.
///
/// The signal directs the session to merge the target branch into the stage
/// branch inside the stage worktree, resolve the conflicts there, rerun the
/// stage's acceptance criteria, and report with `loom stage merge --resolved`.
/// It never works in the main checkout. Where the plan protects paths with
/// `deny_write`, the signal ends with the rule for them.
pub fn generate_merge_signal(
    session: &Session,
    stage: &Stage,
    source_branch: &str,
    target_branch: &str,
    conflicting_files: &[String],
    work_dir: &Path,
) -> Result<PathBuf> {
    generate_merge_snapshot_signal(
        session,
        stage,
        source_branch,
        target_branch,
        conflicting_files,
        work_dir,
        target_branch,
    )
}

pub(crate) fn generate_merge_snapshot_signal(
    session: &Session,
    stage: &Stage,
    source_branch: &str,
    target_branch: &str,
    conflicting_files: &[String],
    work_dir: &Path,
    snapshot: &str,
) -> Result<PathBuf> {
    let content = format_merge_snapshot_content(
        session,
        stage,
        source_branch,
        target_branch,
        conflicting_files,
        &format_protected_paths(stage, work_dir),
        snapshot,
    );
    helpers::write_signal_file(&session.id, &content, work_dir)
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
    if !content
        .lines()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| line.starts_with("# Merge Signal:"))
    {
        return Ok(None);
    }

    let parsed = parse_merge_signal_content(session_id, &content)?;
    Ok(Some(parsed))
}

/// The whole signal. `protected_paths` is [`format_protected_paths`]'s section,
/// empty when the plan protects nothing; the Task cross-references it only
/// when it is there.
#[cfg(test)]
pub(super) fn format_merge_signal_content(
    session: &Session,
    stage: &Stage,
    source_branch: &str,
    target_branch: &str,
    conflicting_files: &[String],
    protected_paths: &str,
) -> String {
    format_merge_snapshot_content(
        session,
        stage,
        source_branch,
        target_branch,
        conflicting_files,
        protected_paths,
        target_branch,
    )
}

fn format_merge_snapshot_content(
    session: &Session,
    stage: &Stage,
    source_branch: &str,
    target_branch: &str,
    conflicting_files: &[String],
    protected_paths: &str,
    snapshot: &str,
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
    content.push_str(&format_failure_section(stage, snapshot, conflicting_files));
    content.push_str(&format_merge_task(
        stage,
        snapshot,
        !protected_paths.is_empty(),
    ));
    content.push_str(&format_acceptance_section(stage));
    content.push_str(&format_merge_important(&stage.id));
    content.push_str(&format_inherited_responsibilities(
        &stage.id,
        source_branch,
        target_branch,
    ));
    content.push_str(protected_paths);
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

fn format_merge_task(stage: &Stage, target_branch: &str, has_protected_paths: bool) -> String {
    let criteria = if stage.acceptance.is_empty() {
        "the stage's acceptance criteria"
    } else {
        "the stage's acceptance criteria (listed below)"
    };
    let commit_rule = helpers::commit_command_rule("<specific-files>");
    let protected_note = if has_protected_paths {
        "; a path under Protected Paths keeps exactly one side instead, as that section says"
    } else {
        ""
    };
    format!(
        "## Your Task\n\n\
         1. If `git status` shows a merge already in progress, continue it only when \
         `git rev-parse MERGE_HEAD` equals `{target_branch}`. Otherwise preserve your staged \
         changes and seek manual recovery. If no merge is in progress, merge the authorized \
         target snapshot WITHOUT committing: \
         `git merge --no-commit --no-ff {target_branch}` (a bare `git merge {target_branch}` \
         commits by itself and cannot be signed in this sandbox) (stage the resolution; never \
         run `git merge --continue`, which commits)\n\
         2. Resolve the conflicts in the files listed above, preserving intent from both \
         sides{protected_note}\n\
         3. Rerun {criteria} in this worktree and fix what the merge broke\n\
         4. Finish the merge: {commit_rule} The worktree must end clean with no merge in \
         progress\n\
         5. Run: `loom stage merge {} --resolved`. Loom reruns all completion gates against the \
         resolved commit and current policy before landing. A failed gate stops this session and \
         hands the stage to a stage session, so step 3 must pass before you commit; fix any \
         other refusal and retry.\n\n",
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
        content.push_str(&super::format::format_criterion(criterion));
    }
    content.push('\n');
    content
}

fn format_merge_important(stage_id: &str) -> String {
    format!(
        "## Important\n\n\
         - Do NOT change code beyond what the conflicts and the acceptance criteria require\n\
         - Preserve intent from BOTH branches where possible\n\
         - Nobody watches this terminal: never end your turn with an explanation or a question. \
         If you cannot finish the merge (a path is read-only, a command is denied, or the right \
         resolution is unclear), run `loom stage block {stage_id} \"<why the merge cannot be \
         finished>\"` instead. Loom then stops this session and hands the stage to the operator \
         with your reason\n\
         - Never touch the main checkout\n\
         - Do not rebase, reset, squash, amend or force-push: the stage's existing commits must \
         stay in the branch history, so merge only. Loom refuses a worktree that lost them\n\
         - Do not run `loom worktree remove`: the orchestrator removes the worktree after this \
         session exits and the merge has landed\n\n"
    )
}

/// The rule for the paths the plan's `deny_write` protects. A resolver's
/// sandbox is released from those denies so `git merge` can write them, and
/// `loom stage commit` checks them instead (`git::stage_commit`). Empty when
/// the stage has no worktree-relative deny entry, and when the plan snapshot
/// cannot be read: the commit check refuses on its own then.
fn format_protected_paths(stage: &Stage, work_dir: &Path) -> String {
    let globs = match crate::sandbox::worktree_write_denies(work_dir, stage) {
        Ok(globs) if !globs.is_empty() => globs,
        _ => return String::new(),
    };
    let listed: String = globs
        .iter()
        .map(|glob| format!("- `{}`\n", inline_safe(glob)))
        .collect();
    format!(
        "## Protected Paths\n\n\
         The plan forbids writes to these paths, and loom checks them when you commit the \
         merge:\n\n\
         {listed}\n\
         Content at a protected path must be exactly what git's merge produced. Do not edit it, \
         and do not take one side of a path git merged cleanly. For a path git could not merge, \
         keep exactly one side of it. A side without the path removes it.\n\n\
         A refused commit prints the exact command for each path it names, with the path already \
         quoted for the shell: run it as printed, and never retype the path. A name with a \
         control character, or a character that prints as something else (a bidirectional \
         override, a zero-width or non-ASCII space or format character), is quoted as `$'...'`, \
         which bash and zsh read, with each such character written as the `\\xHH` of its UTF-8 \
         bytes. A command has this shape, `<path>` standing for the quoted path: `{}` for the \
         stage branch's version, `{}` for the target's.\n\n",
        guarded_restore_command("HEAD"),
        guarded_restore_command("MERGE_HEAD"),
    )
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
