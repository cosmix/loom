//! Knowledge recommendations validation tests

use super::make_stage;
use crate::fs::knowledge::KnowledgeDir;
use crate::plan::schema::types::StageDefinition;
use crate::plan::schema::validation::check_knowledge_recommendations;
use tempfile::TempDir;

#[test]
fn test_knowledge_recommendations_no_knowledge_stage() {
    let stage1 = make_stage("stage-1", "Stage One");
    let mut stage2 = make_stage("stage-2", "Stage Two");
    stage2.dependencies = vec!["stage-1".to_string()];

    let warnings = check_knowledge_recommendations(&[stage1, stage2], None);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("knowledge-bootstrap"));
}

#[test]
fn test_knowledge_recommendations_has_knowledge_id() {
    let stage1 = make_stage("knowledge-bootstrap", "Bootstrap");
    let mut stage2 = make_stage("stage-2", "Stage Two");
    stage2.dependencies = vec!["knowledge-bootstrap".to_string()];

    let warnings = check_knowledge_recommendations(&[stage1, stage2], None);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_has_knowledge_name() {
    let stage = make_stage("init-stage", "Knowledge Bootstrap");

    let warnings = check_knowledge_recommendations(&[stage], None);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_case_insensitive() {
    let stage = make_stage("KNOWLEDGE-setup", "Setup");

    let warnings = check_knowledge_recommendations(&[stage], None);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_no_root_stages() {
    // This scenario shouldn't happen in practice (plans need at least one root),
    // but if all stages have dependencies, no warning should be shown
    let mut stage = make_stage("stage-1", "Stage One");
    stage.dependencies = vec!["nonexistent".to_string()];

    let warnings = check_knowledge_recommendations(&[stage], None);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_empty_stages() {
    let stages: Vec<StageDefinition> = vec![];
    let warnings = check_knowledge_recommendations(&stages, None);
    assert!(warnings.is_empty());
}

#[test]
fn a_populated_knowledge_tree_needs_no_bootstrap() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("doc/loom/knowledge");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("architecture.md"),
        "# Architecture\n\n## Module map\n\n- src/main.rs - entry point\n",
    )
    .unwrap();
    let stage = make_stage("feature", "Feature");

    let warnings = check_knowledge_recommendations(&[stage], Some(temp.path()));
    assert!(warnings.is_empty());
}

#[test]
fn a_scaffold_only_tree_still_warns() {
    let temp = TempDir::new().unwrap();
    KnowledgeDir::new(temp.path()).initialize().unwrap();
    let stage = make_stage("feature", "Feature");

    let warnings = check_knowledge_recommendations(&[stage], Some(temp.path()));
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("knowledge-bootstrap"));
}
