use crate::fs::work_dir::WorkDir;
use crate::models::stage::StageStatus;
use crate::parser::markdown::MarkdownDocument;
use crate::verify::transitions::load_stage;
use crate::verify::{
    human_gate, run_acceptance, transition_stage, trigger_dependents, AcceptanceResult, GateConfig,
    GateDecision,
};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Run acceptance criteria and prompt for human approval
/// Usage: loom verify <stage_id> [--force]
pub fn execute(stage_id: String, force: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    println!("Verifying stage: {stage_id}");
    if force {
        println!("  (force mode: skipping acceptance criteria)");
    }
    println!();

    let stage = load_stage(&stage_id, work_dir.root())
        .with_context(|| format!("Failed to load stage '{stage_id}'"))?;

    if stage.status != StageStatus::Completed {
        bail!(
            "Stage '{stage_id}' is not in Completed status (current: {:?}). Only completed stages can be verified.",
            stage.status
        );
    }

    // Skip acceptance criteria if --force is set
    if !force {
        // Resolve worktree path from stage's worktree field
        let working_dir: Option<PathBuf> = stage
            .worktree
            .as_ref()
            .map(|w| PathBuf::from(".worktrees").join(w))
            .filter(|p| p.exists());

        println!("Running acceptance criteria...");
        if let Some(ref dir) = working_dir {
            println!("  (working directory: {})", dir.display());
        }
        println!();

        let acceptance_result = run_acceptance(&stage, working_dir.as_deref())
            .with_context(|| format!("Failed to run acceptance criteria for stage '{stage_id}'"))?;

        display_acceptance_results(&acceptance_result);

        if !acceptance_result.all_passed() {
            println!();
            println!("Verification FAILED: Not all acceptance criteria passed.");
            println!();
            println!("Failed criteria:");
            for failure in acceptance_result.failures() {
                println!("  - {failure}");
            }
            println!();
            println!("Tip: Use 'loom verify --force {stage_id}' to skip criteria checks.");
            bail!("Verification failed due to failing acceptance criteria");
        }

        println!();
        println!("All acceptance criteria passed!");
    }

    let config = GateConfig::new();
    let decision = human_gate(&stage, &config).context("Failed to execute human approval gate")?;

    match decision {
        GateDecision::Approved => {
            println!();
            println!("Stage approved! Transitioning to Verified status...");

            transition_stage(&stage_id, StageStatus::Verified, work_dir.root())
                .with_context(|| format!("Failed to transition stage '{stage_id}' to Verified"))?;

            // Clean up session files for this verified stage
            cleanup_sessions_for_stage(&stage_id, work_dir.root());

            let triggered = trigger_dependents(&stage_id, work_dir.root())
                .context("Failed to trigger dependent stages")?;

            if triggered.is_empty() {
                println!("No dependent stages were triggered.");
            } else {
                println!();
                println!("Triggered {} dependent stage(s):", triggered.len());
                for dep_id in &triggered {
                    println!("  - {dep_id}");
                }
            }

            println!();
            println!("Stage '{stage_id}' successfully verified!");
        }
        GateDecision::Rejected { reason } => {
            println!();
            println!("Stage verification REJECTED.");
            println!("Reason: {reason}");
            println!();
            println!("Stage '{stage_id}' remains in Completed status.");
            bail!("Stage verification rejected by reviewer");
        }
        GateDecision::Skipped => {
            println!();
            println!("Verification skipped (auto-approve mode).");
        }
    }

    Ok(())
}

fn display_acceptance_results(result: &AcceptanceResult) {
    let total = result.results().len();
    let passed = result.passed_count();
    let failed = result.failed_count();

    println!("Acceptance Results:");
    println!("  Total:  {total}");
    println!("  Passed: {passed}");
    println!("  Failed: {failed}");
    println!();

    for criterion_result in result.results() {
        println!("  {}", criterion_result.summary());
    }
}

