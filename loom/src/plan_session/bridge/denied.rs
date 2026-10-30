//! Notices for actions blocked by auto mode.

use serde_json::Value;

use crate::plan_session::input::sanitize;

pub(super) fn denied_notice(payload: &Value) -> String {
    let Some(tool) = payload.get("tool_name").and_then(Value::as_str) else {
        return "Auto mode blocked an action".to_owned();
    };
    let tool = sanitize(tool);
    match payload.get("reason").and_then(Value::as_str) {
        Some(reason) => format!("Auto mode blocked {tool}: {}", sanitize(reason)),
        None => format!("Auto mode blocked {tool}"),
    }
}

#[cfg(test)]
mod tests {
    use super::denied_notice;
    use serde_json::json;

    #[test]
    fn documented_payload_names_the_blocked_tool_and_reason() {
        let payload = json!({
            "session_id": "abc123",
            "transcript_path": "/x.jsonl",
            "cwd": "/x",
            "permission_mode": "auto",
            "hook_event_name": "PermissionDenied",
            "tool_name": "Bash",
            "tool_input": {
                "command": "rm -rf /tmp/build",
                "description": "Clean build directory"
            },
            "tool_use_id": "toolu_01ABC",
            "reason": "[Irreversible Local Destruction]"
        });

        assert_eq!(
            denied_notice(&payload),
            "Auto mode blocked Bash: [Irreversible Local Destruction]"
        );
    }

    #[test]
    fn missing_reason_names_only_the_tool() {
        assert_eq!(
            denied_notice(&json!({"tool_name": "Bash"})),
            "Auto mode blocked Bash"
        );
    }

    #[test]
    fn non_string_reason_names_only_the_tool() {
        for reason in [json!(null), json!(42), json!(true), json!([]), json!({})] {
            assert_eq!(
                denied_notice(&json!({"tool_name": "Bash", "reason": reason})),
                "Auto mode blocked Bash"
            );
        }
    }

    #[test]
    fn missing_tool_uses_the_generic_notice_even_with_a_reason() {
        assert_eq!(
            denied_notice(&json!({"reason": "blocked"})),
            "Auto mode blocked an action"
        );
    }

    #[test]
    fn non_string_tool_uses_the_generic_notice_even_with_a_reason() {
        for tool in [json!(null), json!(42), json!(true), json!([]), json!({})] {
            assert_eq!(
                denied_notice(&json!({"tool_name": tool, "reason": "blocked"})),
                "Auto mode blocked an action"
            );
        }
    }

    #[test]
    fn missing_both_fields_uses_the_generic_notice() {
        assert_eq!(denied_notice(&json!({})), "Auto mode blocked an action");
    }

    #[test]
    fn reason_uses_the_shared_control_character_sanitiser() {
        let payload = json!({
            "tool_name": "Bash",
            "reason": "blocked\u{0000}\u{001b}\u{007f}\u{0085}\r\nwith\ttext\r"
        });

        assert_eq!(
            denied_notice(&payload),
            "Auto mode blocked Bash: blocked\nwith\ttext"
        );
    }

    #[test]
    fn tool_name_uses_the_shared_control_character_sanitiser() {
        assert_eq!(
            denied_notice(&json!({"tool_name": "B\u{0000}a\u{001b}s\u{007f}h\u{0085}"})),
            "Auto mode blocked Bash"
        );
    }
}
