//! Bridge tests: the start guard on a missing claude binary and the notice a spooled
//! `PermissionDenied` event turns into.

use std::path::PathBuf;

use serde_json::json;

use super::fake_host::*;
use super::BridgeError;
use crate::plan_session::kind::SessionKind::Authoring;
use crate::plan_session::model::Item;

#[test]
fn a_start_without_a_claude_binary_is_refused_before_any_record_exists() {
    let h = Harness::ready(|c| c.claude_bin = PathBuf::new());

    let refused = h.bridge.start_session(h.request(Authoring, OPENING, false));

    assert_eq!(
        refused,
        Err(BridgeError::Refused(
            "no claude binary is configured".to_owned()
        ))
    );
    assert!(h.bridge.sessions().is_empty(), "a session was reserved");
    let records = std::fs::read_dir(&h.runtime.sessions)
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(records, 0, "a session directory was created");
}

fn notice_texts(h: &Harness) -> Vec<String> {
    let turns = h.bridge.chat_snapshot(true).turns;
    let items = turns.into_iter().flat_map(|turn| turn.items);
    items
        .filter_map(|item| match item {
            Item::Notice { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn a_spooled_permission_denied_event_becomes_one_notice() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);

    h.spool(
        &s,
        "PermissionDenied",
        json!({
            "cwd": "/x",
            "permission_mode": "auto",
            "tool_name": "Bash",
            "tool_input": {
                "command": "rm -rf /tmp/build",
                "description": "Clean build directory"
            },
            "tool_use_id": "toolu_01ABC",
            "reason": "[Irreversible Local Destruction]"
        }),
    );

    let blocked = "Auto mode blocked Bash: [Irreversible Local Destruction]";
    wait_until("the denied notice", || {
        notice_texts(&h).iter().any(|text| text == blocked)
    });
    assert_eq!(notice_texts(&h), [blocked]);
}
