use serde_json::{json, Value};

use super::{TeamTracker, TeammateState, TeammateStatus};
use crate::plan_session::model::TokenCounts;
use crate::tokens::tail::MAX_LINE_BYTES;

fn tracker() -> TeamTracker {
    TeamTracker::new("/repo".into())
}

fn assistant(id: &str, stop: Option<&str>, blocks: Vec<Value>, usage: [u64; 4]) -> String {
    json!({
        "type": "assistant",
        "message": {
            "id": id,
            "stop_reason": stop,
            "content": blocks,
            "usage": {
                "input_tokens": usage[0],
                "cache_creation_input_tokens": usage[1],
                "cache_read_input_tokens": usage[2],
                "output_tokens": usage[3],
            },
        },
    })
    .to_string()
}

fn tool(name: &str, path: &str) -> Value {
    json!({"type": "tool_use", "name": name, "input": {"file_path": path}})
}

fn status(tracker: &TeamTracker) -> TeammateStatus {
    let statuses = tracker.statuses();
    assert_eq!(statuses.len(), 1);
    statuses.into_iter().next().unwrap()
}

#[test]
fn tokens_count_distinct_messages_with_the_latest_usage_winning() {
    let mut tracker = tracker();
    let lines = vec![
        assistant("a", None, vec![], [10, 20, 30, 40]),
        assistant("a", None, vec![], [50, 60, 70, 80]),
        assistant("b", None, vec![], [1, 2, 3, 4]),
    ];

    assert!(tracker.ingest("agent", "Writer", &lines));
    assert_eq!(
        status(&tracker).tokens,
        TokenCounts {
            input: 51,
            cache_write: 62,
            cache_read: 73,
            output: 84,
        }
    );
    assert!(!tracker.ingest("agent", "Writer", &lines));

    assert!(tracker.ingest(
        "agent",
        "Writer",
        &[assistant("a", None, vec![], [2, 3, 4, 5]),]
    ));
    assert_eq!(
        status(&tracker).tokens,
        TokenCounts {
            input: 3,
            cache_write: 5,
            cache_read: 7,
            output: 9,
        }
    );
}

#[test]
fn usage_parsing_preserves_cache_normalization_and_rejects_invalid_updates() {
    let mut tracker = tracker();
    let valid = json!({"type": "assistant", "message": {
        "id": "a", "usage": {"cache_creation": {
            "ephemeral_5m_input_tokens": 2, "ephemeral_1h_input_tokens": 3,
        }},
    }})
    .to_string();
    let invalid = json!({"type": "assistant", "message": {
        "id": "a", "usage": {"input_tokens": -1, "output_tokens": 100},
    }})
    .to_string();

    assert!(tracker.ingest("agent", "Writer", &[valid]));
    assert!(!tracker.ingest("agent", "Writer", &[invalid]));
    assert_eq!(
        status(&tracker).tokens,
        TokenCounts {
            cache_write: 5,
            ..TokenCounts::default()
        }
    );
}

#[test]
fn message_ids_are_deduplicated_separately_for_each_teammate() {
    let mut tracker = tracker();
    let line = assistant("shared", None, vec![], [1, 2, 3, 4]);

    assert!(tracker.ingest("a", "A", std::slice::from_ref(&line)));
    assert!(tracker.ingest("b", "B", &[line]));

    let statuses = tracker.statuses();
    assert_eq!(
        statuses[0].tokens,
        TokenCounts {
            input: 1,
            cache_write: 2,
            cache_read: 3,
            output: 4,
        }
    );
    assert_eq!(statuses[0].tokens, statuses[1].tokens);
}

#[test]
fn documents_filter_paths_and_preserve_first_seen_order_without_duplicates() {
    let mut tracker = tracker();
    let blocks = vec![
        tool("Write", "/repo/doc/product/feature/index.md"),
        tool("Edit", "/elsewhere/doc/outside.md"),
        tool("Write", "/repo-other/doc/outside.md"),
        tool("Write", "/repo/src/main.rs"),
        tool("Write", "doc-other/outside.md"),
        tool("Edit", "doc/product/feature/section.md"),
        tool("MultiEdit", "/repo/doc/plans/PLAN-example.md"),
        tool("Read", "/repo/doc/product/read-only.md"),
        tool("Write", "doc/../outside.md"),
        tool("Write", "../elsewhere/doc/outside.md"),
        tool("Write", "doc"),
        tool("Edit", "doc/product/feature/./index.md"),
        json!({"type": "text", "name": "Write",
            "input": {"file_path": "doc/fake.md"}}),
        json!({"type": "tool_use", "name": "Write", "input": {"file_path": 3}}),
    ];
    let lines = vec![assistant("a", Some("tool_use"), blocks, [0; 4])];

    assert!(tracker.ingest("agent", "Writer", &lines));

    let visible = status(&tracker);
    assert_eq!(
        visible.documents,
        [
            "doc/product/feature/index.md",
            "doc/product/feature/section.md",
            "doc/plans/PLAN-example.md",
        ]
    );
    assert_eq!(
        visible.current.as_deref(),
        Some("doc/plans/PLAN-example.md")
    );
    assert_eq!(visible.state, TeammateState::Working);
    assert!(!tracker.ingest("agent", "Writer", &lines));
}

