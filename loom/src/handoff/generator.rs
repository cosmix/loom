//! Handoff file generation for session context exhaustion

use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};

use crate::git::branch::current_branch;
use crate::models::session::Session;
use crate::models::stage::Stage;

/// Content for generating a handoff file
#[derive(Debug, Clone)]
pub struct HandoffContent {
    pub session_id: String,
    pub stage_id: String,
    pub plan_id: Option<String>,
    pub context_percent: f32,
    pub goals: String,
    pub completed_work: Vec<String>,
    pub decisions: Vec<(String, String)>, // (decision, rationale)
    pub current_branch: Option<String>,
    pub test_status: Option<String>,
    pub files_modified: Vec<String>,
    pub next_steps: Vec<String>,
    pub learnings: Vec<String>,
}

impl HandoffContent {
    /// Create a new HandoffContent with minimal fields
    pub fn new(session_id: String, stage_id: String) -> Self {
        Self {
            session_id,
            stage_id,
            plan_id: None,
            context_percent: 0.0,
            goals: String::new(),
            completed_work: Vec::new(),
            decisions: Vec::new(),
            current_branch: None,
            test_status: None,
            files_modified: Vec::new(),
            next_steps: Vec::new(),
            learnings: Vec::new(),
        }
    }

    /// Set the context usage percentage
    pub fn with_context_percent(mut self, percent: f32) -> Self {
        self.context_percent = percent;
        self
    }

    /// Set the overall goals
    pub fn with_goals(mut self, goals: String) -> Self {
        self.goals = goals;
        self
    }

    /// Add completed work items
    pub fn with_completed_work(mut self, work: Vec<String>) -> Self {
        self.completed_work = work;
        self
    }

    /// Add decisions made
    pub fn with_decisions(mut self, decisions: Vec<(String, String)>) -> Self {
        self.decisions = decisions;
        self
    }

    /// Set current branch
    pub fn with_current_branch(mut self, branch: Option<String>) -> Self {
        self.current_branch = branch;
        self
    }

    /// Set test status
    pub fn with_test_status(mut self, status: Option<String>) -> Self {
        self.test_status = status;
        self
    }

    /// Add modified files
    pub fn with_files_modified(mut self, files: Vec<String>) -> Self {
        self.files_modified = files;
        self
    }

    /// Add next steps
    pub fn with_next_steps(mut self, steps: Vec<String>) -> Self {
        self.next_steps = steps;
        self
    }

    /// Add learnings
    pub fn with_learnings(mut self, learnings: Vec<String>) -> Self {
        self.learnings = learnings;
        self
    }

    /// Set plan ID
    pub fn with_plan_id(mut self, plan_id: Option<String>) -> Self {
        self.plan_id = plan_id;
        self
    }
}

/// Generate a handoff file for a session transitioning due to context exhaustion
///
/// # Arguments
/// * `session` - The session being handed off
/// * `stage` - The stage being worked on
/// * `content` - The handoff content
/// * `work_dir` - Path to the .work directory
///
/// # Returns
/// Path to the created handoff file
pub fn generate_handoff(
    _session: &Session,
    stage: &Stage,
    content: HandoffContent,
    work_dir: &Path,
) -> Result<PathBuf> {
    // Ensure handoffs directory exists
    let handoffs_dir = work_dir.join("handoffs");
    if !handoffs_dir.exists() {
        fs::create_dir_all(&handoffs_dir).with_context(|| {
            format!(
                "Failed to create handoffs directory: {}",
                handoffs_dir.display()
            )
        })?;
    }

    // Get next sequential number for this stage
    let handoff_number = get_next_handoff_number(&stage.id, work_dir)?;

    // Generate filename: {stage_id}-handoff-{NNN}.md
    let filename = format!("{}-handoff-{:03}.md", stage.id, handoff_number);
    let handoff_path = handoffs_dir.join(&filename);

    // Generate markdown content
    let markdown = format_handoff_markdown(&content)?;

    // Write the file
    fs::write(&handoff_path, markdown)
        .with_context(|| format!("Failed to write handoff file: {}", handoff_path.display()))?;

    Ok(handoff_path)
}

