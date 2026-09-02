//! Recovery signal generation for crashed/hung sessions.
//!
//! When a session crashes or hangs, the orchestrator generates a recovery signal
//! that contains context about what was happening and how to continue.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::models::stage::Stage;

#[cfg(test)]
use std::fs;

use super::generate::build_embedded_context_with_stage;
use super::recovery_format::format_recovery_signal;
use super::recovery_types::RecoverySignalContent;

/// Generate a recovery signal file
pub fn generate_recovery_signal(
    content: &RecoverySignalContent,
    stage: &Stage,
    work_dir: &Path,
) -> Result<PathBuf> {
    // Build embedded context including any available handoff
    let handoff_file = find_recovery_handoff_for_session(
        work_dir,
        &content.stage_id,
        &content.previous_session_id,
    )?;
    let mut embedded_context = build_embedded_context_with_stage(
        work_dir,
        handoff_file.as_deref(),
        Some(&content.stage_id),
    );

    // The shared context builder takes a stage ID, not a `Stage`, so it cannot
    // build the retrieval query and leaves `context_pack` empty. Populate it
    // HERE, the one place on this path that holds the `Stage` — without it the
    // brief's `if let Some(pack)` in `format_recovery_signal` is dead code and
    // every recovery signal tells the agent to "read the Knowledge Brief first"
    // while carrying no brief at all. Retrieval still degrades to `None` on
    // failure: a retry must never fail because a brief could not be built.
    embedded_context.context_pack = super::helpers::retrieve_stage_pack(work_dir, stage);
    embedded_context.knowledge_tree_empty = super::helpers::knowledge_tree_is_empty(work_dir);

    let signal_content = format_recovery_signal(content, stage, &embedded_context);

    // Same contract as both fresh-spawn paths (`generate.rs`): the delivery
    // record goes down BEFORE the signal that quotes it, so a resumed session
    // that WAS briefed is never reported as `ContextUnavailable` and the prompt
    // hook does not re-deliver what this signal already carries.
    super::helpers::persist_delivery(work_dir, stage, &content.session_id, &embedded_context);
    super::helpers::write_signal_file(&content.session_id, &signal_content, work_dir)
}

