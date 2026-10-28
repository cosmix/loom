//! Messages exchanged with planning clients.

use serde::{Deserialize, Serialize};

use super::checklist::ChecklistView;
use super::concept::ConceptChanged;
use super::files::{DocChanged, FileTree, ValidationChanged};
use super::presence::{AuthoringView, PresenceList};
use super::server::{Refused, ServerInfo, You};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub you: You,
    pub server: ServerInfo,
    pub checklist: ChecklistView,
    pub presence: PresenceList,
    pub authoring: AuthoringView,
}

// `PartialEq` only: the chat model carries no `Eq` types.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanMessage {
    /// Initial snapshot for a newly connected client.
    Hello(Box<Hello>),
    /// Connected people and their activity.
    Presence(PresenceList),
    /// Current authoring holder.
    Authoring(AuthoringView),
    /// Current planning checklist.
    Checklist(ChecklistView),
    /// The server is shutting down.
    ServerStopping,
    /// A command was refused.
    Refused(Refused),
    /// The document tree changed.
    TreeChanged(FileTree),
    /// A document changed on disk or was removed.
    DocChanged(DocChanged),
    /// A plan's validation changed.
    Validation(ValidationChanged),
    /// Concept pages or their sources changed.
    ConceptChanged(ConceptChanged),
    /// A chat snapshot, delta, command result or returned queue.
    Chat(Box<crate::plan_server::wire::chat::ChatFrame>),
}

impl PlanMessage {
    pub fn to_frame(&self) -> String {
        serde_json::to_string(self)
            .expect("planning messages contain only JSON-serializable values")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    /// Change the browser's display name.
    SetName { name: String },
    /// Report the document currently being read.
    Reading { path: Option<String> },
    /// A chat command.
    Chat(crate::plan_server::wire::chat::ChatCommand),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{de::DeserializeOwned, Serialize};
    use serde_json::{json, Value};

    fn assert_fixture<T: DeserializeOwned + Serialize>(fixture: &str) {
        let expected: Value = serde_json::from_str(fixture).unwrap();
        let message: T = serde_json::from_value(expected.clone()).unwrap();
        assert_eq!(serde_json::to_value(message).unwrap(), expected);
    }

    #[test]
    fn hello_fixture_matches_serde_output() {
        let fixture = include_str!("../../../../web/src/plan/api/fixtures/hello.json");
        assert_fixture::<PlanMessage>(fixture);
        let message: PlanMessage = serde_json::from_str(fixture).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&message.to_frame()).unwrap(),
            serde_json::from_str::<Value>(fixture).unwrap()
        );
    }

    #[test]
    fn client_set_name_fixture_matches_serde_output() {
        assert_fixture::<ClientMessage>(include_str!(
            "../../../../web/src/plan/api/fixtures/client-set-name.json"
        ));
    }

    #[test]
    fn client_reading_fixture_matches_serde_output() {
        assert_fixture::<ClientMessage>(include_str!(
            "../../../../web/src/plan/api/fixtures/client-reading.json"
        ));
    }

    #[test]
    fn message_tags_and_nullable_reading() {
        let cases = [
            (
                PlanMessage::Presence(PresenceList { entries: vec![] }),
                json!({"type": "presence", "entries": []}),
            ),
            (
                PlanMessage::Authoring(AuthoringView { holder: None }),
                json!({"type": "authoring", "holder": null}),
            ),
            (
                PlanMessage::ServerStopping,
                json!({"type": "server_stopping"}),
            ),
            (
                PlanMessage::Refused(Refused {
                    command: "save".into(),
                    reason: "read only".into(),
                }),
                json!({"type": "refused", "command": "save", "reason": "read only"}),
            ),
        ];
        for (message, expected) in cases {
            assert_eq!(serde_json::to_value(&message).unwrap(), expected);
            assert_eq!(
                serde_json::from_value::<PlanMessage>(expected).unwrap(),
                message
            );
        }
        let reading = ClientMessage::Reading { path: None };
        assert_eq!(
            serde_json::to_value(&reading).unwrap(),
            json!({"type": "reading", "path": null})
        );
        assert_eq!(
            serde_json::from_value::<ClientMessage>(json!({"type": "reading", "path": null}))
                .unwrap(),
            reading
        );
    }

    #[test]
    fn chat_frames_and_commands_nest_under_the_chat_type() {
        use crate::plan_server::wire::chat::{ChatCommand, ChatFrame, CommandResult};
        let result = PlanMessage::Chat(Box::new(ChatFrame::Result(CommandResult {
            client_id: Some("c1".into()),
            ok: false,
            error: Some("no chat is running".into()),
        })));
        let expected = json!({"type": "chat", "frame": "result", "client_id": "c1", "ok": false,
            "error": "no chat is running"});
        assert_eq!(serde_json::to_value(&result).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<PlanMessage>(expected).unwrap(),
            result
        );
        let command = json!({"type": "chat", "command": "interrupt", "client_id": null});
        assert_eq!(
            serde_json::from_value::<ClientMessage>(command).unwrap(),
            ClientMessage::Chat(ChatCommand::Interrupt { client_id: None })
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(serde_json::from_value::<ClientMessage>(
            json!({"type": "set_name", "name": "Ada", "extra": true})
        )
        .is_err());
    }
}
