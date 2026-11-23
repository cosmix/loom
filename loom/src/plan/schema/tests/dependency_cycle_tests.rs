use super::{make_stage, AcceptanceCriterion, LoomConfig, LoomMetadata, StageDefinition};
use crate::plan::schema::validate;

fn stage(id: &str, dependencies: &[&str]) -> StageDefinition {
    let mut stage = make_stage(id, id);
    stage.dependencies = dependencies
        .iter()
        .map(|dependency| (*dependency).into())
        .collect();
    stage.acceptance = vec![AcceptanceCriterion::Simple("true".into())];
    stage
}

fn plan(stages: Vec<StageDefinition>) -> LoomMetadata {
    LoomMetadata {
        loom: LoomConfig {
            version: 1,
            stages,
            ..Default::default()
        },
    }
}

#[test]
fn two_stage_cycle_fails_validation() {
    let metadata = plan(vec![stage("a", &["b"]), stage("b", &["a"])]);
    let errors = validate(&metadata).expect_err("the dependency cycle must be rejected");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].to_string(),
        "Circular dependency detected: a -> b -> a"
    );
}

#[test]
fn acyclic_diamond_validates() {
    let metadata = plan(vec![
        stage("a", &[]),
        stage("b", &["a"]),
        stage("c", &["a"]),
        stage("d", &["b", "c"]),
    ]);
    validate(&metadata).expect("the diamond is acyclic");
}

#[test]
fn unknown_dependency_is_reported_without_a_cycle_error() {
    let metadata = plan(vec![stage("a", &["missing"])]);
    let errors = validate(&metadata).expect_err("the dependency must exist");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].message, "Unknown dependency: 'missing'");
}
