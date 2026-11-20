//! Adjudicator feedback file lifecycle.
//!
//! For each stage that has been disputed, a single
//! `.loom/work/disputes/<stage>/feedback.md` file holds the most recent
//! human-readable advice the adjudicator surfaced (rejection reasoning,
//! or follow-up evidence questions). The next signal generated for that
//! stage embeds the file's contents so the agent reads them on resume.
//!
//! The file is overwritten on every adjudicator pass — older feedback
//! is intentionally discarded, because the agent only needs the latest
//! verdict to make progress. Persistent history lives in the per-
//! dispute `verdict.md` records. The one append is the failure section of
//! the daemon's completion run after a verdict that held the stage
//! ([`append_completion_failures`]): it follows that verdict's notice, which
//! the fresh session still needs.

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::fs::safe_fs::safe_open_append;
use crate::models::dispute::{truncate_to_byte_limit, Citation, DisputeKind};
use crate::utils::text::single_line;

const FEEDBACK_FILENAME: &str = "feedback.md";

/// Failure lines the completion section lists before it summarises the rest.
const MAX_COMPLETION_FAILURES: usize = 20;
/// Byte cap of one failure line; job summaries can carry whole command output.
const MAX_FAILURE_LINE_BYTES: usize = 300;

/// Path to `.loom/work/disputes/<stage>/feedback.md`. The parent directory
/// is created on demand so the first dispute against a stage works
/// without a prior `mkdir`.
fn feedback_path(work_dir: &Path, stage_id: &str) -> PathBuf {
    work_dir
        .join("disputes")
        .join(stage_id)
        .join(FEEDBACK_FILENAME)
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    Ok(())
}

