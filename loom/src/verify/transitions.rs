//! Stage state transitions and dependency triggering
//!
//! This module handles:
//! - Transitioning stages to new statuses
//! - Triggering dependent stages when dependencies are satisfied
//! - Loading and saving stage state to/from `.work/stages/` markdown files

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::fs::stage_files::{
    compute_stage_depths, find_stage_file, stage_file_path, StageDependencies,
};
use crate::models::stage::{Stage, StageStatus};
use crate::parser::frontmatter::extract_yaml_frontmatter;

/// Transition a stage to a new status with validation
///
/// Loads the stage from `.work/stages/{stage_id}.md`, validates and updates
/// its status using validated transition methods, saves it back to disk,
/// and returns the updated stage.
///
/// # Arguments
/// * `stage_id` - The ID of the stage to transition
/// * `new_status` - The new status to assign
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// The updated stage, or an error if the transition is invalid
///
/// # Errors
/// Returns an error if:
/// - The stage cannot be loaded
/// - The transition is invalid (e.g., `Verified` -> `Pending`)
/// - The stage cannot be saved
pub fn transition_stage(stage_id: &str, new_status: StageStatus, work_dir: &Path) -> Result<Stage> {
    let mut stage = load_stage(stage_id, work_dir)
        .with_context(|| format!("Failed to load stage: {stage_id}"))?;

    // Validate and perform the transition
    stage
        .try_transition(new_status.clone())
        .with_context(|| format!("Invalid transition for stage {stage_id}"))?;

    // Handle special case for Completed which sets additional fields
    if new_status == StageStatus::Completed {
        stage.completed_at = Some(chrono::Utc::now());
    }

    save_stage(&stage, work_dir).with_context(|| format!("Failed to save stage: {stage_id}"))?;

    Ok(stage)
}

/// Trigger dependent stages when a stage is completed
///
/// Finds all stages that depend on `completed_stage_id` and checks if all
/// their dependencies are now satisfied (in Completed status). If so, marks
/// them as Ready using validated transitions.
///
/// # Arguments
/// * `completed_stage_id` - The ID of the stage that was just completed
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// List of stage IDs that were transitioned to Ready
///
/// # Note
/// Only stages in `Pending` status are eligible for triggering, which is
/// a valid transition to `Ready` per the state machine.
pub fn trigger_dependents(completed_stage_id: &str, work_dir: &Path) -> Result<Vec<String>> {
    let all_stages = list_all_stages(work_dir)?;
    let mut triggered = Vec::new();

    for mut stage in all_stages {
        if !stage.dependencies.contains(&completed_stage_id.to_string()) {
            continue;
        }

        // Only Pending stages can be triggered to Ready
        if stage.status != StageStatus::WaitingForDeps {
            continue;
        }

        if are_all_dependencies_satisfied(&stage, work_dir)? {
            // Use validated transition - Pending -> Ready is always valid
            stage.try_mark_queued().with_context(|| {
                format!(
                    "Failed to transition stage {} from {:?} to Ready",
                    stage.id, stage.status
                )
            })?;
            let stage_id = &stage.id;
            save_stage(&stage, work_dir)
                .with_context(|| format!("Failed to save triggered stage: {stage_id}"))?;
            triggered.push(stage.id.clone());
        }
    }

    Ok(triggered)
}

