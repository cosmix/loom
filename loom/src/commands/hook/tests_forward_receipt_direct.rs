use anyhow::Result;
use serde_json::json;

use super::{direct_channel, now, process, states, Fixture, AGENT, THREAD, TOOL};
use crate::models::forward_receipt::{ForwardObservation, ForwardState};

#[test]
#[serial_test::serial]
fn direct_requires_finished_end_marker_for_success() -> Result<()> {
    let completed = Fixture::new(AGENT)?;
    let raw = completed.payload(TOOL, Some((direct_channel(THREAD, true), None)));
    process(&raw, &completed.config(), now())?;
    assert_eq!(
        states(&completed.observations()?),
        vec![ForwardState::Running, ForwardState::Succeeded]
    );

    // START, then the separator with no END: the wrapper failed closed after
    // announcing the thread, so the forward finished without a verdict.
    let printed = Fixture::new(AGENT)?;
    let raw = printed.payload(TOOL, Some((direct_channel(THREAD, false), None)));
    process(&raw, &printed.config(), now())?;
    assert_unknown_with_thread(&printed.observations()?);

    let unknown = Fixture::new(AGENT)?;
    let channel = direct_channel(THREAD, true).replace(
        "\"succeeded\",\"exit_code\":0",
        "\"unknown\",\"exit_code\":125",
    );
    process(
        &unknown.payload(TOOL, Some((channel, None))),
        &unknown.config(),
        now(),
    )?;
    assert_unknown_with_thread(&unknown.observations()?);

    // START alone (a backgrounded forward still running) stays running.
    let streaming = Fixture::new(AGENT)?;
    let channel = format!(
        "LOOM-FORWARD-START {{\"v\":1,\"backend\":\"direct\",\"thread_id\":\"{THREAD}\"}}\n"
    );
    process(
        &streaming.payload(TOOL, Some((channel, None))),
        &streaming.config(),
        now(),
    )?;
    assert_eq!(
        states(&streaming.observations()?),
        vec![ForwardState::Running]
    );
    Ok(())
}

fn assert_unknown_with_thread(observations: &[ForwardObservation]) {
    assert_eq!(
        states(observations),
        vec![ForwardState::Running, ForwardState::Unknown]
    );
    assert!(observations
        .iter()
        .all(|value| value.codex_thread_id.as_deref() == Some(THREAD)));
    assert_eq!(observations[1].exit_code, None);
}

#[test]
#[serial_test::serial]
fn structured_stdout_takes_precedence_over_tool_result_text() -> Result<()> {
    let fixture = Fixture::new(AGENT)?;
    let stdout = direct_channel(THREAD, false);
    let fallback = direct_channel(THREAD, true);
    let raw = fixture.payload(TOOL, Some((stdout, Some(json!({"output":fallback})))));
    process(&raw, &fixture.config(), now())?;

    assert_eq!(
        states(&fixture.observations()?),
        vec![ForwardState::Running, ForwardState::Unknown]
    );
    Ok(())
}