/// Find the newest valid handoff written by the session being recovered.
pub fn find_recovery_handoff_for_session(
    work_dir: &Path,
    stage_id: &str,
    previous_session_id: &str,
) -> Result<Option<String>> {
    crate::handoff::generator::find_continuation_handoff_name(
        stage_id,
        Some(previous_session_id),
        work_dir,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::signals::recovery_types::{LastHeartbeatInfo, RecoveryReason};
    use crate::plan::schema::AcceptanceCriterion;
    use chrono::Utc;
    use tempfile::TempDir;

    fn create_test_stage() -> Stage {
        Stage {
            id: "test-stage".to_string(),
            name: "Test Stage".to_string(),
            description: Some("Test description".to_string()),
            status: crate::models::stage::StageStatus::Executing,
            acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string())],
            files: vec!["src/lib.rs".to_string()],
            plan_id: Some("test-plan".to_string()),
            worktree: Some(".worktrees/test-stage".to_string()),
            session: Some("session-123".to_string()),
            ..Stage::default()
        }
    }

    fn write_v2_handoff(path: &Path, stage_id: &str, session_id: &str) {
        let handoff = crate::handoff::HandoffV2::new(session_id, stage_id);
        fs::write(path, format!("---\n{}---\n", handoff.to_yaml().unwrap())).unwrap();
    }

    #[test]
    fn recovery_selects_the_previous_sessions_valid_handoff() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("handoffs");
        fs::create_dir_all(&dir).unwrap();
        write_v2_handoff(
            &dir.join("test-stage-handoff-001.md"),
            "test-stage",
            "session-old",
        );
        write_v2_handoff(
            &dir.join("test-stage-handoff-002.md"),
            "test-stage",
            "session-other",
        );

        assert_eq!(
            find_recovery_handoff_for_session(temp.path(), "test-stage", "session-old").unwrap(),
            Some("test-stage-handoff-001".to_string())
        );
    }

    #[test]
    fn recovery_surfaces_unreadable_handoff_uncertainty() {
        let temp = TempDir::new().unwrap();
        let path = temp
            .path()
            .join("handoffs")
            .join("test-stage-handoff-001.md");
        fs::create_dir_all(&path).unwrap();

        let error = find_recovery_handoff_for_session(temp.path(), "test-stage", "session-old")
            .unwrap_err();
        assert!(format!("{error:#}").contains("Failed to read handoff file"));
    }

    #[test]
    fn test_recovery_signal_for_crash() {
        use crate::orchestrator::signals::recovery_types::RecoverySignalContent;
        let content = RecoverySignalContent::for_crash(
            "session-new".to_string(),
            "test-stage".to_string(),
            "session-old".to_string(),
            Some(PathBuf::from(".loom/work/crashes/crash-123.md")),
            1,
        );

        assert_eq!(content.reason, RecoveryReason::Crash);
        assert_eq!(content.session_id, "session-new");
        assert_eq!(content.previous_session_id, "session-old");
        assert_eq!(content.recovery_attempt, 1);
        assert!(content.crash_report_path.is_some());
    }

    #[test]
    fn test_recovery_signal_for_hung() {
        use crate::orchestrator::signals::recovery_types::RecoverySignalContent;
        let hb = LastHeartbeatInfo {
            timestamp: Utc::now(),
            context_tokens: Some(45_000),
            last_tool: Some("Bash".to_string()),
            activity: Some("Running tests".to_string()),
        };

        let content = RecoverySignalContent::for_hung(
            "session-new".to_string(),
            "test-stage".to_string(),
            "session-old".to_string(),
            Some(hb),
            2,
        );

        assert_eq!(content.reason, RecoveryReason::Hung);
        assert!(content.last_heartbeat.is_some());
        assert_eq!(content.recovery_attempt, 2);
    }

    #[test]
    fn test_generate_recovery_signal() -> Result<()> {
        use crate::orchestrator::signals::recovery_types::RecoverySignalContent;
        let tmp = TempDir::new()?;
        let work_dir = tmp.path();

        // Create signals directory
        fs::create_dir_all(work_dir.join("signals"))?;

        let stage = create_test_stage();
        let content = RecoverySignalContent::for_crash(
            "session-recovery".to_string(),
            "test-stage".to_string(),
            "session-crashed".to_string(),
            None,
            1,
        );

        let path = generate_recovery_signal(&content, &stage, work_dir)?;
        assert!(path.exists());

        let signal_content = fs::read_to_string(&path)?;
        assert!(signal_content.contains("## Recovery Context"));
        assert!(signal_content.contains("Session crashed"));
        assert!(signal_content.contains("session-crashed"));
        // Code stage (Standard, the default): the recovery signal embeds the full
        // stable prefix, so a resumed stage gets the same execution guidance as a
        // fresh spawn — the mini adversarial code review AND the rest of the rules.
        // The prefix no longer restates CLAUDE.md doctrine (that now lives only in
        // ~/.claude/CLAUDE.md, resident in the agent's context already), so
        // "Subagent Restrictions" and "git add <specific-files>" are gone from
        // every prefix and are not asserted here anymore.
        assert!(signal_content.contains("Mini Adversarial Code Review"));
        assert!(signal_content.contains("**No duplication (DRY)**"));
        assert!(signal_content.contains("## Execution Rules"));
        assert!(signal_content
            .contains("Binding rules: ~/.claude/CLAUDE.md. This signal overrides none of them."));

        Ok(())
    }

    #[test]
    fn test_recovery_signal_omits_review_for_documentation_stage() -> Result<()> {
        use crate::orchestrator::signals::recovery_types::RecoverySignalContent;
        let tmp = TempDir::new()?;
        let work_dir = tmp.path();
        fs::create_dir_all(work_dir.join("signals"))?;

        // KnowledgeDistill is a documentation stage — no code, so no review.
        let mut stage = create_test_stage();
        stage.stage_type = crate::models::stage::StageType::KnowledgeDistill;
        let content = RecoverySignalContent::for_crash(
            "session-recovery".to_string(),
            "test-stage".to_string(),
            "session-crashed".to_string(),
            None,
            1,
        );

        let path = generate_recovery_signal(&content, &stage, work_dir)?;
        let signal_content = fs::read_to_string(&path)?;
        assert!(!signal_content.contains("Mini Adversarial Code Review"));

        Ok(())
    }
}
