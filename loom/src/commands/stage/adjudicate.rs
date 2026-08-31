//! `loom stage adjudicate` — record an adjudication session's verdict.
//!
//! The adjudication session writes its JSON verdict to a file and hands that
//! file to this command. The command validates it and writes
//! `.work/disputes/<stage>/<n>/verdict.md`; the daemon applies the verdict on
//! its next poll.
//!
//! # Trust boundary
//!
//! `verdict.md` exists apart from `request.md` so that the agent whose stage
//! is under dispute cannot approve its own criterion (see
//! `doc/loom/knowledge/conventions.md` § "Dispute File Ownership Convention").
//! An adjudication session is a different session, spawned by the daemon into
//! the main repository, so it is not the party that rule excludes — but this
//! command must not become a general-purpose "write any verdict" tool either.
//! Four guards keep it narrow:
//!
//! 1. it refuses to run from inside a stage worktree (`LOOM_WORKTREE_PATH` is
//!    exported for stage sessions and for no other kind), which is exactly the
//!    party the split excludes — and the one that can be live in
//!    `NeedsAdjudication`, since filing a dispute does not end its session;
//! 2. the stage must be in `NeedsAdjudication`, so a verdict cannot be
//!    injected against a stage that is executing, completed, or merged;
//! 3. the dispute's `request.md` must exist, so a verdict cannot invent the
//!    dispute it answers;
//! 4. `verdict.md` must not exist yet, so a recorded verdict cannot be
//!    overwritten with a different one before the daemon applies it.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::models::dispute::verdict_file;
use crate::models::stage::StageStatus;
use crate::orchestrator::adjudication::{
    attempt_count, persist_verdict, read_request, resolve_model, verdict,
};
use crate::verify::transitions::{load_stage, update_stage};

/// What recording the verdict did.
#[derive(Debug, PartialEq, Eq)]
pub enum AdjudicateOutcome {
    /// `verdict.md` was written and awaits the daemon's next tick.
    Recorded,
    /// The verdict was too degenerate to act on; the stage was escalated to
    /// `NeedsHumanReview` with this reason instead.
    Escalated(String),
}

/// `loom stage adjudicate --stage <id> --dispute <n> --verdict-file <path>`.
pub fn adjudicate(stage_id: String, dispute_id: u32, verdict_path: PathBuf) -> Result<()> {
    refuse_worktree_session(std::env::var("LOOM_WORKTREE_PATH").ok().as_deref())?;

    let work_dir = Path::new(".work");
    match record_verdict(work_dir, &stage_id, dispute_id, &verdict_path)? {
        AdjudicateOutcome::Recorded => {
            println!("Recorded the verdict for stage '{stage_id}' dispute {dispute_id}.");
            println!(
                "The orchestrator applies it on its next poll; run `loom status` to watch the stage."
            );
        }
        AdjudicateOutcome::Escalated(reason) => {
            println!("Stage '{stage_id}' was escalated to NeedsHumanReview: {reason}");
        }
    }
    Ok(())
}

/// Refuse a verdict written from inside a stage worktree.
///
/// Split out as a pure function so the rule is testable without mutating the
/// process environment: the value is the only input, and an absent variable is
/// the only acceptable state.
fn refuse_worktree_session(worktree_path: Option<&str>) -> Result<()> {
    let Some(path) = worktree_path.map(str::trim).filter(|p| !p.is_empty()) else {
        return Ok(());
    };
    bail!(
        "This session runs inside the stage worktree at {path}, so it cannot record an \
         adjudication verdict: a stage may not judge its own disputed criterion. Verdicts \
         come from the adjudication session the orchestrator spawns for the dispute."
    )
}

