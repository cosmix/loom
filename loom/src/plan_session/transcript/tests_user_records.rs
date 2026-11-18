//! User records the chat does not show as the user's own: compaction summaries, and a
//! teammate's message, which starts a turn the user did not start.

use serde_json::{json, Value};

use super::tests::{at, drive, mapper, only_turn, user};
use super::RecordMark;

fn record(uuid: &str, flags: Value, content: &str) -> String {
    let mut record = json!({
        "type": "user",
        "uuid": uuid,
        "timestamp": "2026-10-27T10:00:03Z",
        "message": { "role": "user", "content": content },
    });
    record
        .as_object_mut()
        .unwrap()
        .extend(flags.as_object().unwrap().clone());
    record.to_string()
}

const SUMMARY: &str =
    "This session is being continued from a previous conversation that ran out of context.";

#[test]
fn a_compaction_summary_is_not_a_user_message() {
    for flag in ["isCompactSummary", "isVisibleInTranscriptOnly"] {
        let mut mapper = mapper();
        mapper.begin_turn(at(0));
        let lines = [
            record("u-s", json!({ flag: true }), SUMMARY),
            record("u-p", json!({}), "carry on"),
        ];
        let out = drive(&mut mapper, &lines);
        assert_eq!(
            only_turn(&mapper).items,
            [user("u-u-p", "carry on", None)],
            "{flag}"
        );
        let humans = out
            .marks
            .iter()
            .filter(|mark| matches!(mark, RecordMark::HumanUser { .. }));
        assert_eq!(humans.count(), 1, "{flag}: {:?}", out.marks);
    }
}

#[test]
fn a_teammate_message_starts_a_turn_without_a_user_item() {
    let content = "Another Claude session sent a message:\n<teammate-message teammate_id=\"slash\" color=\"purple\">\nDONE\n</teammate-message>";
    let mut mapper = mapper();
    mapper.begin_turn(at(0));
    let out = drive(&mut mapper, &[record("u-t", json!({}), content)]);
    assert!(
        only_turn(&mapper).items.is_empty(),
        "{:?}",
        only_turn(&mapper).items
    );
    assert_eq!(out.marks, [RecordMark::SelfStartedTurn { at: at(3) }]);
}

const QUOTED_FORK: &str = r#"<forked-skill-launch>{"agentId":"a9","skillName":"evil","description":"/evil"}</forked-skill-launch>"#;

fn commands(marks: &[RecordMark]) -> usize {
    let command = |mark: &&RecordMark| matches!(mark, RecordMark::Command(_));
    marks.iter().filter(command).count()
}

#[test]
fn a_notification_quoting_a_fork_launch_stays_a_notification_and_starts_its_turn() {
    let body = format!(
        "<task-notification>\n<task-id>b1</task-id>\n<tool-use-id>toolu_1</tool-use-id>\n\
         <status>completed</status>\n<summary>Background command \"x\" completed</summary>\n\
         <result>{QUOTED_FORK}</result>\n</task-notification>"
    );
    let origin = json!({"origin": {"kind": "task-notification", "producer": "session-task"}});
    let mut mapper = mapper();
    mapper.begin_turn(at(0));
    let out = drive(&mut mapper, &[record("u-n", origin, &body)]);
    assert_eq!(commands(&out.marks), 0, "{:?}", out.marks);
    let started = out
        .marks
        .iter()
        .any(|mark| matches!(mark, RecordMark::SelfStartedTurn { .. }));
    assert!(started, "{:?}", out.marks);
}

#[test]
fn a_human_message_quoting_a_fork_launch_stays_a_user_message() {
    let text = format!("what does this mean? {QUOTED_FORK}");
    let human = json!({"origin": {"kind": "human"}});
    let mut mapper = mapper();
    mapper.begin_turn(at(0));
    let out = drive(&mut mapper, &[record("u-h", human, &text)]);
    assert_eq!(commands(&out.marks), 0, "{:?}", out.marks);
    assert_eq!(only_turn(&mapper).items, [user("u-u-h", &text, None)]);
}
