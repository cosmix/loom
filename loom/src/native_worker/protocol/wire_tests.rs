use super::*;

#[test]
fn initialize_frame() {
    assert_eq!(
        initialize(1, "1.2"),
        json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"loom","title":"loom","version":"1.2"},"capabilities":{"optOutNotificationMethods":["thread/tokenUsage/updated","item/agentMessage/delta"]}}})
    );
}

#[test]
fn initialized_frame() {
    assert_eq!(initialized(), json!({"method":"initialized","params":{}}));
}

#[test]
fn thread_start_frame_workspace_write() {
    assert_eq!(
        thread_start(2, "model", Path::new("/work"), "instructions"),
        json!({"id":2,"method":"thread/start","params":{"model":"model","cwd":"/work","approvalPolicy":"never","sandbox":"workspace-write","developerInstructions":"instructions"}})
    );
}

#[test]
fn turn_start_frame_workspace_write_policy() {
    assert_eq!(
        turn_start(3, "t", &["hello".into()], "high", Path::new("/work")),
        json!({"id":3,"method":"turn/start","params":{"threadId":"t","input":[{"type":"text","text":"hello"}],"effort":"high","sandboxPolicy":{"type":"workspaceWrite","writableRoots":["/work"],"networkAccess":false,"excludeSlashTmp":true,"excludeTmpdirEnvVar":false}}})
    );
}

#[test]
fn turn_start_emits_one_text_item_per_text() {
    assert_eq!(
        turn_start(
            1,
            "t",
            &["one".into(), "two".into()],
            "medium",
            Path::new("/w")
        ),
        json!({"id":1,"method":"turn/start","params":{"threadId":"t","input":[{"type":"text","text":"one"},{"type":"text","text":"two"}],"effort":"medium","sandboxPolicy":{"type":"workspaceWrite","writableRoots":["/w"],"networkAccess":false,"excludeSlashTmp":true,"excludeTmpdirEnvVar":false}}})
    );
}

#[test]
fn turn_steer_frame() {
    assert_eq!(
        turn_steer(4, "t", "u", "hello"),
        json!({"id":4,"method":"turn/steer","params":{"threadId":"t","expectedTurnId":"u","input":[{"type":"text","text":"hello"}]}})
    );
}

#[test]
fn turn_interrupt_frame() {
    assert_eq!(
        turn_interrupt(5, "t", "u"),
        json!({"id":5,"method":"turn/interrupt","params":{"threadId":"t","turnId":"u"}})
    );
}

#[test]
fn server_reply_echoes_numeric_and_string_ids() {
    for id in [json!(42), json!("request")] {
        assert_eq!(
            server_reply(&id, &ServerReply::Result(json!({"ok":true}))),
            json!({"id":id,"result":{"ok":true}})
        );
    }
}

#[test]
fn server_reply_error_shape() {
    assert_eq!(
        server_reply(
            &json!("r"),
            &ServerReply::Error {
                code: -32601,
                message: "unsupported"
            }
        ),
        json!({"id":"r","error":{"code":-32601,"message":"unsupported"}})
    );
}

#[test]
fn thread_id_and_turn_id_extraction() {
    assert_eq!(
        thread_id_from_result(&json!({"thread":{"id":"t"}})),
        Some("t".into())
    );
    assert_eq!(
        turn_id_from_result(&json!({"turn":{"id":"u"}})),
        Some("u".into())
    );
}

#[test]
fn extraction_returns_none_when_missing() {
    assert_eq!(thread_id_from_result(&json!({})), None);
    assert_eq!(turn_id_from_result(&json!({"turn":{"id":1}})), None);
}

#[test]
fn parse_turn_started() {
    assert_eq!(
        parse_notification("turn/started", &json!({"threadId":"t","turn":{"id":"u"}})),
        Notification::TurnStarted {
            thread_id: "t".into(),
            turn_id: "u".into()
        }
    );
}

#[test]
fn parse_turn_completed_with_error_message() {
    assert_eq!(
        parse_notification(
            "turn/completed",
            &json!({"threadId":"t","turn":{"id":"u","status":"failed","error":{"message":"bad"}}})
        ),
        Notification::TurnCompleted {
            thread_id: "t".into(),
            turn_id: "u".into(),
            status: "failed".into(),
            error: Some("bad".into())
        }
    );
}

#[test]
fn parse_item_completed_agent_message_with_null_phase() {
    assert_eq!(
        parse_notification(
            "item/completed",
            &json!({"threadId":"t","turnId":"u","item":{"type":"agentMessage","id":"i","text":"answer","phase":null}})
        ),
        Notification::ItemCompleted {
            thread_id: "t".into(),
            turn_id: "u".into(),
            message: Some(AgentMessage {
                id: "i".into(),
                text: "answer".into(),
                phase: None
            })
        }
    );
}

#[test]
fn parse_item_completed_non_agent_message_has_no_message() {
    assert_eq!(
        parse_notification(
            "item/completed",
            &json!({"threadId":"t","turnId":"u","item":{"type":"commandExecution"}})
        ),
        Notification::ItemCompleted {
            thread_id: "t".into(),
            turn_id: "u".into(),
            message: None
        }
    );
}

#[test]
fn parse_error_notification() {
    assert_eq!(
        parse_notification(
            "error",
            &json!({"threadId":"t","error":{"message":"bad"},"willRetry":true})
        ),
        Notification::Error {
            thread_id: Some("t".into()),
            message: "bad".into(),
            will_retry: true
        }
    );
    assert_eq!(
        parse_notification("error", &json!({"error":{"message":"bad"}})),
        Notification::Error {
            thread_id: None,
            message: "bad".into(),
            will_retry: false
        }
    );
}

#[test]
fn parse_missing_fields_is_other() {
    for (method, value) in [
        ("turn/started", json!({"threadId":"t"})),
        ("turn/completed", json!({"threadId":"t","turn":{"id":"u"}})),
        ("item/completed", json!({"threadId":"t","turnId":"u"})),
        ("error", json!({})),
    ] {
        assert_eq!(parse_notification(method, &value), Notification::Other);
    }
    assert_eq!(
        parse_notification(
            "item/completed",
            &json!({"threadId":"t","turnId":"u","item":{"type":"agentMessage","id":"i"}})
        ),
        Notification::ItemCompleted {
            thread_id: "t".into(),
            turn_id: "u".into(),
            message: None
        }
    );
}

#[test]
fn parse_unknown_method_is_other() {
    assert_eq!(
        parse_notification("unknown", &json!({})),
        Notification::Other
    );
}

#[test]
fn developer_instructions_text_is_exact() {
    assert_eq!(DEVELOPER_INSTRUCTIONS,"You are a loom implementation worker. Do not run git. Do not read or write any path under .loom or .work. Do not ask questions; if the brief is impossible, say why in your final answer and stop.");
}
