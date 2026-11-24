//! Contract: a message revised after its agent's id table was shed to fit the
//! checkpoint cap replaces the old observation instead of adding to it.

use super::accumulate::MESSAGE_TABLE_CAP;
use super::model::TokenUsage;
use super::tests_fixture::{
    append, body, bulky_body, claude_line, stage, unix_now, usage, Fixture, SESSION_UUID,
};
use std::fs;
use std::path::Path;

const OPUS: &str = "claude-opus-5-5";

/// The first message of the main transcript's `bulky_body`, revised: the same
/// id one second after the original, with other usage.
fn revised_line(now: i64) -> String {
    let id = format!("main{0:0>1200}", 0);
    let text = claude_line(&id, now - 59, OPUS, &usage(0, 7, 0, 0));
    body(&[text])
}

/// A second fixture holding the same final bytes of both transcripts, read
/// whole by one scan.
fn fresh_total(main: &Path, sub: &Path, now: i64) -> TokenUsage {
    let fresh = Fixture::new();
    fresh.add_session(
        "alpha",
        SESSION_UUID,
        &fs::read_to_string(main).expect("main"),
    );
    let sub_text = fs::read_to_string(sub).expect("subagent");
    fresh.add_subagent(SESSION_UUID, "agent-a1.jsonl", &sub_text);
    stage(&fresh.scan(now), "alpha").total
}

#[test]
fn a_revised_message_after_shedding_is_counted_once() {
    let fx = Fixture::new();
    let now = unix_now();
    let main = fx.add_session("alpha", SESSION_UUID, &bulky_body("main", now));
    fx.scan(now);
    let sub = fx.add_subagent(SESSION_UUID, "agent-a1.jsonl", &bulky_body("sub", now));
    fx.scan(now);
    append(&main, &revised_line(now));

    let after_shed = stage(&fx.scan(now), "alpha").total;

    let expected = fresh_total(&main, &sub, now);
    assert_eq!(
        expected.cache_write,
        2 * MESSAGE_TABLE_CAP as u64 + 6,
        "the scenario: one line revised from 1 to 7 tokens"
    );
    assert_eq!(
        after_shed, expected,
        "the revision replaces the old observation"
    );
}
