//! Refusals for server-initiated requests.

pub use super::wire::ServerReply;
use serde_json::json;

pub fn reply_for(method: &str) -> ServerReply {
    match method {
        "item/commandExecution/requestApproval" => {
            ServerReply::Result(json!({"decision": "decline"}))
        }
        "item/fileChange/requestApproval" => ServerReply::Result(json!({"decision": "decline"})),
        "item/permissions/requestApproval" => ServerReply::Result(json!({
            "permissions": {"fileSystem": null, "network": null}, "scope": "turn"
        })),
        "mcpServer/elicitation/request" => ServerReply::Result(json!({"action": "decline"})),
        "item/tool/call" => ServerReply::Result(json!({"contentItems": [], "success": false})),
        "applyPatchApproval" => ServerReply::Result(json!({"decision": "denied"})),
        "execCommandApproval" => ServerReply::Result(json!({"decision": "denied"})),
        "item/tool/requestUserInput" => ServerReply::Error {
            code: -32000,
            message: "loom native worker accepts no interactive input",
        },
        "account/chatgptAuthTokens/refresh" => ServerReply::Error {
            code: -32000,
            message: "loom native worker cannot refresh credentials",
        },
        _ => ServerReply::Error {
            code: -32601,
            message: "method not supported by loom native worker",
        },
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
