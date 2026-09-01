//! Adjudication signal generation.
//!
//! An adjudication session judges one disputed acceptance criterion. Unlike a
//! stage signal there is no assignment, no acceptance criteria of its own and
//! no completion step: the whole job is stated by
//! [`crate::orchestrator::adjudication::prompt`], and the session reports back
//! by running `loom stage adjudicate`.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::models::dispute::DisputeRequest;
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::orchestrator::adjudication::{prompt, verdict_draft_file};

use super::helpers;

/// Generate the signal file for an adjudication session.
///
/// `plan_path` is the live plan markdown, so the briefing can quote the
/// disputed criterion as the plan states it.
pub fn generate_adjudication_signal(
    session: &Session,
    stage: &Stage,
    dispute: &DisputeRequest,
    plan_path: &Path,
    work_dir: &Path,
) -> Result<PathBuf> {
    let draft = verdict_draft_file(work_dir, &stage.id, dispute.id);
    let briefing = prompt::build(plan_path, stage, dispute, work_dir, &draft);
    let content = format_adjudication_signal_content(session, stage, dispute, &briefing.render());
    helpers::write_signal_file(&session.id, &content, work_dir)
}

fn format_adjudication_signal_content(
    session: &Session,
    stage: &Stage,
    dispute: &DisputeRequest,
    briefing: &str,
) -> String {
    let mut content = String::new();

    content.push_str(&format!("# Adjudication Signal: {}\n\n", session.id));

    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {}\n", session.id));
    content.push_str(&format!("- **Stage**: {}\n", stage.id));
    content.push_str(&format!("- **Dispute**: {}\n", dispute.id));
    content.push_str("- **Type**: Adjudication\n");
    content.push_str("- **Location**: the main repository (this is not a worktree session)\n\n");

    content.push_str(briefing);
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::StageStatus;
    use crate::plan::schema::AcceptanceCriterion;
    use chrono::Utc;

    fn stage() -> Stage {
        Stage {
            id: "s1".to_string(),
            name: "Stage One".to_string(),
            status: StageStatus::NeedsAdjudication,
            acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string())],
            ..Stage::default()
        }
    }

    fn request() -> DisputeRequest {
        DisputeRequest {
            id: 2,
            stage_id: "s1".to_string(),
            criterion_index: 0,
            reason: "criterion cannot pass".to_string(),
            evidence_commit: None,
            failure_output: Some("boom".to_string()),
            fix_attempts_at_dispute: 2,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn signal_carries_target_dispute_and_briefing() {
        let session = Session::new_adjudication("s1");
        let content =
            format_adjudication_signal_content(&session, &stage(), &request(), "## Dispute\n\nx\n");
        assert!(content.starts_with("# Adjudication Signal:"));
        assert!(content.contains("- **Stage**: s1"));
        assert!(content.contains("- **Dispute**: 2"));
        assert!(content.contains("- **Type**: Adjudication"));
        assert!(content.contains("## Dispute"));
    }

    #[test]
    fn generate_writes_the_signal_file() {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join(".loom").join("work");
        std::fs::create_dir_all(&work).unwrap();
        let plan = tmp.path().join("PLAN.md");
        std::fs::write(&plan, "# Plan\n").unwrap();

        let session = Session::new_adjudication("s1");
        let path = generate_adjudication_signal(&session, &stage(), &request(), &plan, &work)
            .expect("signal generation must succeed");

        assert_eq!(
            path,
            work.join("signals").join(format!("{}.md", session.id))
        );
        let content = std::fs::read_to_string(&path).unwrap();
        // The session's only route back is the CLI command, so it must be in
        // the file it is told to read.
        assert!(content.contains("loom stage adjudicate --stage s1 --dispute 2"));
        assert!(content.contains("verdict.json"));
    }
}
