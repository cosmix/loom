//! Upper budgets for the commands in stage verification.

use crate::models::stage::{AcceptanceCriterion, Stage, TruthCheck};
use crate::plan::schema::StageDefinition;
use crate::verify::criteria::{DEFAULT_CHECK_TIMEOUT, DEFAULT_COMMAND_TIMEOUT};

pub(crate) fn after_stage_secs(checks: &[TruthCheck]) -> u64 {
    checks.iter().fold(0, |sum: u64, check| {
        sum.saturating_add(check.timeout(DEFAULT_CHECK_TIMEOUT).as_secs())
    })
}

pub(crate) fn stage_command_secs(stage: &Stage) -> u64 {
    let counts = [
        stage.wiring_tests.len(),
        usize::from(stage.dead_code_check.is_some()),
        usize::from(stage.regression_test.is_some()),
        stage.contracts.len(),
        1,
    ];
    command_secs(stage, counts, &stage.after_stage)
}

/// The child takes goal checks and postconditions from the active plan.
pub(crate) fn stage_command_secs_with_plan(
    stage: &Stage,
    plan: Option<&StageDefinition>,
    include_after: bool,
) -> u64 {
    let counts = match plan {
        Some(def) => [
            def.wiring_tests.len(),
            usize::from(def.dead_code_check.is_some()),
            usize::from(def.regression_test.is_some()),
            stage.contracts.len(),
            1,
        ],
        None => [
            stage.wiring_tests.len(),
            usize::from(stage.dead_code_check.is_some()),
            usize::from(stage.regression_test.is_some()),
            stage.contracts.len(),
            1,
        ],
    };
    let after = if include_after {
        plan.map_or(&[][..], |def| &def.after_stage[..])
    } else {
        &[][..]
    };
    command_secs(stage, counts, after)
}

fn command_secs(stage: &Stage, counts: [usize; 5], after_stage: &[TruthCheck]) -> u64 {
    let acceptance = stage.acceptance.iter().fold(0u64, |sum, criterion| {
        let secs = match criterion {
            AcceptanceCriterion::Extended(check) => check
                .timeout_secs
                .unwrap_or(DEFAULT_CHECK_TIMEOUT.as_secs()),
            AcceptanceCriterion::Simple(_) => DEFAULT_COMMAND_TIMEOUT.as_secs(),
        };
        sum.saturating_add(secs)
    });
    counts
        .into_iter()
        .fold(acceptance, |sum, count| {
            sum.saturating_add(
                u64::try_from(count)
                    .unwrap_or(u64::MAX)
                    .saturating_mul(DEFAULT_COMMAND_TIMEOUT.as_secs()),
            )
        })
        .saturating_add(after_stage_secs(after_stage))
}
