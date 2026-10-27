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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
}

impl PlanMessage {
    pub fn to_frame(&self) -> String {
        serde_json::to_string(self)
            .expect("planning messages contain only JSON-serializable values")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    /// Change the browser's display name.
    SetName { name: String },
    /// Report the document currently being read.
    Reading { path: Option<String> },
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
    fn unknown_fields_are_rejected() {
        assert!(serde_json::from_value::<ClientMessage>(
            json!({"type": "set_name", "name": "Ada", "extra": true})
        )
        .is_err());
    }
}
