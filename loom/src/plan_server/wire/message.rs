//! Messages exchanged with planning clients.

use serde::{Deserialize, Serialize};

use super::checklist::{ChecklistView, OfferView, ProjectChecklistView, StandalonePlanView};
use super::concept::ConceptChanged;
use super::files::{DocChanged, FileTree, ValidationChanged};
use super::kickoff::{KickoffDraftView, KickoffView};
use super::presence::{AuthoringView, PresenceList};
use super::pressure::PressureRunView;
use super::run::{RunOutput, RunStateView, RunStrip};
use super::server::{Refused, ServerInfo, You};
use super::summaries::{SummaryPanel, SummaryQuestion, SummaryTurnCost};
use super::team::DraftTeamView;

/// Initial planning snapshot for one connected browser.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    /// The current browser's public identity.
    pub you: You,
    /// Server availability and connection details.
    pub server: ServerInfo,
    /// One checklist per feature, sorted by feature name.
    pub checklists: Vec<ChecklistView>,
    /// Standalone plans, sorted by slug.
    pub standalone: Vec<StandalonePlanView>,
    /// Offers to move misplaced documents into features.
    pub moves: Vec<OfferView>,
    /// Connected browsers and their reading activity.
    pub presence: PresenceList,
    /// The current authoring holder, if any.
    pub authoring: AuthoringView,
}

// `PartialEq` only: the chat model carries no `Eq` types.
/// A server-to-client planning frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanMessage {
    /// Initial snapshot for a newly connected client.
    Hello(Box<Hello>),
    /// Connected people and their activity.
    Presence(PresenceList),
    /// Current authoring holder.
    Authoring(AuthoringView),
    /// Current feature checklists, standalone plans and move offers.
    Checklist(ProjectChecklistView),
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
    /// Current summary rebuild progress, costs and quota.
    SummaryPanel(Box<SummaryPanel>),
    /// Approval requested before a large summary rebuild.
    SummaryQuestion(SummaryQuestion),
    /// A summary rebuild approval question was closed.
    SummaryQuestionClosed {
        id: String,
    },
    /// Summary rebuild costs attributed to an authoring turn.
    SummaryTurnCost(SummaryTurnCost),
    /// A chat snapshot, delta, command result or returned queue.
    Chat(Box<crate::plan_server::wire::chat::ChatFrame>),
    Kickoff(Box<KickoffView>),
    DraftTeam(Box<DraftTeamView>),
    /// Every unfinished kickoff, newest first; sent on hello and after every change.
    KickoffDrafts {
        drafts: Vec<KickoffDraftView>,
    },
    /// A review suggestion, status, open-question count or refusal.
    Review(crate::plan_server::wire::review::ReviewMessage),
    /// Current pressure run progress.
    PressureRun(PressureRunView),
    /// The pressure options changed (a run ended, or a Codex configuration block was
    /// confirmed): browsers fetch them again.
    PressureOptionsChanged,
    /// Current execution handoff state.
    RunState(RunStateView),
    /// Output from an execution handoff job.
    RunOutput(RunOutput),
    /// Execution progress displayed across browsers.
    RunStrip(RunStrip),
    /// A feature's planning trace changed: browsers fetch it again.
    TraceChanged {
        feature: String,
    },
}

impl PlanMessage {
    /// Serialize this message into one WebSocket text frame.
    pub fn to_frame(&self) -> String {
        serde_json::to_string(self)
            .expect("planning messages contain only JSON-serializable values")
    }
}