/// Format HandoffContent into markdown following the template
fn format_handoff_markdown(content: &HandoffContent) -> Result<String> {
    let now = Utc::now();
    let date = now.format("%Y-%m-%d").to_string();

    let mut md = String::new();

    // Title
    md.push_str(&format!("# Handoff: {}\n\n", content.stage_id));

    // Metadata
    md.push_str("## Metadata\n\n");
    md.push_str(&format!("- **Date**: {date}\n"));
    md.push_str(&format!("- **From**: {}\n", content.session_id));
    md.push_str("- **To**: (next session)\n");
    md.push_str(&format!("- **Stage**: {}\n", content.stage_id));
    if let Some(plan_id) = &content.plan_id {
        md.push_str(&format!("- **Plan**: {plan_id}\n"));
    }
    md.push_str(&format!(
        "- **Context**: {:.1}% (approaching threshold)\n\n",
        content.context_percent
    ));

    // Goals
    md.push_str("## Goals (What We're Building)\n\n");
    if content.goals.is_empty() {
        md.push_str("No goals specified.\n\n");
    } else {
        md.push_str(&content.goals);
        md.push_str("\n\n");
    }

    // Completed Work
    md.push_str("## Completed Work\n\n");
    if content.completed_work.is_empty() {
        md.push_str("- No work completed yet\n\n");
    } else {
        for item in &content.completed_work {
            md.push_str(&format!("- {item}\n"));
        }
        md.push('\n');
    }

    // Key Decisions Made
    md.push_str("## Key Decisions Made\n\n");
    if content.decisions.is_empty() {
        md.push_str("No decisions documented.\n\n");
    } else {
        md.push_str("| Decision | Rationale |\n");
        md.push_str("|----------|----------|\n");
        for (decision, rationale) in &content.decisions {
            // Escape pipe characters in table cells
            let decision_escaped = decision.replace('|', "\\|");
            let rationale_escaped = rationale.replace('|', "\\|");
            md.push_str(&format!("| {decision_escaped} | {rationale_escaped} |\n"));
        }
        md.push('\n');
    }

    // Current State
    md.push_str("## Current State\n\n");
    if let Some(branch) = &content.current_branch {
        md.push_str(&format!("- **Branch**: {branch}\n"));
    } else {
        md.push_str("- **Branch**: (unknown)\n");
    }
    if let Some(test_status) = &content.test_status {
        md.push_str(&format!("- **Tests**: {test_status}\n"));
    } else {
        md.push_str("- **Tests**: (not run)\n");
    }
    md.push_str("- **Files Modified**:\n");
    if content.files_modified.is_empty() {
        md.push_str("  - (none)\n");
    } else {
        for file in &content.files_modified {
            md.push_str(&format!("  - {file}\n"));
        }
    }
    md.push('\n');

    // Next Steps
    md.push_str("## Next Steps (Prioritized)\n\n");
    if content.next_steps.is_empty() {
        md.push_str("1. Review current state and determine next actions\n\n");
    } else {
        for (i, step) in content.next_steps.iter().enumerate() {
            md.push_str(&format!("{}. {step}\n", i + 1));
        }
        md.push('\n');
    }

    // Learnings / Patterns Identified
    md.push_str("## Learnings / Patterns Identified\n\n");
    if content.learnings.is_empty() {
        md.push_str("No learnings documented yet.\n");
    } else {
        for learning in &content.learnings {
            md.push_str(&format!("- {learning}\n"));
        }
    }

    Ok(md)
}

