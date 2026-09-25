//! Tests for `StageDefinition::has_any_goal_checks`.

use super::make_stage;
use crate::plan::schema::types::{ReachableCheck, RegressionTest};

#[test]
fn reachable_alone_counts_as_a_goal_check() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.reachable = vec![ReachableCheck {
        symbol: "crate::verify::goal_backward::reachable_gaps".to_string(),
        from: "crate::verify::goal_backward::run_goal_backward_verification".to_string(),
        min_confidence: None,
        description: "reachable_gaps runs from run_goal_backward_verification".to_string(),
    }];

    assert!(stage.has_any_goal_checks());
}

#[test]
fn regression_test_alone_counts_as_a_goal_check() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.regression_test = Some(RegressionTest {
        file: "tests/regression.rs".to_string(),
        must_contain: vec!["reproduces_bug".to_string()],
    });

    assert!(stage.has_any_goal_checks());
}

#[test]
fn no_goal_checks_means_none_declared() {
    let stage = make_stage("stage-1", "Stage One");
    assert!(!stage.has_any_goal_checks());
}
