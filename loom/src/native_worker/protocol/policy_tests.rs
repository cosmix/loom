use super::{reply_for, ServerReply};
use serde_json::json;

#[test]
fn command_execution_approval_is_declined() {
    assert_eq!(
        reply_for("item/commandExecution/requestApproval"),
        ServerReply::Result(json!({"decision": "decline"}))
    );
}

#[test]
fn file_change_approval_is_declined() {
    assert_eq!(
        reply_for("item/fileChange/requestApproval"),
        ServerReply::Result(json!({"decision": "decline"}))
    );
}

#[test]
fn permissions_request_grants_nothing_for_the_turn() {
    assert_eq!(
        reply_for("item/permissions/requestApproval"),
        ServerReply::Result(
            json!({"permissions": {"fileSystem": null, "network": null}, "scope": "turn"})
        )
    );
}

#[test]
fn mcp_elicitation_is_declined() {
    assert_eq!(
        reply_for("mcpServer/elicitation/request"),
        ServerReply::Result(json!({"action": "decline"}))
    );
}

#[test]
fn dynamic_tool_call_fails_without_content() {
    assert_eq!(
        reply_for("item/tool/call"),
        ServerReply::Result(json!({"contentItems": [], "success": false}))
    );
}

#[test]
fn legacy_apply_patch_is_denied() {
    assert_eq!(
        reply_for("applyPatchApproval"),
        ServerReply::Result(json!({"decision": "denied"}))
    );
}

#[test]
fn legacy_exec_command_is_denied() {
    assert_eq!(
        reply_for("execCommandApproval"),
        ServerReply::Result(json!({"decision": "denied"}))
    );
}

#[test]
fn user_input_request_gets_error_32000() {
    assert_eq!(
        reply_for("item/tool/requestUserInput"),
        ServerReply::Error {
            code: -32000,
            message: "loom native worker accepts no interactive input"
        }
    );
}

#[test]
fn token_refresh_gets_error_32000() {
    assert_eq!(
        reply_for("account/chatgptAuthTokens/refresh"),
        ServerReply::Error {
            code: -32000,
            message: "loom native worker cannot refresh credentials"
        }
    );
}

#[test]
fn unknown_method_gets_error_32601() {
    assert_eq!(
        reply_for("unknown"),
        ServerReply::Error {
            code: -32601,
            message: "method not supported by loom native worker"
        }
    );
}

#[test]
fn empty_method_gets_error_32601() {
    assert_eq!(
        reply_for(""),
        ServerReply::Error {
            code: -32601,
            message: "method not supported by loom native worker"
        }
    );
}

#[test]
fn current_and_legacy_approvals_use_different_decision_words() {
    for method in [
        "item/commandExecution/requestApproval",
        "item/fileChange/requestApproval",
    ] {
        assert_eq!(
            reply_for(method),
            ServerReply::Result(json!({"decision": "decline"}))
        );
    }
    for method in ["applyPatchApproval", "execCommandApproval"] {
        assert_eq!(
            reply_for(method),
            ServerReply::Result(json!({"decision": "denied"}))
        );
    }
}