/// Get the next sequential handoff number for a stage
///
/// Scans existing handoff files in .work/handoffs/ and returns the next available number.
fn get_next_handoff_number(stage_id: &str, work_dir: &Path) -> Result<u32> {
    let handoffs_dir = work_dir.join("handoffs");

    // If directory doesn't exist, this is the first handoff
    if !handoffs_dir.exists() {
        return Ok(1);
    }

    // Read directory entries
    let entries = fs::read_dir(&handoffs_dir).with_context(|| {
        format!(
            "Failed to read handoffs directory: {}",
            handoffs_dir.display()
        )
    })?;

    let mut max_number = 0u32;
    let prefix = format!("{stage_id}-handoff-");

    for entry in entries {
        let entry = entry.with_context(|| "Failed to read directory entry")?;
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();

        // Check if this is a handoff file for our stage
        if let Some(rest) = filename_str.strip_prefix(&prefix) {
            // Extract number from "NNN.md"
            if let Some(num_str) = rest.strip_suffix(".md") {
                if let Ok(num) = num_str.parse::<u32>() {
                    if num > max_number {
                        max_number = num;
                    }
                }
            }
        }
    }

    Ok(max_number + 1)
}

/// Find the most recent handoff file for a stage
///
/// Returns the path to the latest handoff file, or None if no handoffs exist.
pub fn find_latest_handoff(stage_id: &str, work_dir: &Path) -> Result<Option<PathBuf>> {
    let handoffs_dir = work_dir.join("handoffs");

    // If directory doesn't exist, no handoffs exist
    if !handoffs_dir.exists() {
        return Ok(None);
    }

    // Read directory entries
    let entries = fs::read_dir(&handoffs_dir).with_context(|| {
        format!(
            "Failed to read handoffs directory: {}",
            handoffs_dir.display()
        )
    })?;

    let mut max_number = 0u32;
    let mut latest_path: Option<PathBuf> = None;
    let prefix = format!("{stage_id}-handoff-");

    for entry in entries {
        let entry = entry.with_context(|| "Failed to read directory entry")?;
        let path = entry.path();
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();

        // Check if this is a handoff file for our stage
        if let Some(rest) = filename_str.strip_prefix(&prefix) {
            // Extract number from "NNN.md"
            if let Some(num_str) = rest.strip_suffix(".md") {
                if let Ok(num) = num_str.parse::<u32>() {
                    if num > max_number {
                        max_number = num;
                        latest_path = Some(path);
                    }
                }
            }
        }
    }

    Ok(latest_path)
}

/// Detect current branch from a repository path
///
/// Helper function to get the current branch for handoff content.
pub fn detect_current_branch(repo_path: &Path) -> Result<String> {
    current_branch(repo_path)
        .with_context(|| format!("Failed to detect current branch in {}", repo_path.display()))
}

