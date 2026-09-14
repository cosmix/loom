use anyhow::{ensure, Result};
use loom::subagent_lifecycle::WorkerOutcome;
use serial_test::serial;
use std::fs;

use crate::assertions::{
    assert_exit, assert_lifecycle_identity, assert_state, assert_wrapper_terminal,
};
use crate::fixture::{Fixture, Forwarder, Launch};

const PARENT_A: &str = "aaaaaaaa-1111-4111-8111-111111111111";
const PARENT_B: &str = "bbbbbbbb-2222-4222-8222-222222222222";

#[test]
#[serial]
fn two_parallel_units_finish_in_reverse_order() -> Result<()> {
    let fixture = Fixture::new("parallel-reverse")?;
    let first = fixture.add_forwarder(PARENT_A, "parallel-a")?;
    let second = fixture.add_forwarder(PARENT_A, "parallel-b")?;
    let (launch_a, launch_b) = launch_parallel(&fixture, &first, &second)?;
    assert_wrapper_terminal(&fixture, &launch_a, "completed")?;
    assert_wrapper_terminal(&fixture, &launch_b, "completed")?;
    ensure!(launch_a.authorization.invocation_id != launch_b.authorization.invocation_id);
    // Seed genuinely mid-flight records directly (bypassing the wrapper's
    // deadline path, which now cancels the job instead of leaving it running)
    // so the daemon has real queued/running jobs to observe out of order.
    fixture.set_job_status(&launch_a, "running", "", "")?;
    fixture.set_job_status(&launch_b, "running", "", "")?;

    fixture.poll()?;
    ensure!(fixture.lifecycle_outcome(&first)? == WorkerOutcome::Active);
    ensure!(fixture.lifecycle_outcome(&second)? == WorkerOutcome::Active);
    fixture.set_job_status(&launch_b, "completed", "thread-b", "turn-b")?;
    fixture.poll()?;
    let midway = fixture.list(&first)?;
    assert_state(&midway, "parallel-a", "forward-wait")?;
    assert_state(&midway, "parallel-b", "done")?;
    assert_exit(&fixture.watch(&launch_a)?, 2);

    fixture.set_job_status(&launch_a, "completed", "thread-a", "turn-a")?;
    fixture.poll()?;
    let settled = fixture.list(&first)?;
    assert_state(&settled, "parallel-a", "done")?;
    assert_state(&settled, "parallel-b", "done")?;
    assert_exit(&fixture.watch(&launch_a)?, 0);
    let terminal_jobs = terminal_job_ids(&fixture)?;
    ensure!(
        terminal_jobs == vec!["job-b".to_string(), "job-a".to_string()],
        "completion order changed: {terminal_jobs:?}"
    );
    Ok(())
}

fn launch_parallel(
    fixture: &Fixture,
    first: &Forwarder,
    second: &Forwarder,
) -> Result<(Launch, Launch)> {
    std::thread::scope(|scope| {
        let left = scope.spawn(|| fixture.launch(first, Some("unit-a"), "job-a", "completed"));
        let right = scope.spawn(|| fixture.launch(second, Some("unit-b"), "job-b", "completed"));
        let left = left.join().expect("first joined fake companion")?;
        let right = right.join().expect("second joined fake companion")?;
        Ok((left, right))
    })
}

fn terminal_job_ids(fixture: &Fixture) -> Result<Vec<String>> {
    let records = fixture.journal_values()?;
    Ok(records
        .iter()
        .filter(|row| row["state"] == "completed")
        .filter_map(|row| row["identity"]["execution"]["job_id"].as_str())
        .map(str::to_owned)
        .collect())
}

#[test]
#[serial]
fn same_agent_id_under_two_parents_in_one_cwd_stays_distinct() -> Result<()> {
    let fixture = Fixture::new("two-parents")?;
    let first = fixture.add_forwarder(PARENT_A, "shared-forwarder")?;
    let second = fixture.add_forwarder(PARENT_B, "shared-forwarder")?;
    let launch_a = fixture.launch(&first, Some("parent-a-unit"), "parent-a-job", "completed")?;
    let launch_b = fixture.launch(&second, Some("parent-b-unit"), "parent-b-job", "completed")?;
    assert_wrapper_terminal(&fixture, &launch_a, "completed")?;
    assert_wrapper_terminal(&fixture, &launch_b, "completed")?;

    fixture.poll()?;
    ensure!(fixture.lifecycle_outcome(&first)? == WorkerOutcome::Succeeded);
    ensure!(fixture.lifecycle_outcome(&second)? == WorkerOutcome::Succeeded);
    assert_state(&fixture.list(&first)?, "shared-forwarder", "done")?;
    assert_state(&fixture.list(&second)?, "shared-forwarder", "done")?;
    assert_exit(&fixture.watch(&launch_a)?, 0);
    assert_exit(&fixture.watch(&launch_b)?, 0);
    for record in fixture
        .journal_values()?
        .iter()
        .filter(|row| row["identity"]["parent_session_id"] == PARENT_A)
    {
        assert_lifecycle_identity(record, &launch_a)?;
    }
    Ok(())
}

#[test]
#[serial]
fn guard_derives_unit_and_mints_fresh_invocation() -> Result<()> {
    let fixture = Fixture::new("derived-unit")?;
    let forwarder = fixture.add_forwarder(PARENT_A, "derived-agent")?;
    let launch = fixture.launch(&forwarder, None, "derived-job", "completed")?;
    assert_wrapper_terminal(&fixture, &launch, "completed")?;

    ensure!(launch.authorization.unit_id == "fwd-derived-agent");
    let invocation = &launch.authorization.invocation_id;
    ensure!(invocation.len() == 36 && invocation.starts_with("inv-"));
    ensure!(invocation[4..]
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
    fixture.poll()?;
    ensure!(fixture.lifecycle_outcome(&forwarder)? == WorkerOutcome::Succeeded);
    assert_exit(&fixture.watch(&launch)?, 0);
    Ok(())
}

#[test]
#[serial]
fn guard_blocks_outside_active_stage() -> Result<()> {
    let fixture = Fixture::new("guard-outside")?;
    let forwarder = fixture.add_forwarder(PARENT_A, "outside-agent")?;
    let before = fixture.authorization_count()?;

    let output = fixture.guard_only(&forwarder, None, false)?;

    assert_exit(&output, 2);
    ensure!(
        fixture.authorization_count()? == before,
        "blocked guard wrote authorization"
    );
    Ok(())
}

#[test]
#[serial]
fn guard_rejects_identity_without_exact_start_row() -> Result<()> {
    let fixture = Fixture::new("guard-start-join")?;
    let forwarder = fixture.add_forwarder(PARENT_A, "started-agent")?;
    fs::remove_file(
        fixture
            .work
            .join("subagents")
            .join(crate::fixture::STAGE)
            .join("starts.jsonl"),
    )?;

    let output = fixture.guard_only(&forwarder, None, true)?;

    assert_exit(&output, 2);
    ensure!(
        fixture.authorization_count()? == 0,
        "unstarted identity was authorized"
    );
    Ok(())
}
