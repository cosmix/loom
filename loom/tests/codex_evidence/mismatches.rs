use anyhow::Result;
use serde_json::json;
use serial_test::serial;
use std::fs;

use crate::assertions::{assert_unknown_after_poll, assert_wrapper_terminal};
use crate::fixture::{Fixture, LOOM_SESSION, STAGE};

const PARENT: &str = "dddddddd-4444-4444-8444-444444444444";
const AGENT: &str = "forwarder-mismatch";

#[test]
#[serial]
fn wrong_worktree_is_unknown() -> Result<()> {
    let fixture = Fixture::new("wrong-worktree")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(
        &forwarder,
        Some("unit-worktree"),
        "job-worktree",
        "completed",
    )?;
    assert_wrapper_terminal(&fixture, &launch, "completed")?;
    let foreign = fixture.root.join("foreign-worktree");
    fs::create_dir(&foreign)?;
    let foreign = fs::canonicalize(foreign)?;
    fixture.edit_job(&launch, |job| job["workspaceRoot"] = json!(foreign))?;

    assert_unknown_after_poll(&fixture, &launch, &forwarder)
}

#[test]
#[serial]
fn wrong_loom_session_is_unknown() -> Result<()> {
    let fixture = Fixture::new("wrong-session")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(&forwarder, Some("unit-session"), "job-session", "completed")?;
    assert_wrapper_terminal(&fixture, &launch, "completed")?;
    let invocation = &launch.authorization.invocation_id;
    let wrong = format!("loom.v1:{STAGE}:wrong-session:unit-session:{invocation}");
    fixture.edit_job(&launch, |job| job["sessionId"] = json!(wrong))?;

    assert_unknown_after_poll(&fixture, &launch, &forwarder)
}

#[test]
#[serial]
fn wrong_unit_is_unknown() -> Result<()> {
    let fixture = Fixture::new("wrong-unit")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(&forwarder, Some("unit-right"), "job-unit", "completed")?;
    assert_wrapper_terminal(&fixture, &launch, "completed")?;
    let invocation = &launch.authorization.invocation_id;
    let wrong = format!("loom.v1:{STAGE}:{LOOM_SESSION}:unit-wrong:{invocation}");
    fixture.edit_job(&launch, |job| job["sessionId"] = json!(wrong))?;

    assert_unknown_after_poll(&fixture, &launch, &forwarder)
}

#[test]
#[serial]
fn wrong_requested_model_is_unknown() -> Result<()> {
    let fixture = Fixture::new("wrong-model")?;
    let forwarder = fixture.add_forwarder(PARENT, AGENT)?;
    let launch = fixture.launch(&forwarder, Some("unit-model"), "job-model", "completed")?;
    assert_wrapper_terminal(&fixture, &launch, "completed")?;
    fixture.edit_job(&launch, |job| job["request"]["model"] = json!("gpt-wrong"))?;

    assert_unknown_after_poll(&fixture, &launch, &forwarder)
}
