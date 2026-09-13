use anyhow::{ensure, Result};
use loom::subagent_lifecycle::WorkerOutcome;
use serial_test::serial;
use std::fs;

use crate::assertions::{assert_exit, assert_state, assert_wrapper_terminal};
use crate::fixture::Fixture;

const PARENT: &str = "eeeeeeee-5555-4555-8555-555555555555";
const AGENT: &str = "forwarder-durable";

#[test]
#[serial]
fn duplicate_monitor_poll_is_journal_idempotent() -> Result<()> {
    let fixture = Fixture::new("duplicate-poll")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(
        &forwarder,
        Some("unit-duplicate"),
        "job-duplicate",
        "completed",
    )?;
    assert_wrapper_terminal(&fixture, &launch, "completed")?;
    let mut monitor = fixture.monitor();

    fixture.poll_monitor(&mut monitor)?;
    let first = fixture.journal_values()?.len();
    fixture.poll_monitor(&mut monitor)?;
    let second = fixture.journal_values()?.len();

    ensure!(
        first == 2 && second == first,
        "duplicate poll changed journal count"
    );
    ensure!(fixture.lifecycle_outcome(&forwarder)? == WorkerOutcome::Succeeded);
    assert_exit(&fixture.watch(&forwarder)?, 0);
    Ok(())
}

#[test]
#[serial]
fn contradictory_terminal_replay_is_unknown() -> Result<()> {
    let fixture = Fixture::new("contradictory")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(
        &forwarder,
        Some("unit-conflict"),
        "job-conflict",
        "completed",
    )?;
    assert_wrapper_terminal(&fixture, &launch, "completed")?;
    fixture.poll()?;
    fixture.set_job_status(&launch, "failed", "thread-conflict", "turn-conflict")?;

    fixture.poll()?;

    ensure!(matches!(
        fixture.lifecycle_outcome(&forwarder)?,
        WorkerOutcome::Unknown(_)
    ));
    assert_state(&fixture.list(&forwarder)?, AGENT, "forward-unknown")?;
    assert_exit(&fixture.watch(&forwarder)?, 2);
    let records = fixture.journal_values()?;
    ensure!(records.iter().any(|row| row["state"] == "completed"));
    ensure!(records.iter().any(|row| row["state"] == "failed"));
    Ok(())
}

#[test]
#[serial]
fn fresh_monitor_reconstructs_terminal_outcome_from_durable_journal() -> Result<()> {
    let fixture = Fixture::new("daemon-restart")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(&forwarder, Some("unit-restart"), "job-restart", "completed")?;
    assert_wrapper_terminal(&fixture, &launch, "completed")?;
    let mut first_daemon = fixture.monitor();
    fixture.poll_monitor(&mut first_daemon)?;
    ensure!(fixture.lifecycle_outcome(&forwarder)? == WorkerOutcome::Succeeded);
    let durable_lines = fixture.journal_values()?.len();
    drop(first_daemon);
    fs::remove_file(&launch.job_path)?;

    let mut restarted_daemon = fixture.monitor();
    fixture.poll_monitor(&mut restarted_daemon)?;

    ensure!(
        fixture.journal_values()?.len() == durable_lines,
        "restart rewrote durable evidence"
    );
    ensure!(fixture.lifecycle_outcome(&forwarder)? == WorkerOutcome::Succeeded);
    assert_state(&fixture.list(&forwarder)?, AGENT, "done")?;
    assert_exit(&fixture.watch(&forwarder)?, 0);
    Ok(())
}
