//! The timeout an extended acceptance criterion contributes to the hold ceiling.

use std::time::Duration;

use super::check_run_limit;
use crate::models::stage::{AcceptanceCriterion, Stage, TruthCheck};

fn stage_with(timeout_secs: Option<u64>) -> Stage {
    let check = TruthCheck {
        command: "true".to_string(),
        stdout_contains: Vec::new(),
        stdout_not_contains: Vec::new(),
        stderr_empty: None,
        exit_code: None,
        timeout_secs,
        description: None,
    };
    Stage {
        acceptance: vec![AcceptanceCriterion::Extended(check)],
        ..Stage::default()
    }
}

#[test]
fn an_extended_criterion_without_timeout_secs_holds_for_300_seconds() {
    assert_eq!(check_run_limit(&stage_with(None)), Duration::from_secs(300));
}

#[test]
fn an_extended_criterion_keeps_its_own_timeout_secs() {
    assert_eq!(
        check_run_limit(&stage_with(Some(900))),
        Duration::from_secs(900)
    );
}
