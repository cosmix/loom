//! Tests for stage transitions, persistence, and serialization

use tempfile::TempDir;

use crate::models::stage::{Stage, StageStatus};
use crate::parser::frontmatter::extract_yaml_frontmatter;

use super::persistence::{list_all_stages, load_stage, save_stage};
use super::serialization::{parse_stage_from_markdown, serialize_stage_to_markdown};
use super::state::{are_all_dependencies_satisfied, transition_stage, trigger_dependents};

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

    let mut stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
    stage1.merged = true; // Dependency must be merged to satisfy
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

    let mut stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
    stage1.merged = true; // Dependency must be merged to satisfy
    save_stage(&stage1, work_dir).expect("Should save stage 1");

    let stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::WaitingForDeps);
    save_stage(&stage2, work_dir).expect("Should save stage 2");

    let mut stage3 = create_test_stage("stage-3", "Stage 3", StageStatus::WaitingForDeps);
    stage3.add_dependency("stage-1".to_string());
    stage3.add_dependency("stage-2".to_string());
    save_stage(&stage3, work_dir).expect("Should save stage 3");

    let triggered = trigger_dependents("stage-1", work_dir).expect("Should trigger dependents");
    assert_eq!(triggered.len(), 0);

    let mut stage2_completed = create_test_stage("stage-2", "Stage 2", StageStatus::Completed);
    stage2_completed.merged = true; // Dependency must be merged to satisfy
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
fn are_all_dependencies_satisfied_empty() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let stage = create_test_stage("stage-1", "Stage 1", StageStatus::WaitingForDeps);

    let satisfied =
        are_all_dependencies_satisfied(&stage, work_dir).expect("Should check dependencies");

    assert!(satisfied);
}

#[test]
fn are_all_dependencies_satisfied_true() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let mut stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
    stage1.merged = true; // Must be merged to satisfy dependency
    save_stage(&stage1, work_dir).expect("Should save stage 1");

    let mut stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::WaitingForDeps);
    stage2.add_dependency("stage-1".to_string());

    let satisfied =
        are_all_dependencies_satisfied(&stage2, work_dir).expect("Should check dependencies");

    assert!(satisfied);
}

#[test]
fn are_all_dependencies_satisfied_false() {
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

#[test]
fn are_all_dependencies_satisfied_requires_merged_flag() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create dependency stage: Completed but NOT merged
    let mut dep_stage = create_test_stage("dep-stage", "Dependency", StageStatus::Completed);
    dep_stage.merged = false; // Explicitly not merged
    save_stage(&dep_stage, work_dir).expect("Should save dep stage");

    let mut dependent_stage =
        create_test_stage("dependent-stage", "Dependent", StageStatus::WaitingForDeps);
    dependent_stage.add_dependency("dep-stage".to_string());

    // Should return false: Completed but not merged
    let satisfied = are_all_dependencies_satisfied(&dependent_stage, work_dir)
        .expect("Should check dependencies");
    assert!(
        !satisfied,
        "Dependency should NOT be satisfied when Completed but merged=false"
    );

    // Now set merged = true
    dep_stage.merged = true;
    save_stage(&dep_stage, work_dir).expect("Should save dep stage with merged=true");

    // Should return true: Completed AND merged
    let satisfied = are_all_dependencies_satisfied(&dependent_stage, work_dir)
        .expect("Should check dependencies");
    assert!(
        satisfied,
        "Dependency should be satisfied when Completed AND merged=true"
    );
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
    let updated =
        transition_stage("stage-1", StageStatus::Executing, work_dir).expect("Ready->Executing");
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
    let updated =
        transition_stage("stage-1", StageStatus::Blocked, work_dir).expect("Executing->Blocked");
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
    let updated =
        transition_stage("stage-1", StageStatus::Queued, work_dir).expect("NeedsHandoff->Ready");
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
