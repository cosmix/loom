//! Recovery signal generation for crashed/hung sessions.
//!
//! When a session crashes or hangs, the orchestrator generates a recovery signal
//! that contains context about what was happening and how to continue.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::models::stage::Stage;

#[cfg(test)]
use std::fs;

use super::generate::{build_dependency_findings_with_changes, build_embedded_context_with_stage};
use super::recovery_format::format_recovery_signal;
use super::recovery_types::RecoverySignalContent;
use super::types::EmbeddedContext;

/// Build a recovery signal's context, including any available handoff.
///
/// The shared context builder takes a stage ID, not a `Stage`, so it cannot build
/// the retrieval query: the brief and dependency findings are resolved here from
/// one dependency snapshot. Retrieval degrades to `None` on failure: a retry must
/// never fail because a brief could not be built.
fn build_recovery_context(
    content: &RecoverySignalContent,
    stage: &Stage,
    work_dir: &Path,
) -> Result<EmbeddedContext> {
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
    let changes = super::retrieval::dependency_changes(work_dir, stage);
    embedded_context.context_pack =
        super::retrieval::retrieve_stage_pack_with_changes(work_dir, stage, &changes);
    embedded_context.knowledge_tree_empty = super::helpers::knowledge_tree_is_empty(work_dir);
    embedded_context.dependency_findings = build_dependency_findings_with_changes(
        work_dir,
        stage,
        embedded_context.context_pack.as_ref(),
        &changes,
    );
    Ok(embedded_context)
}

/// Generate a recovery signal file
///
/// Append dependency findings so resumed stages retain the fresh signal context.
pub fn generate_recovery_signal(
    content: &RecoverySignalContent,
    stage: &Stage,
    work_dir: &Path,
) -> Result<PathBuf> {
    let embedded_context = build_recovery_context(content, stage, work_dir)?;

    let mut signal_content = format_recovery_signal(content, stage, &embedded_context);
    if let Some(findings) = &embedded_context.dependency_findings {
        super::helpers::ensure_trailing_newline(&mut signal_content);
        signal_content.push_str(&format!("{findings}\n\n"));
    }
    // A recovery signal briefs an implementation session, which `start_stage`
    // spawns only once a v2 stage's contracts are frozen: it needs the same
    // frozen-contracts block a fresh implementation signal carries.
    super::v2_section::append_v2_section(&mut signal_content, stage, work_dir);
    // A resumed integration-verify stage keeps the spec-conformance duty and its section list.
    super::generate::append_product_sections(&mut signal_content, stage, work_dir);
    // A stage handed back from a merge resolver must see the failure it has to
    // fix on every resume, not only in its first signal.
    super::section_formatters::append_merge_handback_section(&mut signal_content, stage);

    super::helpers::publish_signal(
        &content.session_id,
        &signal_content,
        work_dir,
        stage,
        &embedded_context,
    )
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

    /// `loom stage retry` on a v2 stage whose contracts are frozen: the
    /// implementation session it briefs must see the frozen contracts, as a
    /// fresh implementation signal shows them; a v1 stage's signal is unchanged.
    #[test]
    fn recovery_signal_carries_frozen_contracts_for_v2_stage() -> Result<()> {
        use crate::orchestrator::signals::recovery_types::RecoverySignalContent;
        use crate::verify::contracts::test_support::{contract, write_test_freeze};
        let tmp = TempDir::new()?;
        let work_dir = tmp.path();
        fs::create_dir_all(work_dir.join("signals"))?;
        write_test_freeze(work_dir, "test-stage", "session-contract");
        let v2 = Stage {
            plan_version: 2,
            contracts: vec![contract()],
            ..create_test_stage()
        };
        let content = RecoverySignalContent::for_crash(
            "session-recovery".to_string(),
            "test-stage".to_string(),
            "session-crashed".to_string(),
            None,
            1,
        );

        let path = generate_recovery_signal(&content, &v2, work_dir)?;
        assert!(fs::read_to_string(&path)?.contains("## Frozen Contracts"));

        let v1 = Stage {
            plan_version: 1,
            ..v2
        };
        let path = generate_recovery_signal(&content, &v1, work_dir)?;
        assert!(!fs::read_to_string(&path)?.contains("## Frozen Contracts"));
        Ok(())
    }

    /// A resumed integration-verify stage keeps the section list and the spec-conformance duty.
    #[test]
    fn recovery_signal_carries_product_sections_for_integration_verify() -> Result<()> {
        use crate::orchestrator::signals::recovery_types::RecoverySignalContent;
        let tmp = TempDir::new()?;
        let work_dir = tmp.path().join(".loom/work");
        fs::create_dir_all(work_dir.join("signals"))?;
        let plan = "doc/plans/alpha/IN_PROGRESS-PLAN-x.md";
        let config = format!("[plan]\nplan_id = \"PLAN-x\"\nsource_path = \"{plan}\"\n");
        fs::write(work_dir.join("config.toml"), config)?;
        fs::create_dir_all(tmp.path().join("doc/plans/alpha"))?;
        fs::write(tmp.path().join(plan), "# X\n\nImplements: section_02\n")?;
        let content = RecoverySignalContent::for_crash(
            "session-recovery".to_string(),
            "test-stage".to_string(),
            "session-crashed".to_string(),
            None,
            1,
        );

        let verify = Stage {
            stage_type: crate::models::stage::StageType::IntegrationVerify,
            ..create_test_stage()
        };
        let path = generate_recovery_signal(&content, &verify, &work_dir)?;
        let signal = fs::read_to_string(&path)?;
        assert!(signal.contains("## Product Sections"), "{signal}");
        assert!(signal.contains("- `doc/product/alpha/section_02.md`\n"));
        assert!(signal.contains("loom memory question \"spec-conformance:"));

        let path = generate_recovery_signal(&content, &create_test_stage(), &work_dir)?;
        assert!(!fs::read_to_string(&path)?.contains("## Product Sections"));
        Ok(())
    }
}
