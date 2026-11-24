//! Contract: a transcript rewritten in place between two scans is counted
//! from its new content, also when the rewrite keeps the file's length.

use super::tests_fixture::{body, claude_line, stage, unix_now, usage, Fixture, SESSION_UUID};
use std::fs;

const OPUS: &str = "claude-opus-5-5";

fn line(msg_id: &str, ts: i64, cache_write: u64) -> String {
    claude_line(msg_id, ts, OPUS, &usage(0, cache_write, 0, 0))
}

#[test]
fn a_same_length_rewrite_is_counted_from_the_new_content() {
    let fx = Fixture::new();
    let now = unix_now();
    let first = body(&[line("m1", now - 90, 100), line("m2", now - 80, 100)]);
    let transcript = fx.add_session("alpha", SESSION_UUID, &first);
    fx.scan(now);

    let rewrite = body(&[line("m1", now - 90, 200), line("m2", now - 80, 300)]);
    assert_eq!(
        first.len(),
        rewrite.len(),
        "the scenario is a rewrite of equal length"
    );
    fs::write(&transcript, &rewrite).expect("rewrite in place");

    let flow = fx.scan(now);

    assert_eq!(
        stage(&flow, "alpha").total.cache_write,
        500,
        "the rewritten lines replace the ones counted before"
    );
}
