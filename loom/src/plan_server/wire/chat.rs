//! Chat frames and commands on the planning WebSocket.

use serde::{Deserialize, Serialize};

use crate::plan_session::attachments::AttachmentInput;
use crate::plan_session::model::{ChatDelta, ChatSnapshot};
use crate::plan_session::scope::KickoffSize;

/// A chat update or command result sent to a browser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
// The snapshot is sent once per connection; boxing it would change the pinned
// `ChatFrame::Snapshot(ChatSnapshot)` constructor the bridge uses.
#[allow(clippy::large_enum_variant)]
pub enum ChatFrame {
    Snapshot(ChatSnapshot),
    Delta(ChatDelta),
    Result(CommandResult),
    QueueReturned { texts: Vec<String> },
}

/// The outcome of a browser command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandResult {
    pub client_id: Option<String>,
    pub ok: bool,
    pub error: Option<String>,
}

/// A chat command sent by a browser. A misspelt field is refused: dropped, it would read as
/// absent (a deny note lost). The outer `type` tag never reaches it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChatCommand {
    Start {
        client_id: Option<String>,
        offer: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        feature: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        plan: Option<String>,
        text: Option<String>,
        attachments: Vec<AttachmentInput>,
    },
    Send {
        client_id: Option<String>,
        text: String,
        attachments: Vec<AttachmentInput>,
    },
    ConfirmKickoff {
        client_id: Option<String>,
        size: KickoffSize,
        name: String,
    },
    Withdraw {
        client_id: Option<String>,
        queue_id: String,
    },
    Permission {
        client_id: Option<String>,
        request_id: String,
        allow: bool,
        note: Option<String>,
    },
    Interrupt {
        client_id: Option<String>,
    },
    End {
        client_id: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn confirm_kickoff_round_trips_with_its_tag() {
        let command = ChatCommand::ConfirmKickoff {
            client_id: Some("confirm-1".into()),
            size: KickoffSize::Feature,
            name: "demo".into(),
        };
        let expected = json!({
            "command": "confirm_kickoff", "client_id": "confirm-1",
            "size": "feature", "name": "demo"
        });

        assert_eq!(serde_json::to_value(&command).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<ChatCommand>(expected).unwrap(),
            command
        );
    }

    #[test]
    fn confirm_kickoff_allows_an_omitted_client_id() {
        let command = serde_json::from_value::<ChatCommand>(json!({
            "command": "confirm_kickoff", "size": "plan", "name": "demo"
        }))
        .unwrap();

        assert_eq!(
            command,
            ChatCommand::ConfirmKickoff {
                client_id: None,
                size: KickoffSize::Plan,
                name: "demo".into(),
            }
        );
    }

    #[test]
    fn confirm_kickoff_rejects_unknown_fields() {
        assert!(serde_json::from_value::<ChatCommand>(json!({
            "command": "confirm_kickoff", "size": "feature", "name": "demo", "extra": true
        }))
        .is_err());
    }

    #[test]
    fn confirm_kickoff_requires_size_and_name() {
        for invalid in [
            json!({"command": "confirm_kickoff", "name": "demo"}),
            json!({"command": "confirm_kickoff", "size": "feature"}),
            json!({"command": "confirm_kickoff", "size": "Feature", "name": "demo"}),
        ] {
            assert!(serde_json::from_value::<ChatCommand>(invalid).is_err());
        }
    }
}