/// The guarded write, against an explicit `.work` root so it is testable.
pub fn record_verdict(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
    verdict_path: &Path,
) -> Result<AdjudicateOutcome> {
    ensure_recordable(work_dir, stage_id, dispute_id)?;

    let raw = std::fs::read_to_string(verdict_path)
        .with_context(|| format!("Failed to read verdict file: {}", verdict_path.display()))?;

    match verdict::parse_and_validate(&raw) {
        verdict::ValidationOutcome::Verdict(v) => {
            let attempt = attempt_count(work_dir, stage_id, dispute_id).max(1);
            persist_verdict(
                work_dir,
                stage_id,
                dispute_id,
                &v,
                &resolve_model(work_dir),
                attempt,
            )
            .context("Failed to write the verdict record")?;
            Ok(AdjudicateOutcome::Recorded)
        }
        // Degenerate output (e.g. needs-more-evidence with no questions) would
        // loop the evidence round forever if it were recorded, so the stage
        // goes to a human instead and no verdict file is written.
        verdict::ValidationOutcome::Escalate { reason } => {
            update_stage(stage_id, work_dir, |s| {
                s.try_request_human_review(reason.clone()).ok();
                Ok(())
            })
            .context("Failed to escalate the stage after a degenerate verdict")?;
            Ok(AdjudicateOutcome::Escalated(reason))
        }
    }
}