/// Check if all dependencies of a stage are satisfied
///
/// A dependency is satisfied if its status is Completed.
///
/// # Arguments
/// * `stage` - The stage to check dependencies for
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// `true` if all dependencies are in Completed status, `false` otherwise
fn are_all_dependencies_satisfied(stage: &Stage, work_dir: &Path) -> Result<bool> {
    if stage.dependencies.is_empty() {
        return Ok(true);
    }

    for dep_id in &stage.dependencies {
        let dep_stage = load_stage(dep_id, work_dir).with_context(|| {
            format!(
                "Failed to load dependency stage {} for stage {}",
                dep_id, stage.id
            )
        })?;

        if dep_stage.status != StageStatus::Completed {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Load a stage from disk
///
/// Finds and reads the stage file from `.work/stages/`, handling both
/// prefixed (e.g., `01-stage-id.md`) and non-prefixed (`stage-id.md`) formats.
///
/// # Arguments
/// * `stage_id` - The ID of the stage to load
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// The loaded stage
pub fn load_stage(stage_id: &str, work_dir: &Path) -> Result<Stage> {
    let stages_dir = work_dir.join("stages");

    let stage_path = find_stage_file(&stages_dir, stage_id)?
        .ok_or_else(|| anyhow::anyhow!("Stage file not found for: {stage_id}"))?;

    let content = fs::read_to_string(&stage_path)
        .with_context(|| format!("Failed to read stage file: {}", stage_path.display()))?;

    parse_stage_from_markdown(&content)
        .with_context(|| format!("Failed to parse stage from: {}", stage_path.display()))
}

/// Save a stage to disk
///
/// Serializes the stage to YAML frontmatter + markdown body and writes
/// to `.work/stages/`. Uses depth-prefixed filenames (e.g., `01-stage-id.md`)
/// for topological ordering visibility.
///
/// If the stage file already exists (with any prefix), updates it in place.
/// For new stages, computes the topological depth based on dependencies.
///
/// # Arguments
/// * `stage` - The stage to save
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// Ok(()) on success
pub fn save_stage(stage: &Stage, work_dir: &Path) -> Result<()> {
    let stages_dir = work_dir.join("stages");
    if !stages_dir.exists() {
        fs::create_dir_all(&stages_dir).with_context(|| {
            format!(
                "Failed to create stages directory: {}",
                stages_dir.display()
            )
        })?;
    }

    // Check if a file already exists for this stage (with any prefix)
    let stage_path = if let Some(existing_path) = find_stage_file(&stages_dir, &stage.id)? {
        // Update existing file in place
        existing_path
    } else {
        // New stage - compute depth and create with prefix
        let depth = compute_stage_depth(stage, work_dir)?;
        stage_file_path(&stages_dir, depth, &stage.id)
    };

    let content = serialize_stage_to_markdown(stage)?;

    fs::write(&stage_path, content)
        .with_context(|| format!("Failed to write stage file: {}", stage_path.display()))?;

    Ok(())
}

/// Compute the topological depth for a single stage based on its dependencies
/// and existing stages in the work directory.
///
/// # Arguments
/// * `stage` - The stage to compute depth for
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// The depth (0-indexed)
fn compute_stage_depth(stage: &Stage, work_dir: &Path) -> Result<usize> {
    // Load all existing stages to get their dependency info
    let existing_stages = list_all_stages(work_dir).unwrap_or_default();

    // Build dependency info including the new stage
    let mut stage_deps: Vec<StageDependencies> = existing_stages
        .iter()
        .map(|s| StageDependencies {
            id: s.id.clone(),
            dependencies: s.dependencies.clone(),
        })
        .collect();

    // Add the current stage if not already present
    if !stage_deps.iter().any(|s| s.id == stage.id) {
        stage_deps.push(StageDependencies {
            id: stage.id.clone(),
            dependencies: stage.dependencies.clone(),
        });
    }

    // Compute depths for all stages
    let depths = compute_stage_depths(&stage_deps)?;

    // Return depth for this stage
    Ok(depths.get(&stage.id).copied().unwrap_or(0))
}

/// List all stages from `.work/stages/`
///
/// Reads all `.md` files in the stages directory and parses them into
/// Stage structs.
///
/// # Arguments
/// * `work_dir` - Path to the `.work` directory
///
/// # Returns
/// List of all stages
pub fn list_all_stages(work_dir: &Path) -> Result<Vec<Stage>> {
    let stages_dir = work_dir.join("stages");

    if !stages_dir.exists() {
        return Ok(Vec::new());
    }

    let mut stages = Vec::new();

    let entries = fs::read_dir(&stages_dir)
        .with_context(|| format!("Failed to read stages directory: {}", stages_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            match load_stage_from_path(&path) {
                Ok(stage) => stages.push(stage),
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to load stage from {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
    }

    Ok(stages)
}

/// Load a stage from a specific file path
fn load_stage_from_path(path: &Path) -> Result<Stage> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read stage file: {}", path.display()))?;

    parse_stage_from_markdown(&content)
        .with_context(|| format!("Failed to parse stage from: {}", path.display()))
}

/// Parse a Stage from markdown with YAML frontmatter
///
/// Expects content in the format:
/// ```markdown
/// ---
/// id: stage-1
/// name: Test Stage
/// ...
/// ---
///
/// # Stage body content
/// ```
fn parse_stage_from_markdown(content: &str) -> Result<Stage> {
    let frontmatter = extract_yaml_frontmatter(content)?;

    let stage: Stage = serde_yaml::from_value(frontmatter)
        .context("Failed to deserialize Stage from YAML frontmatter")?;

    Ok(stage)
}

/// Serialize a Stage to markdown with YAML frontmatter
///
/// Creates a markdown file with YAML frontmatter containing the stage data
/// followed by a markdown body with stage details.
pub fn serialize_stage_to_markdown(stage: &Stage) -> Result<String> {
    let yaml = serde_yaml::to_string(stage).context("Failed to serialize Stage to YAML")?;

    let mut content = String::new();
    content.push_str("---\n");
    content.push_str(&yaml);
    content.push_str("---\n\n");

    content.push_str(&format!("# Stage: {}\n\n", stage.name));

    if let Some(desc) = &stage.description {
        content.push_str(&format!("{desc}\n\n"));
    }

    content.push_str(&format!("**Status**: {:?}\n\n", stage.status));

    if !stage.dependencies.is_empty() {
        content.push_str("## Dependencies\n\n");
        for dep in &stage.dependencies {
            content.push_str(&format!("- {dep}\n"));
        }
        content.push('\n');
    }

    if !stage.acceptance.is_empty() {
        content.push_str("## Acceptance Criteria\n\n");
        for criterion in &stage.acceptance {
            content.push_str(&format!("- [ ] {criterion}\n"));
        }
        content.push('\n');
    }

    if !stage.files.is_empty() {
        content.push_str("## Files\n\n");
        for file in &stage.files {
            content.push_str(&format!("- `{file}`\n"));
        }
        content.push('\n');
    }

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_stage(id: &str, name: &str, status: StageStatus) -> Stage {
        let mut stage = Stage::new(name.to_string(), Some(format!("Test stage {name}")));
        stage.id = id.to_string();
        stage.status = status;
        stage
    }

    #[test]
    fn test_load_and_save_stage() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::WaitingForDeps);

        save_stage(&stage, work_dir).expect("Should save stage");

        let loaded = load_stage("stage-1", work_dir).expect("Should load stage");

        assert_eq!(loaded.id, stage.id);
        assert_eq!(loaded.name, stage.name);
        assert_eq!(loaded.status, stage.status);
        assert_eq!(loaded.description, stage.description);
    }

    #[test]
    fn test_load_nonexistent_stage() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let result = load_stage("nonexistent", work_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_transition_stage_to_ready() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::WaitingForDeps);
        save_stage(&stage, work_dir).expect("Should save stage");

        let updated = transition_stage("stage-1", StageStatus::Queued, work_dir)
            .expect("Should transition stage");

        assert_eq!(updated.status, StageStatus::Queued);

        let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
        assert_eq!(reloaded.status, StageStatus::Queued);
    }

    #[test]
    fn test_transition_stage_to_completed() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Executing);
        save_stage(&stage, work_dir).expect("Should save stage");

        let updated = transition_stage("stage-1", StageStatus::Completed, work_dir)
            .expect("Should transition stage");

        assert_eq!(updated.status, StageStatus::Completed);
    }

    #[test]
    fn test_list_all_stages() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::WaitingForDeps);
        let stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::Queued);
        let stage3 = create_test_stage("stage-3", "Stage 3", StageStatus::Completed);

        save_stage(&stage1, work_dir).expect("Should save stage 1");
        save_stage(&stage2, work_dir).expect("Should save stage 2");
        save_stage(&stage3, work_dir).expect("Should save stage 3");

        let stages = list_all_stages(work_dir).expect("Should list stages");

        assert_eq!(stages.len(), 3);

        let ids: Vec<String> = stages.iter().map(|s| s.id.clone()).collect();
        assert!(ids.contains(&"stage-1".to_string()));
        assert!(ids.contains(&"stage-2".to_string()));
        assert!(ids.contains(&"stage-3".to_string()));
    }

    #[test]
    fn test_list_all_stages_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stages = list_all_stages(work_dir).expect("Should handle empty directory");
        assert_eq!(stages.len(), 0);
    }

    #[test]
    fn test_trigger_dependents_single_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
        save_stage(&stage1, work_dir).expect("Should save stage 1");

        let mut stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::WaitingForDeps);
        stage2.add_dependency("stage-1".to_string());
        save_stage(&stage2, work_dir).expect("Should save stage 2");

        let triggered = trigger_dependents("stage-1", work_dir).expect("Should trigger dependents");

        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0], "stage-2");

        let reloaded = load_stage("stage-2", work_dir).expect("Should reload stage 2");
        assert_eq!(reloaded.status, StageStatus::Queued);
    }

    #[test]
    fn test_trigger_dependents_multiple_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
        save_stage(&stage1, work_dir).expect("Should save stage 1");

        let stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::WaitingForDeps);
        save_stage(&stage2, work_dir).expect("Should save stage 2");

        let mut stage3 = create_test_stage("stage-3", "Stage 3", StageStatus::WaitingForDeps);
        stage3.add_dependency("stage-1".to_string());
        stage3.add_dependency("stage-2".to_string());
        save_stage(&stage3, work_dir).expect("Should save stage 3");

        let triggered = trigger_dependents("stage-1", work_dir).expect("Should trigger dependents");
        assert_eq!(triggered.len(), 0);

        let stage2_completed = create_test_stage("stage-2", "Stage 2", StageStatus::Completed);
        save_stage(&stage2_completed, work_dir).expect("Should save stage 2");

        let triggered = trigger_dependents("stage-2", work_dir).expect("Should trigger dependents");
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0], "stage-3");

        let reloaded = load_stage("stage-3", work_dir).expect("Should reload stage 3");
        assert_eq!(reloaded.status, StageStatus::Queued);
    }

    #[test]
    fn test_trigger_dependents_no_dependents() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
        save_stage(&stage1, work_dir).expect("Should save stage 1");

        let triggered = trigger_dependents("stage-1", work_dir).expect("Should trigger dependents");

        assert_eq!(triggered.len(), 0);
    }

    #[test]
    fn test_trigger_dependents_already_ready() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
        save_stage(&stage1, work_dir).expect("Should save stage 1");

        let mut stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::Queued);
        stage2.add_dependency("stage-1".to_string());
        save_stage(&stage2, work_dir).expect("Should save stage 2");

        let triggered = trigger_dependents("stage-1", work_dir).expect("Should trigger dependents");

        assert_eq!(triggered.len(), 0);
    }

    #[test]
    fn test_serialize_and_parse_roundtrip() {
        let mut stage = create_test_stage("stage-1", "Test Stage", StageStatus::WaitingForDeps);
        stage.add_dependency("stage-0".to_string());
        stage.add_acceptance_criterion("Criterion 1".to_string());
        stage.add_acceptance_criterion("Criterion 2".to_string());
        stage.add_file_pattern("src/**/*.rs".to_string());

        let markdown = serialize_stage_to_markdown(&stage).expect("Should serialize");

        let parsed = parse_stage_from_markdown(&markdown).expect("Should parse");

        assert_eq!(parsed.id, stage.id);
        assert_eq!(parsed.name, stage.name);
        assert_eq!(parsed.status, stage.status);
        assert_eq!(parsed.dependencies, stage.dependencies);
        assert_eq!(parsed.acceptance, stage.acceptance);
        assert_eq!(parsed.files, stage.files);
    }

    #[test]
    fn test_extract_yaml_frontmatter() {
        let content = r#"---
id: stage-1
name: Test
status: Pending
---

# Body content"#;

        let yaml = extract_yaml_frontmatter(content).expect("Should extract frontmatter");
        assert!(yaml.is_mapping());

        let map = yaml.as_mapping().unwrap();
        assert_eq!(
            map.get(serde_yaml::Value::String("id".to_string()))
                .unwrap()
                .as_str()
                .unwrap(),
            "stage-1"
        );
    }

    #[test]
    fn test_extract_yaml_frontmatter_missing_delimiter() {
        let content = "id: stage-1\nname: Test";

        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No frontmatter"));
    }

    #[test]
    fn test_extract_yaml_frontmatter_unclosed() {
        let content = "---\nid: stage-1\nname: Test";

        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not properly closed"));
    }

    #[test]
    fn test_are_all_dependencies_satisfied_empty() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Stage 1", StageStatus::WaitingForDeps);

        let satisfied =
            are_all_dependencies_satisfied(&stage, work_dir).expect("Should check dependencies");

        assert!(satisfied);
    }

    #[test]
    fn test_are_all_dependencies_satisfied_true() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
        save_stage(&stage1, work_dir).expect("Should save stage 1");

        let mut stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::WaitingForDeps);
        stage2.add_dependency("stage-1".to_string());

        let satisfied =
            are_all_dependencies_satisfied(&stage2, work_dir).expect("Should check dependencies");

        assert!(satisfied);
    }

    #[test]
    fn test_are_all_dependencies_satisfied_false() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::WaitingForDeps);
        save_stage(&stage1, work_dir).expect("Should save stage 1");

        let mut stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::WaitingForDeps);
        stage2.add_dependency("stage-1".to_string());

        let satisfied =
            are_all_dependencies_satisfied(&stage2, work_dir).expect("Should check dependencies");

        assert!(!satisfied);
    }

    // =========================================================================
    // State transition validation tests
    // =========================================================================

    #[test]
    fn test_transition_stage_invalid_completed_to_pending() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Completed);
        save_stage(&stage, work_dir).expect("Should save stage");

        let result = transition_stage("stage-1", StageStatus::WaitingForDeps, work_dir);
        assert!(result.is_err());

        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Invalid") || err.contains("transition"),
            "Error should mention invalid transition: {err}"
        );

        // Verify stage status was not changed
        let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
        assert_eq!(
            reloaded.status,
            StageStatus::Completed,
            "Stage should remain in Completed status"
        );
    }

    #[test]
    fn test_transition_stage_invalid_pending_to_completed() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::WaitingForDeps);
        save_stage(&stage, work_dir).expect("Should save stage");

        let result = transition_stage("stage-1", StageStatus::Completed, work_dir);
        assert!(result.is_err());

        // Verify stage status was not changed
        let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
        assert_eq!(reloaded.status, StageStatus::WaitingForDeps);
    }

    #[test]
    fn test_transition_stage_invalid_ready_to_completed() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Queued);
        save_stage(&stage, work_dir).expect("Should save stage");

        let result = transition_stage("stage-1", StageStatus::Completed, work_dir);
        assert!(result.is_err());

        // Verify stage status was not changed
        let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
        assert_eq!(reloaded.status, StageStatus::Queued);
    }

    #[test]
    fn test_transition_stage_invalid_completed_to_executing() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Completed);
        save_stage(&stage, work_dir).expect("Should save stage");

        let result = transition_stage("stage-1", StageStatus::Executing, work_dir);
        assert!(result.is_err());

        // Verify stage status was not changed
        let reloaded = load_stage("stage-1", work_dir).expect("Should reload stage");
        assert_eq!(reloaded.status, StageStatus::Completed);
    }

    #[test]
    fn test_transition_stage_valid_full_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create stage in Pending status
        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::WaitingForDeps);
        save_stage(&stage, work_dir).expect("Should save stage");

        // Pending -> Ready (valid)
        let updated =
            transition_stage("stage-1", StageStatus::Queued, work_dir).expect("Pending->Ready");
        assert_eq!(updated.status, StageStatus::Queued);

        // Ready -> Executing (valid)
        let updated = transition_stage("stage-1", StageStatus::Executing, work_dir)
            .expect("Ready->Executing");
        assert_eq!(updated.status, StageStatus::Executing);

        // Executing -> Completed (valid, terminal state)
        let updated = transition_stage("stage-1", StageStatus::Completed, work_dir)
            .expect("Executing->Completed");
        assert_eq!(updated.status, StageStatus::Completed);

        // Completed is terminal, no further transitions allowed
        let result = transition_stage("stage-1", StageStatus::Queued, work_dir);
        assert!(result.is_err());
    }

    #[test]
    fn test_transition_stage_same_status_is_valid() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Executing);
        save_stage(&stage, work_dir).expect("Should save stage");

        // Same status transition should succeed (no-op)
        let updated = transition_stage("stage-1", StageStatus::Executing, work_dir)
            .expect("Same status should be valid");
        assert_eq!(updated.status, StageStatus::Executing);
    }

    #[test]
    fn test_transition_stage_blocked_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create stage in Executing status
        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Executing);
        save_stage(&stage, work_dir).expect("Should save stage");

        // Executing -> Blocked (valid)
        let updated = transition_stage("stage-1", StageStatus::Blocked, work_dir)
            .expect("Executing->Blocked");
        assert_eq!(updated.status, StageStatus::Blocked);

        // Blocked -> Ready (valid - unblocking)
        let updated =
            transition_stage("stage-1", StageStatus::Queued, work_dir).expect("Blocked->Ready");
        assert_eq!(updated.status, StageStatus::Queued);
    }

    #[test]
    fn test_transition_stage_handoff_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create stage in Executing status
        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Executing);
        save_stage(&stage, work_dir).expect("Should save stage");

        // Executing -> NeedsHandoff (valid)
        let updated = transition_stage("stage-1", StageStatus::NeedsHandoff, work_dir)
            .expect("Executing->NeedsHandoff");
        assert_eq!(updated.status, StageStatus::NeedsHandoff);

        // NeedsHandoff -> Ready (valid - resuming)
        let updated = transition_stage("stage-1", StageStatus::Queued, work_dir)
            .expect("NeedsHandoff->Ready");
        assert_eq!(updated.status, StageStatus::Queued);
    }

    #[test]
    fn test_transition_stage_waiting_for_input() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Create stage in Executing status
        let stage = create_test_stage("stage-1", "Test Stage", StageStatus::Executing);
        save_stage(&stage, work_dir).expect("Should save stage");

        // Executing -> WaitingForInput (valid)
        let updated = transition_stage("stage-1", StageStatus::WaitingForInput, work_dir)
            .expect("Executing->WaitingForInput");
        assert_eq!(updated.status, StageStatus::WaitingForInput);

        // WaitingForInput -> Executing (valid - input received)
        let updated = transition_stage("stage-1", StageStatus::Executing, work_dir)
            .expect("WaitingForInput->Executing");
        assert_eq!(updated.status, StageStatus::Executing);
    }
}
