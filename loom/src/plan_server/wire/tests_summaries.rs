//! Parity tests for summary wire types and their web fixtures.

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use super::summaries::{SummaryPanel, SummaryQuestion, SummaryTurnCost};
use crate::planning::summary::cost::estimate::EstimateBasis;

const PANEL: &str = include_str!("../../../../web/src/plan/api/fixtures/summary-panel.json");
const QUESTION: &str = include_str!("../../../../web/src/plan/api/fixtures/summary-question.json");
const TURN_COST: &str =
    include_str!("../../../../web/src/plan/api/fixtures/summary-turn-cost.json");

fn assert_fixture<T: DeserializeOwned + Serialize>(fixture: &str) {
    let expected: Value = serde_json::from_str(fixture).unwrap();
    let parsed: T = serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), expected);
}

#[test]
fn panel_fixture_matches_serde_output() {
    assert_fixture::<SummaryPanel>(PANEL);
}

#[test]
fn panel_fixture_deserializes() {
    let panel: SummaryPanel = serde_json::from_str(PANEL).unwrap();
    let running = panel.running.unwrap();
    assert_eq!((running.done, running.total, running.ids.len()), (1, 3, 3));
    assert_eq!(panel.terminal_claims, ["plans/loom-rebuilds"]);
    assert!(panel.last.unwrap().tokens.cache_write.is_none());
}

#[test]
fn absent_options_serialize_as_null_and_absent_tokens_are_omitted() {
    let panel: SummaryPanel = serde_json::from_str(PANEL).unwrap();
    let panel = SummaryPanel {
        running: None,
        last: None,
        ..panel
    };
    let value = serde_json::to_value(panel).unwrap();
    for field in ["running", "question", "last", "read_only", "start_error"] {
        assert_eq!(value[field], Value::Null, "{field}");
    }
    let turn: SummaryTurnCost = serde_json::from_str(TURN_COST).unwrap();
    let rows = serde_json::to_value(turn).unwrap();
    assert!(rows["rows"][1].get("tokens").is_none());
}

#[test]
fn question_fixture_matches_serde_output() {
    assert_fixture::<SummaryQuestion>(QUESTION);
}

#[test]
fn question_fixture_deserializes() {
    let question: SummaryQuestion = serde_json::from_str(QUESTION).unwrap();
    assert_eq!(question.id, "rebuild-q1");
    assert_eq!((question.count, question.ids.len()), (6, 6));
    assert_eq!(question.basis, EstimateBasis::Last);
}

#[test]
fn turn_cost_fixture_matches_serde_output() {
    assert_fixture::<SummaryTurnCost>(TURN_COST);
}

#[test]
fn turn_cost_fixture_deserializes() {
    let cost: SummaryTurnCost = serde_json::from_str(TURN_COST).unwrap();
    assert_eq!(cost.turn, "turn-004");
    assert_eq!(cost.rows.len(), 3);
    assert_eq!(cost.rows[1].tokens, None);
}
