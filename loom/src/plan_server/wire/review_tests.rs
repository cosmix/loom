use super::review::*;
use serde_json::{json, Value};

fn passage() -> SuggestedPassageWire {
    SuggestedPassageWire {
        path: "doc/product/example.md".into(),
        anchor: "overview".into(),
        text: "A passage".into(),
        lines: Some((1, 2)),
        stage: None,
        summary_sentence: None,
    }
}

#[test]
fn every_review_command_kind_round_trips() {
    let cases = [
        (
            ReviewCommand::Suggest {
                passage: passage(),
                note: "note".into(),
            },
            "suggest",
        ),
        (
            ReviewCommand::SuggestionAnswer {
                id: "sg-1".into(),
                action: SuggestionActionWire::Attach,
            },
            "suggestion_answer",
        ),
    ];
    for (command, kind) in cases {
        let value = serde_json::to_value(&command).unwrap();
        assert_eq!(value["kind"], kind);
        assert_eq!(
            serde_json::from_value::<ReviewCommand>(value).unwrap(),
            command
        );
    }
}

#[test]
fn every_review_message_kind_round_trips() {
    let cases = [
        (
            ReviewMessage::Suggestion {
                id: "sg-1".into(),
                from: "Ada".into(),
                passage: passage(),
                note: "note".into(),
            },
            "suggestion",
        ),
        (
            ReviewMessage::SuggestionStatus {
                id: "sg-1".into(),
                status: SuggestionStatus::Attached,
            },
            "suggestion_status",
        ),
        (
            ReviewMessage::OpenQuestions {
                counts: Default::default(),
            },
            "open_questions",
        ),
        (
            ReviewMessage::Refused {
                reason: "no holder".into(),
            },
            "refused",
        ),
    ];
    for (message, kind) in cases {
        let value = serde_json::to_value(&message).unwrap();
        assert_eq!(value["kind"], kind);
        assert_eq!(
            serde_json::from_value::<ReviewMessage>(value).unwrap(),
            message
        );
    }
}

#[test]
fn suggestion_status_states_serialize_snake_case() {
    let cases = [
        (
            SuggestionStatus::Sent {
                holder: "Ada".into(),
                away: true,
            },
            json!({"state": "sent", "holder": "Ada", "away": true}),
        ),
        (SuggestionStatus::Attached, json!({"state": "attached"})),
        (SuggestionStatus::Dismissed, json!({"state": "dismissed"})),
        (
            SuggestionStatus::Dropped {
                reason: "authoring changed hands".into(),
            },
            json!({"state": "dropped", "reason": "authoring changed hands"}),
        ),
    ];
    for (status, expected) in cases {
        let message = ReviewMessage::SuggestionStatus {
            id: "sg-1".into(),
            status,
        };
        let mut expected: Value = expected;
        expected["kind"] = json!("suggestion_status");
        expected["id"] = json!("sg-1");
        assert_eq!(serde_json::to_value(&message).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<ReviewMessage>(expected).unwrap(),
            message
        );
    }
}