/// Guards 2-4: the stage is under adjudication, the dispute exists, and no
/// verdict has been recorded for it yet.
fn ensure_recordable(work_dir: &Path, stage_id: &str, dispute_id: u32) -> Result<()> {
    let stage = load_stage(stage_id, work_dir)
        .with_context(|| format!("Failed to load stage '{stage_id}'"))?;
    if stage.status != StageStatus::NeedsAdjudication {
        bail!(
            "Stage '{stage_id}' is {}, not NeedsAdjudication, so no verdict can be recorded \
             against it.",
            stage.status
        );
    }
    read_request(work_dir, stage_id, dispute_id).with_context(|| {
        format!("No readable dispute {dispute_id} for stage '{stage_id}' to answer")
    })?;
    if verdict_file(&work_dir.join("disputes"), stage_id, dispute_id).exists() {
        bail!(
            "A verdict for stage '{stage_id}' dispute {dispute_id} has already been recorded; \
             it cannot be replaced."
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::dispute::{request_file, DisputeRequest, DisputeVerdictRecord};
    use crate::models::stage::Stage;
    use crate::plan::schema::AcceptanceCriterion;
    use chrono::Utc;

    fn setup(status: StageStatus) -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().to_path_buf();
        std::fs::create_dir_all(work.join("stages")).unwrap();
        let stage = Stage {
            id: "s1".to_string(),
            name: "s1".to_string(),
            status,
            acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string())],
            ..Stage::default()
        };
        crate::verify::transitions::save_stage(&stage, &work).unwrap();
        (tmp, work)
    }

    fn write_request(work: &Path, dispute_id: u32) {
        let disputes_root = work.join("disputes");
        std::fs::create_dir_all(disputes_root.join("s1").join(dispute_id.to_string())).unwrap();
        let req = DisputeRequest {
            id: dispute_id,
            stage_id: "s1".to_string(),
            criterion_index: 0,
            reason: "impossible".to_string(),
            evidence_commit: None,
            failure_output: None,
            fix_attempts_at_dispute: 1,
            created_at: Utc::now(),
        };
        let yaml = serde_yaml::to_string(&req).unwrap();
        std::fs::write(
            request_file(&disputes_root, "s1", dispute_id),
            format!("---\n{yaml}---\n\n# Dispute\n"),
        )
        .unwrap();
    }

    fn write_json(work: &Path, body: &serde_json::Value) -> PathBuf {
        let path = work.join("verdict.json");
        std::fs::write(&path, body.to_string()).unwrap();
        path
    }

    fn reject_json() -> serde_json::Value {
        serde_json::json!({
            "verdict": "reject",
            "reasoning": "the criterion is correct",
            "citations": [{"file": "src/a.rs", "excerpt": "fn a", "claim": "exists"}]
        })
    }

    #[test]
    fn records_a_valid_verdict() {
        let (_tmp, work) = setup(StageStatus::NeedsAdjudication);
        write_request(&work, 1);
        let json = write_json(&work, &reject_json());

        assert_eq!(
            record_verdict(&work, "s1", 1, &json).unwrap(),
            AdjudicateOutcome::Recorded
        );

        let path = verdict_file(&work.join("disputes"), "s1", 1);
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("reject"));
        // The record must parse back as the daemon reads it.
        let record: DisputeVerdictRecord =
            serde_yaml::from_str(content.split("---").nth(1).unwrap()).unwrap();
        assert_eq!(record.stage_id, "s1");
        assert_eq!(record.adjudicator_attempt_count, 1);
    }

    #[test]
    fn refuses_when_the_stage_is_not_under_adjudication() {
        let (_tmp, work) = setup(StageStatus::Executing);
        write_request(&work, 1);
        let json = write_json(&work, &reject_json());
        let err = record_verdict(&work, "s1", 1, &json).unwrap_err();
        assert!(format!("{err:#}").contains("not NeedsAdjudication"));
        assert!(!verdict_file(&work.join("disputes"), "s1", 1).exists());
    }

    #[test]
    fn refuses_a_verdict_for_a_dispute_that_was_never_filed() {
        let (_tmp, work) = setup(StageStatus::NeedsAdjudication);
        let json = write_json(&work, &reject_json());
        let err = record_verdict(&work, "s1", 7, &json).unwrap_err();
        assert!(format!("{err:#}").contains("No readable dispute 7"));
    }

    #[test]
    fn refuses_to_replace_a_recorded_verdict() {
        let (_tmp, work) = setup(StageStatus::NeedsAdjudication);
        write_request(&work, 1);
        let json = write_json(&work, &reject_json());
        record_verdict(&work, "s1", 1, &json).unwrap();

        let err = record_verdict(&work, "s1", 1, &json).unwrap_err();
        assert!(format!("{err:#}").contains("already been recorded"));
    }

    #[test]
    fn degenerate_verdict_escalates_instead_of_recording() {
        let (_tmp, work) = setup(StageStatus::NeedsAdjudication);
        write_request(&work, 1);
        // needs-more-evidence with no questions is the one shape that would
        // loop forever if recorded.
        let json = write_json(
            &work,
            &serde_json::json!({"verdict": "needs-more-evidence", "questions": []}),
        );

        match record_verdict(&work, "s1", 1, &json).unwrap() {
            AdjudicateOutcome::Escalated(reason) => assert!(!reason.is_empty()),
            other => panic!("expected escalation, got {other:?}"),
        }
        assert!(!verdict_file(&work.join("disputes"), "s1", 1).exists());
        let after = crate::verify::transitions::load_stage("s1", &work).unwrap();
        assert_eq!(after.status, StageStatus::NeedsHumanReview);
    }

    #[test]
    fn unparseable_output_is_recorded_as_needs_more_evidence() {
        let (_tmp, work) = setup(StageStatus::NeedsAdjudication);
        write_request(&work, 1);
        let path = work.join("verdict.json");
        std::fs::write(&path, "I could not decide.").unwrap();

        assert_eq!(
            record_verdict(&work, "s1", 1, &path).unwrap(),
            AdjudicateOutcome::Recorded
        );
        let content =
            std::fs::read_to_string(verdict_file(&work.join("disputes"), "s1", 1)).unwrap();
        assert!(content.contains("needs-more-evidence"));
    }

    #[test]
    fn a_stage_worktree_session_may_not_record_a_verdict() {
        assert!(refuse_worktree_session(None).is_ok());
        assert!(refuse_worktree_session(Some("  ")).is_ok());
        let err = refuse_worktree_session(Some("/repo/.worktrees/s1")).unwrap_err();
        assert!(format!("{err:#}").contains("may not judge its own disputed criterion"));
    }
}
