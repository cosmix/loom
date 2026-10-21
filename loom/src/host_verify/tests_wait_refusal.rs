//! A session call that fails still prints `Host job: refused (...)` as its
//! first line (the failure's message after it) and exits 1, and a call that
//! failed never shows a relay line.

use std::time::Duration;

use anyhow::{anyhow, Result};

use super::tests_wait::fixture;
use super::wait::{exit_code, wait_or_request, TicketSink, WaitOutcome, WaitSite};
use crate::relay::emit::RelaySink;
use crate::relay::HostJobRequest;

fn refused(outcome: &WaitOutcome) -> &str {
    match outcome {
        WaitOutcome::Refused(message) => message,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_checkout_that_cannot_be_read_prints_the_refusal_first_line() {
    let fx = fixture();
    let missing = fx.checkout.join("not-here");
    let site = WaitSite {
        checkout: &missing,
        ..fx.site(Duration::ZERO, false)
    };
    let mut relay = |_: &HostJobRequest| -> Result<String> { unreachable!("nothing to request") };
    let mut out = Vec::new();

    let outcome = wait_or_request(&site, &mut relay, &mut out).unwrap();

    let text = String::from_utf8(out).unwrap();
    assert_eq!(exit_code(&outcome), 1);
    assert!(text.starts_with("Host job: refused (check, "), "{text}");
    assert!(text.contains(refused(&outcome)), "{text}");
}

#[test]
fn a_failed_emit_is_refused_and_leaves_no_request_state() {
    let fx = fixture();
    let mut relay =
        |_: &HostJobRequest| -> Result<String> { Err(anyhow!("the scratch directory is full")) };
    let mut out = Vec::new();

    let outcome = wait_or_request(&fx.site(Duration::ZERO, false), &mut relay, &mut out).unwrap();

    let text = String::from_utf8(out).unwrap();
    assert_eq!(exit_code(&outcome), 1);
    assert!(text.starts_with("Host job: refused (check, "), "{text}");
    assert!(!text.contains("requested"), "{text}");
    assert!(
        text.contains("the request was not sent: the scratch directory is full"),
        "{text}"
    );
    assert!(!fx.state_file().exists());
}

#[test]
fn a_state_that_cannot_be_written_is_refused() {
    let fx = fixture();
    let blocker = fx.checkout.with_file_name("blocker");
    std::fs::write(&blocker, "a file where the scratch directory should be").unwrap();
    let site = WaitSite {
        scratch_dir: &blocker,
        ..fx.site(Duration::ZERO, false)
    };
    let mut relay = |_: &HostJobRequest| -> Result<String> { Ok("r1".to_string()) };
    let mut out = Vec::new();

    let outcome = wait_or_request(&site, &mut relay, &mut out).unwrap();

    let text = String::from_utf8(out).unwrap();
    assert_eq!(exit_code(&outcome), 1);
    assert!(text.starts_with("Host job: refused (check, "), "{text}");
    assert!(!text.contains("requested"), "{text}");
    assert!(text.contains("the request was not sent"), "{text}");
}

#[test]
fn a_ticket_sink_holds_the_relay_line_until_it_is_printed() {
    let mut sink = TicketSink::default();
    writeln!(sink.stdout(), "LOOM_RELAY_V1 id=r1").unwrap();
    let mut out = Vec::new();

    sink.print_line(&mut out).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "LOOM_RELAY_V1 id=r1\n");

    let mut again = Vec::new();
    sink.print_line(&mut again).unwrap();
    assert!(again.is_empty(), "the line prints once");
}
