//! An extended criterion's own `timeout_secs` bounds its run; without one it
//! keeps the extended default rather than the configured simple timeout.

use std::time::Duration;

use crate::models::stage::{AcceptanceCriterion, CommandConfinement, Stage, TruthCheck};
use crate::verify::criteria::config::CriteriaConfig;
use crate::verify::criteria::runner::run_acceptance_with_config;

fn sleeping_stage(timeout_secs: Option<u64>) -> Stage {
    let mut stage = Stage::new("timeouts".to_string(), None);
    stage.sandbox.command_confinement = Some(CommandConfinement::Inherit);
    stage.add_acceptance_criterion(AcceptanceCriterion::Extended(TruthCheck {
        command: "sleep 2".to_string(),
        stdout_contains: vec![],
        stdout_not_contains: vec![],
        stderr_empty: None,
        exit_code: None,
        timeout_secs,
        description: None,
    }));
    stage
}

#[test]
fn an_extended_criterion_times_out_after_its_own_timeout_secs() {
    let dir = tempfile::tempdir().unwrap();
    let config = CriteriaConfig::default();
    let result =
        run_acceptance_with_config(&sleeping_stage(Some(1)), Some(dir.path()), &config).unwrap();
    assert!(!result.all_passed());
    assert!(result.results()[0].timed_out, "{:?}", result.results()[0]);
}

#[test]
fn without_timeout_secs_an_extended_criterion_ignores_the_simple_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let config = CriteriaConfig::with_timeout(Duration::from_secs(1));
    let result =
        run_acceptance_with_config(&sleeping_stage(None), Some(dir.path()), &config).unwrap();
    assert!(result.all_passed(), "{:?}", result.results());
}
