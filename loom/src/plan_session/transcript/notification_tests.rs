//! The notification parser's edge cases over hand-built bodies, and the evidence each record
//! shape of the probe fixtures gives.

use serde_json::{json, Value};

use super::*;

const SHELL: &str = "<task-notification>\n<task-id>b1</task-id>\n<tool-use-id>toolu_1</tool-use-id>\n\
    <status>completed</status>\n<summary>Background command \"x\" completed (exit code 0)</summary>\n\
    </task-notification>";

#[test]
fn a_body_without_a_status_or_a_task_id_is_not_parsed() {
    let unstated = SHELL.replace("<status>completed</status>\n", "");
    assert_eq!(parse_task_notification(&unstated), None);
    let anonymous = SHELL.replace("<task-id>b1</task-id>\n", "");
    assert_eq!(parse_task_notification(&anonymous), None);
    let empty = SHELL.replace("<task-id>b1</task-id>", "<task-id></task-id>");
    assert_eq!(parse_task_notification(&empty), None);
    assert_eq!(
        parse_task_notification("<task-id>b1</task-id><status>failed</status>"),
        None
    );
}

#[test]
fn an_unknown_status_is_kept_as_other() {
    let body = SHELL.replace("<status>completed</status>", "<status> lost </status>");
    let parsed = parse_task_notification(&body).unwrap();
    assert_eq!(parsed.status, NotificationStatus::Other("lost".to_owned()));
    assert!(!parsed.interim);
}

#[test]
fn a_body_without_a_tool_use_id_or_a_summary_keeps_the_rest() {
    let body = "<task-notification>\n<task-id><AGENT-1></task-id>\n<status>killed</status>\n\
        </task-notification>";
    let parsed = parse_task_notification(body).unwrap();
    let expected = TaskNotification {
        task_id: "<AGENT-1>".to_owned(),
        tool_use_id: None,
        status: NotificationStatus::Killed,
        summary: None,
        interim: false,
    };
    assert_eq!(parsed, expected);
}

#[test]
fn an_overlong_task_id_and_a_huge_summary_are_bounded() {
    let long_id = "b".repeat(200);
    let body = SHELL.replace(
        "<task-id>b1</task-id>",
        &format!("<task-id>{long_id}</task-id>"),
    );
    assert_eq!(parse_task_notification(&body), None);
    let summary = format!(
        "{} is waiting on its own background work",
        "é".repeat(2_000)
    );
    let body = SHELL.replace("Background command \"x\" completed (exit code 0)", &summary);
    let parsed = parse_task_notification(&body).unwrap();
    assert_eq!(parsed.summary.unwrap().chars().count(), LINE_CHARS);
    assert!(parsed.interim, "the interim ending is read before the cap");
}

fn user(content: Value, extra: Value) -> Value {
    let mut record = json!({"type": "user", "message": {"role": "user", "content": content}});
    for (key, value) in extra.as_object().unwrap() {
        record[key] = value.clone();
    }
    record
}

fn result(tool_use: &str, tool_use_result: Value) -> Value {
    let block = json!([{"type": "tool_result", "tool_use_id": tool_use, "content": "ok"}]);
    user(block, json!({ "toolUseResult": tool_use_result }))
}

#[test]
fn launch_and_task_stop_results_give_their_evidence() {
    let agent = json!({"isAsync": true, "status": "async_launched", "agentId": "a1"});
    let expected = BackgroundEvidence::AgentLaunch {
        agent_id: "a1".to_owned(),
        tool_use_id: Some("toolu_a".to_owned()),
        label: None,
    };
    assert_eq!(evidence(&result("toolu_a", agent)), Some(expected));
    let shell = json!({"backgroundTaskId": "bmiyho9ji", "stdout": ""});
    let launched = evidence(&result("toolu_b", shell));
    assert!(
        matches!(&launched, Some(BackgroundEvidence::ShellLaunch { task_id, tool_use_id, .. })
            if task_id == "bmiyho9ji" && tool_use_id.as_deref() == Some("toolu_b")),
        "{launched:?}"
    );
    let stop = json!({"message": "Successfully stopped task: bx0h460uz (sleep 60)",
        "task_id": "bx0h460uz", "task_type": "local_bash", "command": "sleep 60"});
    let stopped = BackgroundEvidence::TaskStopped {
        task_id: "bx0h460uz".to_owned(),
    };
    assert_eq!(evidence(&result("toolu_c", stop)), Some(stopped));
    let foreground = json!({"stdout": "done", "interrupted": false});
    assert_eq!(evidence(&result("toolu_d", foreground)), None);
    assert_eq!(
        evidence(&result("toolu_e", json!("User rejected tool use"))),
        None
    );
}

#[test]
fn a_hand_back_and_a_fork_launch_give_their_evidence() {
    let origin = json!({"origin": {"kind": "peer", "from": "a1", "senderTaskId": "a1",
        "handback": true, "handbackReportText": "  DONE\n"}, "isMeta": true});
    let handback = BackgroundEvidence::Handback {
        agent_id: "a1".to_owned(),
        report: Some("DONE".to_owned()),
    };
    assert_eq!(
        evidence(&user(json!("<agent-message>"), origin)),
        Some(handback)
    );
    let teammate = json!({"origin": {"kind": "peer", "from": "a1", "senderTaskId": "a1"}});
    assert_eq!(evidence(&user(json!("<teammate-message>"), teammate)), None);
    let launch = r#"{"agentId":"a1","skillName":"probe-fork","description":"/probe-fork"}"#;
    let content = format!("<local-command-stdout>Running</local-command-stdout>\n<forked-skill-launch>{launch}</forked-skill-launch>");
    let record = json!({"type": "system", "subtype": "local_command", "content": content});
    let fork = BackgroundEvidence::ForkLaunch {
        agent_id: "a1".to_owned(),
        label: Some("/probe-fork".to_owned()),
    };
    assert_eq!(evidence(&record), Some(fork));
    let broken = json!({"type": "system", "subtype": "local_command",
        "content": "<forked-skill-launch>{\"agentId\":</forked-skill-launch>"});
    assert_eq!(evidence(&broken), None);
}

#[test]
fn a_notification_record_gives_its_body_and_a_human_record_gives_nothing() {
    let origin = json!({"origin": {"kind": "task-notification", "producer": "session-task"}});
    let found = evidence(&user(json!(SHELL), origin));
    let body = parse_task_notification(SHELL).map(BackgroundEvidence::Notification);
    assert_eq!(found, body);
    let human = json!({"origin": {"kind": "human"}});
    assert_eq!(evidence(&user(json!("hello"), human)), None);
}

#[test]
fn a_launch_takes_its_calls_description_once() {
    let unlabelled = BackgroundEvidence::ShellLaunch {
        task_id: "b1".to_owned(),
        tool_use_id: Some("toolu_b".to_owned()),
        label: None,
    };
    let labelled = unlabelled.labelled(|call| (call == "toolu_b").then(|| "Run it".to_owned()));
    let expected = BackgroundEvidence::ShellLaunch {
        task_id: "b1".to_owned(),
        tool_use_id: Some("toolu_b".to_owned()),
        label: Some("Run it".to_owned()),
    };
    assert_eq!(labelled, expected);
    let input = json!({"description": "  Survey  ", "prompt": "x"});
    assert_eq!(launch_label("Agent", &input).as_deref(), Some("Survey"));
    assert_eq!(launch_label("Read", &input), None);
    assert_eq!(launch_label("Bash", &json!({"command": "ls"})), None);
}
