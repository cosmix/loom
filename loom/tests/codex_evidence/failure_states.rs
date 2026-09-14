use anyhow::{ensure, Result};
use loom::subagent_lifecycle::WorkerOutcome;
use serial_test::serial;
use std::fs;

use crate::assertions::{
    assert_exit, assert_state, assert_unknown_after_poll, assert_wrapper_active,
    assert_wrapper_terminal,
};
use crate::fixture::Fixture;

const PARENT: &str = "cccccccc-3333-4333-8333-333333333333";
const AGENT: &str = "forwarder-failure";

#[test]
#[serial]
fn failed_job_is_immediately_terminal_with_exit_one() -> Result<()> {
    let fixture = Fixture::new("failed")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(&forwarder, Some("unit-failed"), "job-failed", "failed")?;
    assert_wrapper_terminal(&fixture, &launch, "failed")?;

    fixture.poll()?;
    ensure!(matches!(
        fixture.lifecycle_outcome(&forwarder)?,
        WorkerOutcome::Failed(_)
    ));
    assert_state(&fixture.list(&forwarder)?, AGENT, "failed")?;
    assert_exit(&fixture.watch(&forwarder)?, 1);
    Ok(())
}

#[test]
#[serial]
fn cancelled_job_is_immediately_terminal_with_exit_three() -> Result<()> {
    let fixture = Fixture::new("cancelled")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(
        &forwarder,
        Some("unit-cancelled"),
        "job-cancelled",
        "cancelled",
    )?;
    assert_wrapper_terminal(&fixture, &launch, "cancelled")?;

    fixture.poll()?;
    ensure!(matches!(
        fixture.lifecycle_outcome(&forwarder)?,
        WorkerOutcome::Cancelled(_)
    ));
    assert_state(&fixture.list(&forwarder)?, AGENT, "cancelled")?;
    assert_exit(&fixture.watch(&forwarder)?, 3);
    Ok(())
}

#[test]
#[serial]
fn malformed_job_json_is_unknown() -> Result<()> {
    let fixture = Fixture::new("malformed")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(
        &forwarder,
        Some("unit-malformed"),
        "job-malformed",
        "running",
    )?;
    assert_wrapper_active(&fixture, &launch)?;
    fs::write(&launch.job_path, "{malformed\n")?;

    assert_unknown_after_poll(&fixture, &launch, &forwarder)
}

#[test]
#[serial]
fn pruned_job_file_is_unknown() -> Result<()> {
    let fixture = Fixture::new("pruned")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(&forwarder, Some("unit-pruned"), "job-pruned", "running")?;
    assert_wrapper_active(&fixture, &launch)?;
    fs::remove_file(&launch.job_path)?;

    assert_unknown_after_poll(&fixture, &launch, &forwarder)
}
