//! Contract: a task notification's outcome comes from its `<status>` element, never from the
//! summary wording, and the parser keeps the task id, the tool-use id and the interim variant.

use serde_json::Value;

use super::notification::{parse_task_notification, NotificationStatus, TaskNotification};

const STATUS: &str =
    include_str!("../../../tests/fixtures/plan_session/transcripts/p1-status.jsonl");
const BASH: &str =
    include_str!("../../../tests/fixtures/plan_session/transcripts/p1-background-bash.jsonl");
const INTERRUPT: &str =
    include_str!("../../../tests/fixtures/plan_session/transcripts/p1-interrupt.jsonl");

/// The `message.content` of every task-notification user record, in transcript order.
fn bodies(transcript: &str) -> Vec<String> {
    let records = transcript.lines().filter_map(|line| {
        let record: Value = serde_json::from_str(line).ok()?;
        let notified = record["type"] == "user" && record["origin"]["kind"] == "task-notification";
        notified.then(|| record["message"]["content"].as_str().map(str::to_owned))?
    });
    records.collect()
}

fn parsed(body: &str) -> TaskNotification {
    parse_task_notification(body).unwrap_or_else(|| panic!("not parsed: {body}"))
}

fn word(status: &NotificationStatus) -> String {
    match status {
        NotificationStatus::Completed => "completed".to_owned(),
        NotificationStatus::Failed => "failed".to_owned(),
        NotificationStatus::Killed => "killed".to_owned(),
        NotificationStatus::Other(other) => format!("other: {other}"),
    }
}

fn check(n: &TaskNotification, task: &str, tool_use: &str, status: &str, interim: bool) {
    assert_eq!(n.task_id, task);
    assert_eq!(n.tool_use_id.as_deref(), Some(tool_use), "task {task}");
    assert_eq!(
        word(&n.status),
        status,
        "task {task}, summary {:?}",
        n.summary
    );
    assert_eq!(n.interim, interim, "task {task}, summary {:?}", n.summary);
}

#[test]
fn notification_status_comes_from_the_status_element() {
    let status = bodies(STATUS);
    assert_eq!(status.len(), 2, "p1-status notifications");
    let failed = parsed(&status[0]);
    let exit = failed.summary.as_deref().unwrap_or_default();
    assert!(exit.contains("failed with exit code 7"), "{exit:?}");
    check(
        &failed,
        "badtfi3uv",
        "toolu_01RsWxunPyoVUUXBkZUVm5QN",
        "failed",
        false,
    );
    let killed = parsed(&status[1]);
    check(
        &killed,
        "<AGENT-1>",
        "toolu_01XXov5nUzt6Z3rFUnDS7Bfw",
        "killed",
        false,
    );

    let bash = bodies(BASH);
    assert_eq!(bash.len(), 1, "p1-background-bash notifications");
    let shell = "toolu_01DfQmxWFkRxjV3MGY5sKeAx";
    check(&parsed(&bash[0]), "bmiyho9ji", shell, "completed", false);
    // The same body with only its <status> changed: the summary still says it completed.
    let relabelled = bash[0].replace("<status>completed</status>", "<status>failed</status>");
    check(&parsed(&relabelled), "bmiyho9ji", shell, "failed", false);

    let interrupt = bodies(INTERRUPT);
    assert_eq!(interrupt.len(), 1, "p1-interrupt notifications");
    let waiting = parsed(&interrupt[0]);
    let summary = waiting.summary.as_deref().unwrap_or_default();
    assert!(
        summary.ends_with("is waiting on its own background work"),
        "{summary:?}"
    );
    check(
        &waiting,
        "<AGENT-1>",
        "toolu_01KoWA6n7ZZEw6Z3s5sNV1NJ",
        "completed",
        true,
    );
}
