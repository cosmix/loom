use anyhow::{ensure, Context, Result};
use loom::models::forward_receipt::job_record::read_companion_job;
use loom::subagent_lifecycle::WorkerOutcome;
use serial_test::serial;

use crate::assertions::{
    assert_exit, assert_lifecycle_identity, assert_state, assert_wrapper_terminal,
    assert_wrapper_timed_out,
};
use crate::fixture::{Fixture, EFFORT, MODEL};

const PARENT: &str = "11111111-2222-4333-8444-555555555555";
const AGENT: &str = "forwarder-happy";

#[test]
#[serial]
fn companion_fixture_pins_v1_0_6_job_schema() -> Result<()> {
    let fixture = Fixture::new("schema")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(&forwarder, Some("unit-schema"), "job-schema", "completed")?;
    assert_wrapper_terminal(&fixture, &launch, "completed")?;

    let job = read_companion_job(&launch.job_path, "job-schema")?;
    job.validate_v1_0_6()?;
    ensure!(
        job.job_class.as_deref() == Some("task"),
        "v1.0.6 jobClass changed"
    );
    ensure!(job.write == Some(true), "v1.0.6 write changed");
    let request = job.request.context("v1.0.6 request missing")?;
    ensure!(
        request.model == MODEL && request.effort == EFFORT,
        "request fields changed"
    );
    ensure!(
        request.cwd == fixture.project && request.write,
        "request workspace/write changed"
    );
    ensure!(
        !request.resume_last && request.job_id == "job-schema",
        "request identity changed"
    );
    Ok(())
}

#[test]
#[serial]
fn completed_job_preserves_request_and_terminal_identity() -> Result<()> {
    let fixture = Fixture::new("completed")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch_with_ids(
        &forwarder,
        Some("unit-completed"),
        "job-completed",
        "completed",
        "thread-precise",
        "turn-precise",
    )?;
    assert_wrapper_terminal(&fixture, &launch, "completed")?;

    fixture.poll()?;
    ensure!(fixture.lifecycle_outcome(&forwarder)? == WorkerOutcome::Succeeded);
    let records = fixture.journal_values()?;
    ensure!(
        records.len() == 2,
        "completed reconcile must write authorization and observation"
    );
    for record in &records {
        assert_lifecycle_identity(record, &launch)?;
        ensure!(record["evidence"]["requested_model"] == MODEL);
        ensure!(record["evidence"]["requested_effort"] == EFFORT);
    }
    let terminal = records
        .iter()
        .find(|row| row["state"] == "completed")
        .context("terminal record")?;
    ensure!(terminal["evidence"]["thread_id"] == "thread-precise");
    ensure!(terminal["evidence"]["turn_id"] == "turn-precise");
    assert_state(&fixture.list(&forwarder)?, AGENT, "done")?;
    assert_exit(&fixture.watch(&launch)?, 0);
    Ok(())
}

#[test]
#[serial]
fn wrapper_timeout_cancels_the_job_and_the_daemon_observes_cancelled() -> Result<()> {
    let fixture = Fixture::new("timeout-then-cancel")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(&forwarder, Some("unit-running"), "job-running", "running")?;
    assert_wrapper_timed_out(&fixture, &launch)?;

    fixture.poll()?;
    ensure!(matches!(
        fixture.lifecycle_outcome(&forwarder)?,
        WorkerOutcome::Cancelled(_)
    ));
    assert_state(&fixture.list(&forwarder)?, AGENT, "cancelled")?;
    assert_exit(&fixture.watch(&launch)?, 3);
    Ok(())
}

#[test]
#[serial]
fn exact_job_wins_over_newer_unrelated_job() -> Result<()> {
    let fixture = Fixture::new("newer-unrelated")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(&forwarder, Some("unit-exact"), "job-exact", "completed")?;
    assert_wrapper_terminal(&fixture, &launch, "completed")?;
    let unrelated = fixture.write_unrelated_job(&launch, "job-newer")?;
    ensure!(
        unrelated.metadata()?.modified()? > launch.job_path.metadata()?.modified()?,
        "unrelated job fixture is not newer"
    );

    fixture.poll()?;
    ensure!(fixture.lifecycle_outcome(&forwarder)? == WorkerOutcome::Succeeded);
    let records = fixture.journal_values()?;
    ensure!(
        records.len() == 2,
        "unrelated job changed journal cardinality"
    );
    for record in &records {
        assert_lifecycle_identity(record, &launch)?;
    }
    assert_state(&fixture.list(&forwarder)?, AGENT, "done")?;
    assert_exit(&fixture.watch(&launch)?, 0);
    Ok(())
}
