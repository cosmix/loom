//! `Stage::from_definition` for the plan `version: 2` fields.

use crate::models::stage::{PlanIdentity, Stage};
use crate::plan::schema::{ContractSpec, ReachableCheck, RegressionTest, StageDefinition};

fn v2_definition() -> StageDefinition {
    StageDefinition {
        id: "contracted".to_string(),
        name: "Contracted".to_string(),
        working_dir: "loom".to_string(),
        contracts: vec![ContractSpec {
            id: "rejects-empty-id".to_string(),
            file: "src/plan/tests.rs".to_string(),
            test: "rejects_empty_id".to_string(),
            runner: Some("cargo".to_string()),
            scenario: "a stage whose id is empty".to_string(),
            rejects: "a validator that skips the id check".to_string(),
        }],
        harness: vec!["tests/fixtures/**".to_string()],
        reachable: vec![ReachableCheck {
            symbol: "validate_contracts".to_string(),
            from: "validate".to_string(),
            min_confidence: Some(0.5),
            description: "plan validation runs the contract checks".to_string(),
        }],
        ..StageDefinition::default()
    }
}

#[test]
fn from_definition_copies_v2_fields() {
    let definition = v2_definition();
    let ratchet_files = vec!["loom/maintainability-baseline.txt".to_string()];
    let plan = PlanIdentity {
        id: "plan-v2",
        version: 2,
        ratchet_files: &ratchet_files,
    };

    let stage = Stage::from_definition(&definition, &plan);

    assert_eq!(stage.plan_id.as_deref(), Some("plan-v2"));
    assert_eq!(stage.plan_version, 2);
    assert_eq!(stage.ratchet_files, ratchet_files);
    assert_eq!(stage.contracts, definition.contracts);
    assert_eq!(stage.harness, definition.harness);
    assert_eq!(stage.reachable, definition.reachable);

    // A stage file persisted before `plan_version` existed reads as a v1 stage.
    let mut yaml = serde_yaml::to_value(&stage).expect("stage serializes");
    let fields = yaml.as_mapping_mut().expect("stage mapping");
    assert!(fields.remove("plan_version").is_some());
    let reloaded: Stage = serde_yaml::from_value(yaml).expect("v1 stage loads");
    assert_eq!(reloaded.plan_version, 1);
    assert_eq!(reloaded.contracts, definition.contracts);
    assert_eq!(reloaded.reachable, definition.reachable);
}

#[test]
fn has_any_goal_checks_true_for_reachable_only() {
    let stage = Stage {
        reachable: vec![ReachableCheck {
            symbol: "validate_contracts".to_string(),
            from: "validate".to_string(),
            min_confidence: Some(0.5),
            description: "plan validation runs the contract checks".to_string(),
        }],
        ..Stage::default()
    };

    assert!(stage.has_any_goal_checks());
}

#[test]
fn has_any_goal_checks_true_for_regression_test_only() {
    let stage = Stage {
        regression_test: Some(RegressionTest {
            file: "tests/policy.rs".to_string(),
            must_contain: vec!["regression".to_string()],
        }),
        ..Stage::default()
    };

    assert!(stage.has_any_goal_checks());
}
