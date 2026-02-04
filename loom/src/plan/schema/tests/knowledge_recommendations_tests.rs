//! Knowledge recommendations validation tests

use super::make_stage;
use crate::plan::schema::types::StageDefinition;
use crate::plan::schema::validation::check_knowledge_recommendations;

#[test]
fn test_knowledge_recommendations_no_knowledge_stage() {
    let stage1 = make_stage("stage-1", "Stage One");
    let mut stage2 = make_stage("stage-2", "Stage Two");
    stage2.dependencies = vec!["stage-1".to_string()];

    let warnings = check_knowledge_recommendations(&[stage1, stage2]);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("knowledge-bootstrap"));
}

#[test]
fn test_knowledge_recommendations_has_knowledge_id() {
    let stage1 = make_stage("knowledge-bootstrap", "Bootstrap");
    let mut stage2 = make_stage("stage-2", "Stage Two");
    stage2.dependencies = vec!["knowledge-bootstrap".to_string()];

    let warnings = check_knowledge_recommendations(&[stage1, stage2]);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_has_knowledge_name() {
    let stage = make_stage("init-stage", "Knowledge Bootstrap");

    let warnings = check_knowledge_recommendations(&[stage]);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_case_insensitive() {
    let stage = make_stage("KNOWLEDGE-setup", "Setup");

    let warnings = check_knowledge_recommendations(&[stage]);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_no_root_stages() {
    // This scenario shouldn't happen in practice (plans need at least one root),
    // but if all stages have dependencies, no warning should be shown
    let mut stage = make_stage("stage-1", "Stage One");
    stage.dependencies = vec!["nonexistent".to_string()];

    let warnings = check_knowledge_recommendations(&[stage]);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_empty_stages() {
    let stages: Vec<StageDefinition> = vec![];
    let warnings = check_knowledge_recommendations(&stages);
    assert!(warnings.is_empty());
}
