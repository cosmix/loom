//! Knowledge recommendations validation tests

use crate::plan::schema::types::{StageDefinition, StageType};
use crate::plan::schema::validation::check_knowledge_recommendations;

#[test]
fn test_knowledge_recommendations_no_knowledge_stage() {
    let stages = vec![
        StageDefinition {
            id: "stage-1".to_string(),
            name: "Stage One".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::default(),
            truths: vec![],
            artifacts: vec![],
            wiring: vec![],
        },
        StageDefinition {
            id: "stage-2".to_string(),
            name: "Stage Two".to_string(),
            description: None,
            dependencies: vec!["stage-1".to_string()],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::default(),
            truths: vec![],
            artifacts: vec![],
            wiring: vec![],
        },
    ];

    let warnings = check_knowledge_recommendations(&stages);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("knowledge-bootstrap"));
}

#[test]
fn test_knowledge_recommendations_has_knowledge_id() {
    let stages = vec![
        StageDefinition {
            id: "knowledge-bootstrap".to_string(),
            name: "Bootstrap".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::default(),
            truths: vec![],
            artifacts: vec![],
            wiring: vec![],
        },
        StageDefinition {
            id: "stage-2".to_string(),
            name: "Stage Two".to_string(),
            description: None,
            dependencies: vec!["knowledge-bootstrap".to_string()],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::default(),
            truths: vec![],
            artifacts: vec![],
            wiring: vec![],
        },
    ];

    let warnings = check_knowledge_recommendations(&stages);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_has_knowledge_name() {
    let stages = vec![StageDefinition {
        id: "init-stage".to_string(),
        name: "Knowledge Bootstrap".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::default(),
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
    }];

    let warnings = check_knowledge_recommendations(&stages);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_case_insensitive() {
    let stages = vec![StageDefinition {
        id: "KNOWLEDGE-setup".to_string(),
        name: "Setup".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::default(),
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
    }];

    let warnings = check_knowledge_recommendations(&stages);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_no_root_stages() {
    // This scenario shouldn't happen in practice (plans need at least one root),
    // but if all stages have dependencies, no warning should be shown
    let stages = vec![StageDefinition {
        id: "stage-1".to_string(),
        name: "Stage One".to_string(),
        description: None,
        dependencies: vec!["nonexistent".to_string()],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::default(),
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
    }];

    let warnings = check_knowledge_recommendations(&stages);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_empty_stages() {
    let stages: Vec<StageDefinition> = vec![];
    let warnings = check_knowledge_recommendations(&stages);
    assert!(warnings.is_empty());
}