/// Clean up session files associated with a verified stage
///
/// When a stage is verified, the tmux sessions are already gone.
/// This removes the session and signal files from `.work/` to keep status clean.
fn cleanup_sessions_for_stage(stage_id: &str, work_dir: &Path) {
    let sessions_dir = work_dir.join("sessions");
    let signals_dir = work_dir.join("signals");

    if !sessions_dir.exists() {
        return;
    }

    // Find and remove sessions associated with this stage
    let entries = match fs::read_dir(&sessions_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        // Read and parse the session file to check if it belongs to this stage
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let doc = match MarkdownDocument::parse(&content) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Check if this session belongs to the verified stage
        let session_stage_id = doc.get_frontmatter("stage_id");
        if session_stage_id.map(|s| s.as_str()) != Some(stage_id) {
            continue;
        }

        // Get the session ID for signal cleanup
        let session_id = doc.get_frontmatter("id").cloned();

        // Remove the session file
        if let Err(e) = fs::remove_file(&path) {
            eprintln!(
                "Warning: failed to remove session file '{}': {e}",
                path.display()
            );
        } else {
            println!(
                "  Cleaned up session: {}",
                path.file_stem().unwrap_or_default().to_string_lossy()
            );
        }

        // Remove the associated signal file if it exists
        if let Some(sid) = session_id {
            let signal_path = signals_dir.join(format!("{sid}.md"));
            if signal_path.exists() {
                if let Err(e) = fs::remove_file(&signal_path) {
                    eprintln!(
                        "Warning: failed to remove signal file '{}': {e}",
                        signal_path.display()
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::Stage;
    use chrono::Utc;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_stage(id: &str, status: StageStatus) -> Stage {
        Stage {
            id: id.to_string(),
            name: format!("Stage {id}"),
            description: None,
            status,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            plan_id: None,
            worktree: None,
            session: None,
            held: false,
            parent_stage: None,
            child_stages: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            close_reason: None,
        }
    }

    fn setup_work_dir() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        work_dir.initialize().unwrap();
        temp_dir
    }

    fn save_test_stage(work_dir: &Path, stage: &Stage) {
        let yaml = serde_yaml::to_string(stage).unwrap();
        let content = format!("---\n{yaml}---\n\n# Stage: {}\n", stage.name);

        let stages_dir = work_dir.join("stages");
        fs::create_dir_all(&stages_dir).unwrap();

        let stage_path = stages_dir.join(format!("00-{}.md", stage.id));
        fs::write(stage_path, content).unwrap();
    }

    #[test]
    fn test_display_acceptance_results_all_passed() {
        let result = AcceptanceResult::AllPassed { results: vec![] };

        display_acceptance_results(&result);
    }

    #[test]
    fn test_cleanup_sessions_for_stage_no_sessions_dir() {
        let temp_dir = TempDir::new().unwrap();

        cleanup_sessions_for_stage("test-stage", temp_dir.path());
    }

    #[test]
    fn test_cleanup_sessions_for_stage_empty_dir() {
        let temp_dir = setup_work_dir();

        cleanup_sessions_for_stage("test-stage", temp_dir.path().join(".work").as_path());
    }

    #[test]
    fn test_cleanup_sessions_for_stage_with_session() {
        let temp_dir = setup_work_dir();
        let work_dir = temp_dir.path().join(".work");

        let session_content = r#"---
id: session-1
stage_id: test-stage
tmux_session: null
worktree_path: null
pid: null
status: running
context_tokens: 0
context_limit: 200000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T00:00:00Z"
---

# Session: session-1
"#;

        let sessions_dir = work_dir.join("sessions");
        fs::write(sessions_dir.join("session-1.md"), session_content).unwrap();

        let signals_dir = work_dir.join("signals");
        fs::write(signals_dir.join("session-1.md"), "signal").unwrap();

        cleanup_sessions_for_stage("test-stage", &work_dir);

        assert!(!sessions_dir.join("session-1.md").exists());
        assert!(!signals_dir.join("session-1.md").exists());
    }

    #[test]
    fn test_cleanup_sessions_for_stage_ignores_other_stages() {
        let temp_dir = setup_work_dir();
        let work_dir = temp_dir.path().join(".work");

        let session_content = r#"---
id: session-1
stage_id: other-stage
tmux_session: null
worktree_path: null
pid: null
status: running
context_tokens: 0
context_limit: 200000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T00:00:00Z"
---

# Session
"#;

        let sessions_dir = work_dir.join("sessions");
        fs::write(sessions_dir.join("session-1.md"), session_content).unwrap();

        cleanup_sessions_for_stage("test-stage", &work_dir);

        assert!(sessions_dir.join("session-1.md").exists());
    }

    #[test]
    fn test_cleanup_sessions_for_stage_handles_invalid_markdown() {
        let temp_dir = setup_work_dir();
        let work_dir = temp_dir.path().join(".work");

        let sessions_dir = work_dir.join("sessions");
        fs::write(sessions_dir.join("invalid.md"), "invalid content").unwrap();

        cleanup_sessions_for_stage("test-stage", &work_dir);

        assert!(sessions_dir.join("invalid.md").exists());
    }

    #[test]
    #[serial]
    fn test_execute_requires_completed_status() {
        let temp_dir = setup_work_dir();
        let work_dir_path = temp_dir.path().join(".work");

        let stage = create_test_stage("test-stage", StageStatus::Ready);
        save_test_stage(&work_dir_path, &stage);

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = execute("test-stage".to_string(), false);

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not in Completed status"));
    }

    #[test]
    #[serial]
    fn test_execute_nonexistent_stage() {
        let temp_dir = setup_work_dir();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = execute("nonexistent".to_string(), false);

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_execute_force_skips_acceptance() {
        let temp_dir = setup_work_dir();
        let work_dir_path = temp_dir.path().join(".work");

        let mut stage = create_test_stage("test-stage", StageStatus::Completed);
        stage.acceptance = vec!["exit 1".to_string()];
        save_test_stage(&work_dir_path, &stage);

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = execute("test-stage".to_string(), true);

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_err() || result.unwrap_err().to_string().contains("gate"));
    }
}
