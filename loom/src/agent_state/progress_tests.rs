use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use super::{AgentProgress, SubagentState};

const DEBOUNCE: u64 = 180;

fn stamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339()
}

fn assistant(at: Option<DateTime<Utc>>, content: Value) -> Value {
    let mut row = json!({"type": "assistant", "message": {"content": content}});
    if let Some(at) = at {
        row["timestamp"] = json!(stamp(at));
    }
    row
}

fn observed(rows: &[Value]) -> AgentProgress {
    let mut progress = AgentProgress::default();
    rows.iter().for_each(|row| progress.observe(row));
    progress
}

#[test]
fn nothing_observed_is_unknown_with_no_idle_time() {
    let progress = AgentProgress::default();
    assert_eq!(progress.state(Utc::now(), DEBOUNCE), SubagentState::Unknown);
    assert_eq!(progress.idle_secs(Utc::now()), None);
    assert!(progress.last().is_none());
}

#[test]
fn an_entry_without_a_timestamp_clears_last_at_instead_of_keeping_an_earlier_one() {
    let now = Utc::now();
    let rows = [
        assistant(
            Some(now - Duration::seconds(500)),
            json!([{"type": "text", "text": "a"}]),
        ),
        assistant(None, json!([{"type": "text", "text": "b"}])),
    ];
    let progress = observed(&rows);
    assert_eq!(progress.last_at(), None);
    assert_eq!(progress.idle_secs(now), None);
    // No timestamp counts as 0 idle seconds, inside the debounce.
    assert_eq!(progress.state(now, DEBOUNCE), SubagentState::Generating);
}

#[test]
fn text_only_assistant_entry_is_generating_inside_the_debounce_and_done_past_it() {
    let now = Utc::now();
    let text = json!([{"type": "text", "text": "report"}]);
    let recent = observed(&[assistant(Some(now - Duration::seconds(10)), text.clone())]);
    assert_eq!(recent.state(now, DEBOUNCE), SubagentState::Generating);
    let old = observed(&[assistant(Some(now - Duration::seconds(300)), text)]);
    assert_eq!(old.state(now, DEBOUNCE), SubagentState::Done);
    assert_eq!(old.idle_secs(now), Some(300));
}

#[test]
fn a_tool_use_entry_is_tool_wait_and_names_the_last_tool() {
    let now = Utc::now();
    let rows = [assistant(
        Some(now),
        json!([
            {"type": "tool_use", "name": "Read", "id": "t1"},
            {"type": "tool_use", "name": "Grep", "id": "t2"},
        ]),
    )];
    let progress = observed(&rows);
    assert_eq!(progress.state(now, DEBOUNCE), SubagentState::ToolWait);
    assert_eq!(progress.last_tool(), Some("Grep"));
    assert_eq!(progress.assistant_entries(), 1);
}

#[test]
fn the_last_tool_survives_a_later_user_result_and_a_text_entry() {
    let now = Utc::now();
    let rows = [
        assistant(Some(now), json!([{"type": "tool_use", "name": "Bash"}])),
        json!({"type": "user", "timestamp": stamp(now), "message": {"content": []}}),
        assistant(Some(now), json!([{"type": "text", "text": "ok"}])),
    ];
    let progress = observed(&rows);
    assert_eq!(progress.last_tool(), Some("Bash"));
    assert_eq!(progress.assistant_entries(), 2);
}

#[test]
fn a_trailing_attachment_row_makes_the_last_entry_unknown() {
    let now = Utc::now();
    let rows = [
        assistant(Some(now), json!([{"type": "tool_use", "name": "Read"}])),
        json!({"type": "attachment", "timestamp": stamp(now)}),
    ];
    let progress = observed(&rows);
    assert_eq!(progress.state(now, DEBOUNCE), SubagentState::Unknown);
    assert_eq!(progress.assistant_entries(), 1);
}

#[test]
fn idle_time_is_never_negative() {
    let now = Utc::now();
    let progress = observed(&[assistant(
        Some(now + Duration::seconds(30)),
        json!([{"type": "text", "text": "x"}]),
    )]);
    assert_eq!(progress.idle_secs(now), Some(0));
}
