//! Adjudicator feedback file lifecycle.
//!
//! For each stage that has been disputed, a single
//! `.work/disputes/<stage>/feedback.md` file holds the most recent
//! human-readable advice the adjudicator surfaced (rejection reasoning,
//! or follow-up evidence questions). The next signal generated for that
//! stage embeds the file's contents so the agent reads them on resume.
//!
//! The file is overwritten on every adjudicator pass — older feedback
//! is intentionally discarded, because the agent only needs the latest
//! verdict to make progress. Persistent history lives in the per-
//! dispute `verdict.md` records.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::dispute::Citation;

const FEEDBACK_FILENAME: &str = "feedback.md";

/// Path to `.work/disputes/<stage>/feedback.md`. The parent directory
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

/// Write rejection feedback: the adjudicator concluded the criterion is
/// correct and the agent must fix the implementation. Overwrites any
/// prior content.
pub fn append_rejection(
    work_dir: &Path,
    stage_id: &str,
    reasoning: &str,
    citations: &[Citation],
) -> Result<()> {
    let path = feedback_path(work_dir, stage_id);
    ensure_parent(&path)?;
    let mut body =
        String::from("The adjudicator rejected your dispute. The acceptance criterion stands.\n\n");
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
    body.push_str("Action: fix the implementation so the criterion passes. Do NOT re-dispute.\n");
    fs::write(&path, body).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// Write evidence-loop feedback: the adjudicator wants the agent to
/// answer specific questions before re-disputing. Overwrites any prior
/// content.
pub fn append_questions(work_dir: &Path, stage_id: &str, questions: &[String]) -> Result<()> {
    let path = feedback_path(work_dir, stage_id);
    ensure_parent(&path)?;
    let mut body = String::from(
        "The adjudicator needs more evidence before deciding. Answer these in your next dispute:\n\n",
    );
    for (i, q) in questions.iter().enumerate() {
        body.push_str(&format!("{}. {}\n", i + 1, q));
    }
    body.push('\n');
    body.push_str(
        "Action: gather the requested evidence, commit it, and file a follow-up dispute.\n",
    );
    fs::write(&path, body).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
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
        append_questions(work, "stage-c", &["first".to_string()]).unwrap();
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
        append_questions(tmp.path(), "z", &["q".to_string()]).unwrap();
        clear_feedback(tmp.path(), "z").unwrap();
        assert!(read_feedback(tmp.path(), "z").unwrap().is_none());
    }
}
