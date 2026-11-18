//! Bridge tests: an agent id Claude Code starts again counts as a new run.

use std::time::{Duration, Instant};

use serde_json::json;

use super::fake_host::*;
use super::{EndCause, EndMode, EndOutcome};
use crate::plan_session::kind::SessionKind::Authoring;

#[test]
fn a_subagent_started_again_after_its_stop_still_lets_the_chat_close() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    // A background coordinator is woken by its children's notifications: each wake starts and
    // stops the same agent id.
    for event in [
        "SubagentStart",
        "SubagentStop",
        "SubagentStart",
        "SubagentStop",
    ] {
        h.spool(&s, event, json!({"agent_id": "a1"}));
    }
    let asked = Instant::now();
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    h.spool(&s, "Stop", json!({}));
    wait_until("the closing message", || h.sent(&s) == [CLOSING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": CLOSING}));
    h.spool(&s, "Stop", json!({}));
    wait_until("the end", || h.ended(&s.id).is_some());
    assert_eq!(h.ended(&s.id), Some(EndOutcome::Closed));
    assert!(asked.elapsed() < Duration::from_secs(10));
}
