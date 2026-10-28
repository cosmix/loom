//! Fixture parity tests for the review wire types.

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

use super::message::{ClientMessage, PlanMessage};
use super::review::{
    CompareResponse, DocChange, RestoreFile, RestoreFileResult, RestoreResponse, ReviewCommand,
    ReviewMessage, SuggestedPassageWire, SuggestionActionWire, SuggestionStatus,
};

const RESTORE: &str = include_str!("../../../../web/src/plan/api/fixtures/review-restore.json");
const COMPARE: &str = include_str!("../../../../web/src/plan/api/fixtures/review-compare.json");
const SUGGESTION: &str =
    include_str!("../../../../web/src/plan/api/fixtures/review-suggestion.json");

fn assert_parity<T: DeserializeOwned + Serialize>(fixture: &str) -> T {
    let expected: Value = serde_json::from_str(fixture).unwrap();
    let parsed: T = serde_json::from_str(fixture).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), expected);
    parsed
}

#[test]
fn restore_fixture_round_trips_all_four_results() {
    let response = assert_parity::<RestoreResponse>(RESTORE);

    assert!(matches!(
        response.files.as_slice(),
        [
            RestoreFile {
                result: RestoreFileResult::Restored,
                ..
            },
            RestoreFile {
                result: RestoreFileResult::Refused { .. },
                ..
            },
            RestoreFile {
                result: RestoreFileResult::Changed { .. },
                ..
            },
            RestoreFile {
                result: RestoreFileResult::Invalid { .. },
                ..
            },
        ]
    ));
}

#[test]
fn compare_fixture_round_trips_added_changed_and_removed_docs() {
    let response = assert_parity::<CompareResponse>(COMPARE);

    assert!(response.available);
    assert_eq!(
        response
            .docs
            .iter()
            .map(|doc| doc.change)
            .collect::<Vec<_>>(),
        vec![DocChange::Added, DocChange::Changed, DocChange::Removed]
    );
}

#[test]
fn suggestion_fixture_round_trips_suggestion_and_sent_status() {
    let messages = assert_parity::<Vec<ReviewMessage>>(SUGGESTION);
    let [ReviewMessage::Suggestion { id, .. }, ReviewMessage::SuggestionStatus {
        id: status_id,
        status: SuggestionStatus::Sent { holder, away },
    }] = messages.as_slice()
    else {
        panic!("fixture must pair a suggestion with its sent status");
    };

    assert_eq!(id, status_id);
    assert_eq!(holder, "Alex");
    assert!(*away);
}

#[test]
fn review_messages_round_trip_with_outer_type_and_inner_kind() {
    let messages: Vec<ReviewMessage> = serde_json::from_str(SUGGESTION).unwrap();
    let values: Vec<Value> = serde_json::from_str(SUGGESTION).unwrap();

    for (review, mut expected) in messages.into_iter().zip(values) {
        expected["type"] = json!("review");
        let message = PlanMessage::Review(review);

        assert_eq!(serde_json::to_value(&message).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<PlanMessage>(expected.clone()).unwrap(),
            message
        );
        assert_eq!(
            serde_json::from_str::<Value>(&message.to_frame()).unwrap(),
            expected
        );
    }
}

fn assert_command_envelope(command: ReviewCommand, expected: Value) {
    let message = ClientMessage::Review(command);
    assert_eq!(serde_json::to_value(&message).unwrap(), expected);
    assert_eq!(
        serde_json::from_value::<ClientMessage>(expected).unwrap(),
        message
    );
}

#[test]
fn suggest_command_round_trips_with_outer_type_and_inner_kind() {
    let command = ReviewCommand::Suggest {
        passage: SuggestedPassageWire {
            path: "doc/product/section_10.md".into(),
            anchor: "suggestions-and-restores".into(),
            text: "A viewer can send a passage to the authoring holder.".into(),
            lines: Some((24, 24)),
            stage: None,
            summary_sentence: None,
        },
        note: "Clarify what happens when the holder is away.".into(),
    };

    assert_command_envelope(
        command,
        json!({
            "type": "review",
            "kind": "suggest",
            "passage": {
                "path": "doc/product/section_10.md",
                "anchor": "suggestions-and-restores",
                "text": "A viewer can send a passage to the authoring holder.",
                "lines": [24, 24],
                "stage": null,
                "summary_sentence": null
            },
            "note": "Clarify what happens when the holder is away."
        }),
    );
}

#[test]
fn suggestion_answers_round_trip_with_outer_type_and_inner_kind() {
    for (action, wire_action) in [
        (SuggestionActionWire::Attach, "attach"),
        (SuggestionActionWire::Dismiss, "dismiss"),
    ] {
        assert_command_envelope(
            ReviewCommand::SuggestionAnswer {
                id: "sg-1".into(),
                action,
            },
            json!({
                "type": "review",
                "kind": "suggestion_answer",
                "id": "sg-1",
                "action": wire_action
            }),
        );
    }
}
