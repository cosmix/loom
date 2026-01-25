//! Knowledge stage signal generation
//!
//! Knowledge stages run in the main repository (not worktrees) and focus on
//! exploring and documenting the codebase. They don't require commits or merges.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::session::Session;
use crate::models::stage::Stage;

use super::cache::generate_knowledge_stable_prefix;
use super::generate::build_embedded_context_with_session;
use super::types::DependencyStatus;

/// Generate a signal file for a knowledge stage
///
/// Knowledge stages differ from regular stages:
/// - Run in the main repository (no worktree)
/// - No commits or merges required
/// - Focus on populating doc/loom/knowledge/
pub fn generate_knowledge_signal(
    session: &Session,
    stage: &Stage,
    repo_root: &Path,
    dependencies_status: &[DependencyStatus],
    work_dir: &Path,
) -> Result<PathBuf> {
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;
    }

    // Build embedded context
    let embedded_context =
        build_embedded_context_with_session(work_dir, None, &stage.id, Some(&session.id));

    let signal_path = signals_dir.join(format!("{}.md", session.id));
    let content = format_knowledge_signal_content(
        session,
        stage,
        repo_root,
        dependencies_status,
        &embedded_context,
    );

    fs::write(&signal_path, content).with_context(|| {
        format!(
            "Failed to write knowledge signal file: {}",
            signal_path.display()
        )
    })?;

    Ok(signal_path)
}

/// Format the content for a knowledge stage signal
fn format_knowledge_signal_content(
    session: &Session,
    stage: &Stage,
    repo_root: &Path,
    dependencies_status: &[DependencyStatus],
    embedded_context: &super::types::EmbeddedContext,
) -> String {
    let mut content = String::new();

    // Header with session ID
    content.push_str(&format!("# Signal: {}\n\n", &session.id));

    // Knowledge-specific stable prefix
    content.push_str(&generate_knowledge_stable_prefix());

    // Target section
    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {}\n", &session.id));
    content.push_str(&format!("- **Stage**: {}\n", &stage.id));
    content.push_str("- **Type**: Knowledge (no worktree)\n");
    if let Some(plan_id) = &stage.plan_id {
        content.push_str(&format!("- **Plan**: {plan_id}\n"));
    }
    content.push_str(&format!("- **Directory**: {}\n", repo_root.display()));
    content.push('\n');

    // Plan overview if available
    if let Some(plan_overview) = &embedded_context.plan_overview {
        content.push_str("## Plan Overview\n\n");
        content.push_str("<plan-overview>\n");
        content.push_str(plan_overview);
        content.push_str("\n</plan-overview>\n\n");
    }

    // Assignment section
    content.push_str("## Assignment\n\n");
    content.push_str(&format!("{}: ", &stage.name));
    if let Some(desc) = &stage.description {
        content.push_str(desc);
    } else {
        content.push_str("(no description provided)");
    }
    content.push_str("\n\n");

    // Dependencies status
    if !dependencies_status.is_empty() {
        content.push_str("## Dependencies Status\n\n");
        content.push_str(&super::format::format_dependency_table(dependencies_status));
        content.push('\n');
    }

    // Acceptance Criteria
    content.push_str("## Acceptance Criteria\n\n");
    if stage.acceptance.is_empty() {
        content.push_str("- [ ] Knowledge files populated in doc/loom/knowledge/\n");
        content.push_str("- [ ] Entry points documented\n");
        content.push_str("- [ ] Key patterns documented\n");
        content.push_str("- [ ] Coding conventions documented\n");
    } else {
        for criterion in &stage.acceptance {
            content.push_str(&format!("- [ ] {criterion}\n"));
        }
    }
    content.push('\n');

    // Files to explore (for knowledge stages this is read-only focus)
    if !stage.files.is_empty() {
        content.push_str("## Files to Explore\n\n");
        for file in &stage.files {
            content.push_str(&format!("- {file}\n"));
        }
        content.push('\n');
    }

    // Immediate tasks
    content.push_str("## Immediate Tasks\n\n");
    let tasks = extract_tasks_from_stage(stage);
    if tasks.is_empty() {
        content.push_str("1. Explore the codebase starting from entry points\n");
        content
            .push_str("2. Document key architectural patterns in doc/loom/knowledge/patterns.md\n");
        content.push_str("3. Document coding conventions in doc/loom/knowledge/conventions.md\n");
        content.push_str("4. Verify acceptance criteria are met\n");
        content.push_str(&format!("5. Run `loom stage complete {}`\n", &stage.id));
    } else {
        for (i, task) in tasks.iter().enumerate() {
            content.push_str(&format!("{}. {task}\n", i + 1));
        }
    }
    content.push('\n');

    content
}

