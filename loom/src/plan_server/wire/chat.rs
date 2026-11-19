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
    /// End a running kickoff and keep its draft. `now` tears the session down at once instead
    /// of waiting for its closing write.
    SetAsideKickoff {
        client_id: Option<String>,
        session: String,
        now: bool,
    },
    /// Start a new kickoff that carries an unfinished draft.
    ContinueKickoff {
        client_id: Option<String>,
        session: String,
    },
    /// Delete an unfinished draft. `delete_files` also removes the files its confirmed kickoff
    /// wrote.
    DiscardKickoff {
        client_id: Option<String>,
        session: String,
        delete_files: bool,
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
    /// "Allow this session" on a `WebFetch` card: allow it and the session's later fetches.
    AllowSessionFetches {
        client_id: Option<String>,
        request_id: String,
    },
    /// `"Always allow <host>"` on a `WebFetch` card: list its host in Settings and allow it.
    AllowFetchHost {
        client_id: Option<String>,
        request_id: String,
    },
    /// "Ask again": the session's later fetches show a card again.
    AskFetchesAgain {
        client_id: Option<String>,
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

    #[test]
    fn set_aside_kickoff_round_trips_with_its_now_flag() {
        let command = ChatCommand::SetAsideKickoff {
            client_id: Some("set-aside-1".into()),
            session: "11111111-1111-4111-8111-111111111111".into(),
            now: true,
        };
        let expected = json!({
            "command": "set_aside_kickoff", "client_id": "set-aside-1",
            "session": "11111111-1111-4111-8111-111111111111", "now": true
        });

        assert_eq!(serde_json::to_value(&command).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<ChatCommand>(expected).unwrap(),
            command
        );
    }

    #[test]
    fn set_aside_kickoff_requires_the_now_flag() {
        assert!(serde_json::from_value::<ChatCommand>(json!({
            "command": "set_aside_kickoff", "session": "kickoff-1"
        }))
        .is_err());
    }

    #[test]
    fn discard_kickoff_round_trips_with_its_delete_files_flag() {
        let command = ChatCommand::DiscardKickoff {
            client_id: None,
            session: "11111111-1111-4111-8111-111111111111".into(),
            delete_files: false,
        };
        let expected = json!({
            "command": "discard_kickoff", "client_id": null,
            "session": "11111111-1111-4111-8111-111111111111", "delete_files": false
        });

        assert_eq!(serde_json::to_value(&command).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<ChatCommand>(expected).unwrap(),
            command
        );
    }

    #[test]
    fn discard_kickoff_requires_the_delete_files_flag() {
        assert!(serde_json::from_value::<ChatCommand>(json!({
            "command": "discard_kickoff", "session": "kickoff-1"
        }))
        .is_err());
    }
}