/// Get modified files from git status
///
/// Helper function to populate files_modified in handoff content.
pub fn get_modified_files(repo_path: &Path) -> Result<Vec<String>> {
    use std::process::Command;

    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .with_context(|| "Failed to run git status")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git status failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            // Parse git status --porcelain format: "XY filename"
            // Take everything after the status code (first 3 chars)
            line.get(3..).unwrap_or(line).trim().to_string()
        })
        .collect();

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_handoff_content_builder() {
        let content = HandoffContent::new("session-123".to_string(), "stage-456".to_string())
            .with_context_percent(75.5)
            .with_goals("Build feature X".to_string())
            .with_next_steps(vec!["Step 1".to_string(), "Step 2".to_string()]);

        assert_eq!(content.session_id, "session-123");
        assert_eq!(content.stage_id, "stage-456");
        assert_eq!(content.context_percent, 75.5);
        assert_eq!(content.goals, "Build feature X");
        assert_eq!(content.next_steps.len(), 2);
    }

    #[test]
    fn test_format_handoff_markdown() {
        let content = HandoffContent::new("session-abc".to_string(), "stage-xyz".to_string())
            .with_context_percent(80.0)
            .with_goals("Implement authentication".to_string())
            .with_completed_work(vec!["Created login form".to_string()])
            .with_decisions(vec![(
                "Use JWT tokens".to_string(),
                "Industry standard".to_string(),
            )])
            .with_next_steps(vec!["Add token refresh".to_string()]);

        let markdown = format_handoff_markdown(&content).unwrap();

        assert!(markdown.contains("# Handoff: stage-xyz"));
        assert!(markdown.contains("**From**: session-abc"));
        assert!(markdown.contains("**Context**: 80.0%"));
        assert!(markdown.contains("Implement authentication"));
        assert!(markdown.contains("Created login form"));
        assert!(markdown.contains("Use JWT tokens"));
        assert!(markdown.contains("Add token refresh"));
    }

    #[test]
    fn test_format_handoff_markdown_escapes_pipes() {
        let content = HandoffContent::new("session-1".to_string(), "stage-1".to_string())
            .with_decisions(vec![(
                "Choice with | pipe".to_string(),
                "Reason with | pipe".to_string(),
            )]);

        let markdown = format_handoff_markdown(&content).unwrap();

        // Should escape pipes in table cells
        assert!(markdown.contains(r"Choice with \| pipe"));
        assert!(markdown.contains(r"Reason with \| pipe"));
    }

    #[test]
    fn test_get_next_handoff_number_empty_dir() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path();

        let number = get_next_handoff_number("stage-1", work_dir).unwrap();
        assert_eq!(number, 1);
    }

    #[test]
    fn test_get_next_handoff_number_with_existing() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path();
        let handoffs_dir = work_dir.join("handoffs");
        fs::create_dir_all(&handoffs_dir).unwrap();

        // Create some existing handoff files
        fs::write(handoffs_dir.join("stage-1-handoff-001.md"), "test").unwrap();
        fs::write(handoffs_dir.join("stage-1-handoff-002.md"), "test").unwrap();
        fs::write(handoffs_dir.join("stage-2-handoff-001.md"), "test").unwrap();

        let number = get_next_handoff_number("stage-1", work_dir).unwrap();
        assert_eq!(number, 3);

        let number2 = get_next_handoff_number("stage-2", work_dir).unwrap();
        assert_eq!(number2, 2);
    }

    #[test]
    fn test_find_latest_handoff() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path();
        let handoffs_dir = work_dir.join("handoffs");
        fs::create_dir_all(&handoffs_dir).unwrap();

        // Create some handoff files
        fs::write(handoffs_dir.join("stage-1-handoff-001.md"), "first").unwrap();
        fs::write(handoffs_dir.join("stage-1-handoff-002.md"), "second").unwrap();
        fs::write(handoffs_dir.join("stage-1-handoff-003.md"), "third").unwrap();

        let latest = find_latest_handoff("stage-1", work_dir).unwrap();
        assert!(latest.is_some());

        let latest_path = latest.unwrap();
        assert!(latest_path.ends_with("stage-1-handoff-003.md"));

        let content = fs::read_to_string(latest_path).unwrap();
        assert_eq!(content, "third");
    }

    #[test]
    fn test_find_latest_handoff_none_exist() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path();

        let latest = find_latest_handoff("stage-1", work_dir).unwrap();
        assert!(latest.is_none());
    }

    #[test]
    fn test_generate_handoff() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path();

        let session = Session::new();
        let stage = Stage::new("test-stage".to_string(), Some("Test stage".to_string()));

        let content = HandoffContent::new(session.id.clone(), stage.id.clone())
            .with_context_percent(75.0)
            .with_goals("Complete test".to_string());

        let handoff_path = generate_handoff(&session, &stage, content, work_dir).unwrap();

        assert!(handoff_path.exists());
        assert!(handoff_path.to_string_lossy().contains(&stage.id));
        assert!(handoff_path.to_string_lossy().contains("handoff-001.md"));

        let content = fs::read_to_string(&handoff_path).unwrap();
        assert!(content.contains("# Handoff:"));
        assert!(content.contains(&session.id));
        assert!(content.contains("Complete test"));
    }

    #[test]
    fn test_generate_multiple_handoffs() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path();

        let session = Session::new();
        let stage = Stage::new("test-stage".to_string(), None);

        let content1 = HandoffContent::new(session.id.clone(), stage.id.clone());
        let content2 = HandoffContent::new(session.id.clone(), stage.id.clone());

        let path1 = generate_handoff(&session, &stage, content1, work_dir).unwrap();
        let path2 = generate_handoff(&session, &stage, content2, work_dir).unwrap();

        assert!(path1.to_string_lossy().contains("handoff-001.md"));
        assert!(path2.to_string_lossy().contains("handoff-002.md"));
    }
}