#[test]
fn idle_clears_current_and_later_user_or_tool_result_records_resume_work() {
    let mut tracker = tracker();
    let writing = assistant(
        "a",
        Some("tool_use"),
        vec![tool("Write", "doc/plans/PLAN-example.md")],
        [0; 4],
    );
    let done = assistant("b", Some("end_turn"), vec![], [0; 4]);
    assert!(tracker.ingest("agent", "Writer", &[writing]));
    assert!(tracker.ingest("agent", "Writer", std::slice::from_ref(&done)));
    assert_eq!(status(&tracker).state, TeammateState::Idle);
    assert_eq!(status(&tracker).current, None);
    assert_eq!(status(&tracker).documents, ["doc/plans/PLAN-example.md"]);

    let user = json!({"type": "user", "message": {"content": "Continue"}}).to_string();
    assert!(tracker.ingest("agent", "Writer", &[user]));
    assert_eq!(status(&tracker).state, TeammateState::Working);
    assert_eq!(
        status(&tracker).current.as_deref(),
        Some("doc/plans/PLAN-example.md")
    );

    assert!(tracker.ingest("agent", "Writer", &[done]));
    let result = json!({"type": "tool_result", "content": "Done"}).to_string();
    assert!(tracker.ingest("agent", "Writer", &[result]));
    assert_eq!(status(&tracker).state, TeammateState::Working);
}

#[test]
fn last_record_in_a_batch_determines_state() {
    let mut tracker = tracker();
    let done = assistant("a", Some("end_turn"), vec![], [0; 4]);
    let tool_result = json!({"type": "user", "message": {"content": [
        {"type": "tool_result", "tool_use_id": "tool", "content": "Done"},
    ]}})
    .to_string();

    assert!(tracker.ingest("agent", "Writer", &[done.clone(), tool_result]));
    assert_eq!(status(&tracker).state, TeammateState::Working);
    assert_eq!(status(&tracker).current, None);
    assert!(tracker.ingest("agent", "Writer", &[done]));
    assert_eq!(status(&tracker).state, TeammateState::Idle);
    let thinking = assistant("b", None, vec![], [0; 4]);
    assert!(tracker.ingest("agent", "Writer", &[thinking]));
    assert_eq!(status(&tracker).state, TeammateState::Working);
}

#[test]
fn new_teammates_and_nonempty_name_changes_are_visible() {
    let mut tracker = tracker();

    assert!(tracker.ingest("agent", "", &[]));
    assert_eq!(
        status(&tracker),
        TeammateStatus {
            agent_id: "agent".into(),
            name: String::new(),
            documents: vec![],
            current: None,
            state: TeammateState::Working,
            tokens: TokenCounts::default(),
        }
    );
    assert!(!tracker.ingest("agent", "", &[]));
    assert!(tracker.ingest("agent", "Writer", &[]));
    assert_eq!(status(&tracker).name, "Writer");
    assert!(!tracker.ingest("agent", "", &[]));
    assert!(!tracker.ingest("agent", "Writer", &[]));
    assert!(tracker.ingest("agent", "Reviewer", &[]));
    assert_eq!(status(&tracker).name, "Reviewer");
}

#[test]
fn no_visible_change_is_reported_when_a_batch_only_changes_internal_usage_ids() {
    let mut tracker = tracker();
    assert!(tracker.ingest("agent", "Writer", &[]));

    let lines = vec![
        assistant("a", Some("end_turn"), vec![], [0; 4]),
        assistant("b", None, vec![], [0; 4]),
    ];
    assert!(!tracker.ingest("agent", "Writer", &lines));
    assert!(!tracker.ingest("agent", "Writer", &lines));
}

#[test]
fn garbage_missing_types_and_oversized_records_are_skipped() {
    let mut tracker = tracker();
    let done = assistant("a", Some("end_turn"), vec![], [1, 2, 3, 4]);
    tracker.ingest("agent", "Writer", &[done]);
    let before = tracker.statuses();
    let garbage = vec![
        "not json".into(),
        "{".into(),
        json!(null).to_string(),
        json!([]).to_string(),
        json!({"message": {"id": "missing-type", "usage": {"input_tokens": 99},
            "content": [tool("Write", "doc/ignored.md")]}})
        .to_string(),
        json!({"type": null}).to_string(),
        json!({"type": 3}).to_string(),
        json!({"type": "user", "payload": "x".repeat(MAX_LINE_BYTES)}).to_string(),
    ];

    assert!(!tracker.ingest("agent", "Writer", &garbage));
    assert_eq!(tracker.statuses(), before);
}

#[test]
fn statuses_sort_by_name_then_agent_id_and_resort_after_renaming() {
    let mut tracker = tracker();
    tracker.ingest("c", "Zoe", &[]);
    tracker.ingest("b", "Amy", &[]);
    tracker.ingest("a", "Amy", &[]);

    let ids = |tracker: &TeamTracker| {
        tracker
            .statuses()
            .into_iter()
            .map(|status| status.agent_id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&tracker), ["a", "b", "c"]);
    assert!(tracker.ingest("c", "Aaron", &[]));
    assert_eq!(ids(&tracker), ["c", "a", "b"]);
}

#[test]
fn real_teammate_fixture_counts_latest_message_usage_and_finishes_idle() {
    let mut tracker = tracker();
    let lines: Vec<String> = include_str!("../../plan_server/pressure/fixtures/teammate.jsonl")
        .lines()
        .map(str::to_owned)
        .collect();

    assert!(tracker.ingest("areviewer-4f2a9c1d7e3b5a60", "reviewer", &lines));

    assert_eq!(
        status(&tracker),
        TeammateStatus {
            agent_id: "areviewer-4f2a9c1d7e3b5a60".into(),
            name: "reviewer".into(),
            documents: vec![],
            current: None,
            state: TeammateState::Idle,
            tokens: TokenCounts {
                input: 8,
                cache_write: 27438,
                cache_read: 71550,
                output: 244,
            },
        }
    );
    assert!(!tracker.ingest("areviewer-4f2a9c1d7e3b5a60", "reviewer", &lines));
}