/// Extract tasks from stage description
fn extract_tasks_from_stage(stage: &Stage) -> Vec<String> {
    let mut tasks = Vec::new();

    if let Some(desc) = &stage.description {
        tasks.extend(extract_tasks_from_description(desc));
    }

    if tasks.is_empty() && !stage.acceptance.is_empty() {
        for criterion in &stage.acceptance {
            tasks.push(criterion.clone());
        }
    }

    tasks
}

/// Extract tasks from description text
fn extract_tasks_from_description(description: &str) -> Vec<String> {
    let mut tasks = Vec::new();

    for line in description.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            tasks.push(trimmed[2..].trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
            if let Some(task) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
                tasks.push(task.trim().to_string());
            }
        }
    }

    tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::StageStatus;
    use chrono::Utc;

    fn create_test_stage() -> Stage {
        Stage {
            id: "knowledge-bootstrap".to_string(),
            name: "Bootstrap Knowledge Base".to_string(),
            description: Some("Explore the codebase and document findings.".to_string()),
            status: StageStatus::Queued,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![
                "grep -q '## ' doc/loom/knowledge/entry-points.md".to_string(),
                "grep -q '## ' doc/loom/knowledge/patterns.md".to_string(),
            ],
            setup: vec![],
            files: vec!["src/**/*.rs".to_string()],
            stage_type: crate::models::stage::StageType::Knowledge,
            plan_id: Some("test-plan".to_string()),
            worktree: None,
            session: None,
            held: false,
            parent_stage: None,
            child_stages: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            started_at: None,
            duration_secs: None,
            close_reason: None,
            auto_merge: None,
            working_dir: Some(".".to_string()),
            retry_count: 0,
            max_retries: None,
            last_failure_at: None,
            failure_info: None,
            resolved_base: None,
            base_branch: None,
            base_merged_from: vec![],
            outputs: vec![],
            completed_commit: None,
            merged: false,
            merge_conflict: false,
            verification_status: Default::default(),
        }
    }

    #[test]
    fn test_format_knowledge_signal_contains_required_sections() {
        let session = Session::new();
        let stage = create_test_stage();
        let repo_root = PathBuf::from("/repo");
        let deps: Vec<DependencyStatus> = vec![];
        let embedded = super::super::types::EmbeddedContext::default();

        let content =
            format_knowledge_signal_content(&session, &stage, &repo_root, &deps, &embedded);

        assert!(content.contains("# Signal:"));
        assert!(content.contains("## Knowledge Stage Context"));
        assert!(content.contains("## Target"));
        assert!(content.contains("Type**: Knowledge"));
        assert!(content.contains("## Assignment"));
        assert!(content.contains("## Acceptance Criteria"));
        assert!(content.contains("## Files to Explore"));
        assert!(content.contains("## Immediate Tasks"));
    }

    #[test]
    fn test_format_knowledge_signal_no_worktree_instructions() {
        let session = Session::new();
        let stage = create_test_stage();
        let repo_root = PathBuf::from("/repo");
        let deps: Vec<DependencyStatus> = vec![];
        let embedded = super::super::types::EmbeddedContext::default();

        let content =
            format_knowledge_signal_content(&session, &stage, &repo_root, &deps, &embedded);

        // Should NOT contain worktree-specific instructions
        assert!(!content.contains("Worktree Context"));
        assert!(!content.contains("isolated git worktree"));
        assert!(!content.contains("git add <specific-files>"));

        // Should contain knowledge-specific instructions
        assert!(content.contains("NO COMMITS REQUIRED"));
        assert!(content.contains("main repository"));
    }
}