/// Write rejection feedback: the adjudicator concluded the disputed check (an
/// acceptance criterion, wiring check or wiring test) is correct and the
/// agent must fix the implementation. Overwrites any prior content.
pub fn append_rejection(
    work_dir: &Path,
    stage_id: &str,
    reasoning: &str,
    citations: &[Citation],
) -> Result<()> {
    let path = feedback_path(work_dir, stage_id);
    ensure_parent(&path)?;
    let mut body =
        String::from("The adjudicator rejected your dispute. The disputed check stands.\n\n");
    body.push_str("### Reasoning\n\n");
    body.push_str(reasoning.trim());
    body.push_str("\n\n");
    if !citations.is_empty() {
        body.push_str("### Citations\n\n");
        for c in citations {
            let line = c.line.map(|n| format!(":{n}")).unwrap_or_default();
            body.push_str(&format!("- `{}{}` — {}\n", c.file, line, c.claim));
            body.push_str(&format!("  > {}\n", c.excerpt.replace('\n', " ")));
        }
        body.push('\n');
    }
    body.push_str("Action: fix the implementation so the check passes. Do NOT re-dispute.\n");
    fs::write(&path, body).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// What the agent does with an evidence question, for every dispute kind but a
/// contract's.
pub const EVIDENCE_ACTION: &str =
    "gather the requested evidence, commit it, and file a follow-up dispute.";

/// The action line an evidence-loop notice ends with for a dispute of `kind`.
/// A contract dispute never asks the agent to edit the frozen file: that edit
/// is forbidden, and a replacement travels with the dispute instead.
pub fn evidence_action(stage_id: &str, kind: &DisputeKind) -> String {
    let DisputeKind::Contract { contract_ids, .. } = kind else {
        return EVIDENCE_ACTION.to_string();
    };
    let contracts: String = contract_ids
        .iter()
        .map(|id| format!(" --contract {id}"))
        .collect();
    format!(
        "answer these in your next dispute. Never edit a frozen file. If a contract must \
         change, re-file `loom stage dispute-contract {stage_id}{contracts} \
         --replacement <frozen-file>=<path> --reason \"...\"` with one whole replacement \
         per frozen file that changes; an earlier replacement is not carried over."
    )
}

/// Write evidence-loop feedback: the adjudicator wants the agent to
/// answer specific questions before re-disputing, then do `action`.
/// Overwrites any prior content.
pub fn append_questions(
    work_dir: &Path,
    stage_id: &str,
    questions: &[String],
    action: &str,
) -> Result<()> {
    let path = feedback_path(work_dir, stage_id);
    ensure_parent(&path)?;
    let mut body = String::from(
        "The adjudicator needs more evidence before deciding. Answer these in your next dispute:\n\n",
    );
    for (i, q) in questions.iter().enumerate() {
        body.push_str(&format!("{}. {}\n", i + 1, q));
    }
    body.push('\n');
    body.push_str(&format!("Action: {action}\n"));
    fs::write(&path, body).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// Write feedback the caller composed: the outcome of a findings, contract or
/// integrity dispute. Overwrites any prior content.
pub fn write_notice(work_dir: &Path, stage_id: &str, body: &str) -> Result<()> {
    let path = feedback_path(work_dir, stage_id);
    ensure_parent(&path)?;
    fs::write(&path, body).with_context(|| format!("Failed to write {}", path.display()))
}

/// Append what stopped the daemon's completion after a verdict (a failed run
/// or an eligibility error) to the stage's feedback, creating the file when missing and keeping the verdict's
/// notice above it. Appends nothing for an empty `failures`.
///
/// Each failure is flattened to one line and bulleted, so command output such
/// as `## heading` or an escape sequence never forms a heading or reaches a
/// terminal raw.
pub fn append_completion_failures(
    work_dir: &Path,
    stage_id: &str,
    failures: &[String],
) -> Result<()> {
    if failures.is_empty() {
        return Ok(());
    }
    let path = feedback_path(work_dir, stage_id);
    ensure_parent(&path)?;
    let dir = path.parent().context("feedback path has no parent")?;
    let mut fd = safe_open_append(dir, Path::new(FEEDBACK_FILENAME))
        .with_context(|| format!("Failed to open {}", path.display()))?;
    fd.write_all(completion_failures_section(failures).as_bytes())
        .with_context(|| format!("Failed to append to {}", path.display()))
}

fn completion_failures_section(failures: &[String]) -> String {
    let mut body = String::from(
        "\n### Completion after the verdict\n\nThe verdict changed nothing in the plan, so loom \
         tried to complete the stage on the committed tree without a session. It did not \
         complete:\n\n",
    );
    for failure in failures.iter().take(MAX_COMPLETION_FAILURES) {
        let line = truncate_to_byte_limit(&single_line(failure), MAX_FAILURE_LINE_BYTES);
        body.push_str(&format!("- {line}\n"));
    }
    if failures.len() > MAX_COMPLETION_FAILURES {
        body.push_str(&format!(
            "- and {} more; `loom stage complete` lists every failure\n",
            failures.len() - MAX_COMPLETION_FAILURES
        ));
    }
    body.push_str("\nAction: address these, commit, then run `loom stage complete`.\n");
    body
}

/// Read the current feedback for a stage, if any. Returns `Ok(None)`
/// when no file exists.
pub fn read_feedback(work_dir: &Path, stage_id: &str) -> Result<Option<String>> {
    let path = feedback_path(work_dir, stage_id);
    if !path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    Ok(Some(content))
}

/// Remove the feedback file (idempotent). Called once the agent's
/// follow-up dispute is filed, so a future Accept verdict doesn't carry
/// stale text into the next signal.
pub fn clear_feedback(work_dir: &Path, stage_id: &str) -> Result<()> {
    let path = feedback_path(work_dir, stage_id);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::anyhow!("Failed to remove {}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_work() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn append_rejection_writes_expected_body() {
        let tmp = temp_work();
        let work = tmp.path();
        let citations = vec![Citation {
            file: "src/a.rs".to_string(),
            line: Some(42),
            excerpt: "fn foo()".to_string(),
            claim: "function exists".to_string(),
        }];
        append_rejection(work, "stage-a", "criterion is fine", &citations).unwrap();
        let content = read_feedback(work, "stage-a").unwrap().unwrap();
        assert!(content.contains("rejected"));
        assert!(content.contains("src/a.rs:42"));
        assert!(content.contains("function exists"));
        assert!(content.contains("Do NOT re-dispute"));
    }

    #[test]
    fn append_questions_lists_them() {
        let tmp = temp_work();
        let work = tmp.path();
        append_questions(
            work,
            "stage-b",
            &[
                "why does X fail?".to_string(),
                "is Y reachable?".to_string(),
            ],
            EVIDENCE_ACTION,
        )
        .unwrap();
        let content = read_feedback(work, "stage-b").unwrap().unwrap();
        assert!(content.contains("1. why does X fail?"));
        assert!(content.contains("2. is Y reachable?"));
    }

    #[test]
    fn second_write_overwrites_first() {
        let tmp = temp_work();
        let work = tmp.path();
        append_questions(work, "stage-c", &["first".to_string()], EVIDENCE_ACTION).unwrap();
        append_rejection(work, "stage-c", "second reasoning", &[]).unwrap();
        let content = read_feedback(work, "stage-c").unwrap().unwrap();
        assert!(!content.contains("first"));
        assert!(content.contains("second reasoning"));
    }

    #[test]
    fn read_feedback_returns_none_when_missing() {
        let tmp = temp_work();
        assert!(read_feedback(tmp.path(), "missing-stage")
            .unwrap()
            .is_none());
    }

    #[test]
    fn clear_feedback_is_idempotent() {
        let tmp = temp_work();
        clear_feedback(tmp.path(), "never-written").unwrap();
        append_questions(tmp.path(), "z", &["q".to_string()], EVIDENCE_ACTION).unwrap();
        clear_feedback(tmp.path(), "z").unwrap();
        assert!(read_feedback(tmp.path(), "z").unwrap().is_none());
    }

    #[test]
    fn completion_failures_follow_the_verdict_notice() {
        let tmp = temp_work();
        let work = tmp.path();
        write_notice(work, "s", "The adjudicator accepted your dispute.\n").unwrap();
        append_completion_failures(work, "s", &["criterion 2 failed".to_string()]).unwrap();
        let content = read_feedback(work, "s").unwrap().unwrap();
        assert_eq!(
            content,
            "The adjudicator accepted your dispute.\n\n### Completion after the verdict\n\n\
             The verdict changed nothing in the plan, so loom tried to complete the stage on the \
             committed tree without a session. It did not complete:\n\n- criterion 2 failed\n\n\
             Action: address these, commit, then run `loom stage complete`.\n"
        );
    }

    #[test]
    fn completion_failure_output_cannot_form_a_heading_or_escape() {
        let tmp = temp_work();
        let work = tmp.path();
        append_completion_failures(work, "s", &["## x\u{1b}[2J\nnext".to_string()]).unwrap();
        let content = read_feedback(work, "s").unwrap().unwrap();
        assert!(!content.chars().any(|c| c.is_control() && c != '\n'));
        assert!(!content.lines().any(|line| line.starts_with("## ")));
        let bullets: Vec<&str> = content.lines().filter(|l| l.starts_with("- ")).collect();
        assert_eq!(bullets, vec!["- ## x [2J next"]);
    }

    #[test]
    fn completion_failures_list_twenty_then_count_the_rest() {
        let tmp = temp_work();
        let work = tmp.path();
        let failures: Vec<String> = (1..=25).map(|n| format!("failure {n}")).collect();
        append_completion_failures(work, "s", &failures).unwrap();
        let content = read_feedback(work, "s").unwrap().unwrap();
        let bullets: Vec<&str> = content.lines().filter(|l| l.starts_with("- ")).collect();
        assert_eq!(bullets.len(), 21);
        assert_eq!(bullets[19], "- failure 20");
        assert_eq!(
            bullets[20],
            "- and 5 more; `loom stage complete` lists every failure"
        );
    }

    #[test]
    fn completion_failures_refuse_a_symlinked_feedback_file() {
        let tmp = temp_work();
        let outside = temp_work();
        let target = outside.path().join("target.md");
        fs::write(&target, "untouched\n").unwrap();
        let path = feedback_path(tmp.path(), "s");
        ensure_parent(&path).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();

        assert!(append_completion_failures(tmp.path(), "s", &["x".to_string()]).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "untouched\n");
    }

    #[test]
    fn no_completion_failures_append_nothing() {
        let tmp = temp_work();
        append_completion_failures(tmp.path(), "s", &[]).unwrap();
        assert!(read_feedback(tmp.path(), "s").unwrap().is_none());
    }
}