/// A command sent by a planning browser.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    /// Change the browser's display name.
    SetName { name: String },
    /// Report the document currently being read.
    Reading { path: Option<String> },
    /// A chat command.
    Chat(crate::plan_server::wire::chat::ChatCommand),
    /// A review suggestion or answer.
    Review(crate::plan_server::wire::review::ReviewCommand),
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
        let fixture = include_str!("fixtures/hello.json");
        assert_fixture::<PlanMessage>(fixture);
        let message: PlanMessage = serde_json::from_str(fixture).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&message.to_frame()).unwrap(),
            serde_json::from_str::<Value>(fixture).unwrap()
        );
    }

    #[test]
    fn checklist_message_fixture_contains_the_complete_planning_snapshot() {
        let fixture = include_str!("fixtures/checklist-message.json");
        assert_fixture::<PlanMessage>(fixture);
        let message: PlanMessage = serde_json::from_str(fixture).unwrap();
        let PlanMessage::Checklist(snapshot) = message else {
            panic!("expected the existing checklist message type");
        };
        assert_eq!(snapshot.checklists[0].feature, "demo");
        assert_eq!(snapshot.standalone[0].slug, "example");
        assert_eq!(
            snapshot.moves[0].kind,
            crate::plan_server::wire::checklist::OfferKind::MoveIntoFeature
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
                PlanMessage::PressureOptionsChanged,
                json!({"type": "pressure_options_changed"}),
            ),
            (
                PlanMessage::SummaryQuestionClosed {
                    id: "rebuild-q1".into(),
                },
                json!({"type": "summary_question_closed", "id": "rebuild-q1"}),
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
    fn summary_message_fixtures_round_trip_with_their_tags() {
        let cases = [
            (
                "summary_panel",
                include_str!("../../../../web/src/plan/api/fixtures/summary-panel.json"),
            ),
            (
                "summary_question",
                include_str!("../../../../web/src/plan/api/fixtures/summary-question.json"),
            ),
            (
                "summary_turn_cost",
                include_str!("../../../../web/src/plan/api/fixtures/summary-turn-cost.json"),
            ),
        ];
        for (tag, fixture) in cases {
            let mut expected: Value = serde_json::from_str(fixture).unwrap();
            expected["type"] = json!(tag);
            let message: PlanMessage = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(serde_json::to_value(&message).unwrap(), expected);
            assert_eq!(
                serde_json::from_str::<Value>(&message.to_frame()).unwrap(),
                expected
            );
        }
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
    fn kickoff_message_round_trips_with_its_tag() {
        use super::super::kickoff::KickoffShape;
        use crate::plan_session::scope::KickoffSize;

        let message = PlanMessage::Kickoff(Box::new(KickoffView {
            session: "kickoff-1".into(),
            shape: KickoffShape {
                size: Some(KickoffSize::Feature),
                name: Some("demo".into()),
                ..Default::default()
            },
            target: Some("doc/product/demo/index.md".into()),
        }));
        let expected = json!({
            "type": "kickoff", "session": "kickoff-1",
            "shape": {"size": "feature", "name": "demo", "proposal": false, "problem": "",
                "decisions": [], "questions": [], "sections": []},
            "target": "doc/product/demo/index.md"
        });

        assert_eq!(serde_json::to_value(&message).unwrap(), expected);
        assert_eq!(
            serde_json::from_str::<Value>(&message.to_frame()).unwrap(),
            expected
        );
        assert_eq!(
            serde_json::from_value::<PlanMessage>(expected).unwrap(),
            message
        );
    }

    #[test]
    fn draft_team_message_round_trips_with_its_tag() {
        let message = PlanMessage::DraftTeam(Box::new(DraftTeamView {
            session: "team-1".into(),
            documents: vec!["doc/product/demo/section_01.md".into()],
            teammates: vec![],
        }));
        let expected = json!({
            "type": "draft_team", "session": "team-1",
            "documents": ["doc/product/demo/section_01.md"], "teammates": []
        });

        assert_eq!(serde_json::to_value(&message).unwrap(), expected);
        assert_eq!(
            serde_json::from_str::<Value>(&message.to_frame()).unwrap(),
            expected
        );
        assert_eq!(
            serde_json::from_value::<PlanMessage>(expected).unwrap(),
            message
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
