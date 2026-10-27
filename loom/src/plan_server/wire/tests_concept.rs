//! Parity tests for the concept wire types and their web fixtures.

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

use super::concept::{ConceptChanged, ConceptPage, EmptyKind, OutOfDateReason, SummaryStateView};
use super::message::PlanMessage;

const OVERVIEW: &str = include_str!("../../../../web/src/plan/api/fixtures/concept-overview.json");
const SUMMARY: &str = include_str!("../../../../web/src/plan/api/fixtures/concept-summary.json");

fn assert_fixture<T: DeserializeOwned + Serialize>(fixture: &str) -> T {
    let expected: Value = serde_json::from_str(fixture).unwrap();
    let parsed: T = serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), expected);
    parsed
}

#[test]
fn overview_fixture_matches_serde_output() {
    assert_fixture::<ConceptPage>(OVERVIEW);
}

#[test]
fn overview_fixture_deserializes() {
    let page: ConceptPage = serde_json::from_str(OVERVIEW).unwrap();
    assert_eq!(page.key_chips.len(), 3);
    assert_eq!(
        page.key_chips[0].out_of_date.as_ref().unwrap().reason,
        OutOfDateReason::SourceChanged
    );
    assert_eq!(page.blocks.len(), 3);
}

#[test]
fn summary_fixture_matches_serde_output() {
    assert_fixture::<ConceptPage>(SUMMARY);
}

#[test]
fn summary_fixture_deserializes() {
    let page: ConceptPage = serde_json::from_str(SUMMARY).unwrap();
    assert_eq!(page.key, "product/section_01");
    assert_eq!(page.implemented_by.len(), 1);
    assert_eq!(page.state.unwrap().state, SummaryStateView::Stale);
}

#[test]
fn concept_changed_message_round_trips() {
    let message = PlanMessage::ConceptChanged(ConceptChanged {
        pages: vec!["overview".into()],
        out_of_date: vec!["product/section_01".into()],
        sources: vec!["doc/product/vision.md".into()],
    });
    let expected = json!({
        "type": "concept_changed",
        "pages": ["overview"],
        "out_of_date": ["product/section_01"],
        "sources": ["doc/product/vision.md"],
    });
    assert_eq!(serde_json::to_value(&message).unwrap(), expected);
    assert_eq!(
        serde_json::from_value::<PlanMessage>(expected).unwrap(),
        message
    );
}

fn wire<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn summary_state_serializes_as_snake_case_strings() {
    // Exhaustive matches fail to compile when a variant is added without a new case here.
    match SummaryStateView::Current {
        SummaryStateView::Current
        | SummaryStateView::Incomplete
        | SummaryStateView::Stale
        | SummaryStateView::Rebuilding
        | SummaryStateView::Missing => {}
    }
    let states = [
        SummaryStateView::Current,
        SummaryStateView::Incomplete,
        SummaryStateView::Stale,
        SummaryStateView::Rebuilding,
        SummaryStateView::Missing,
    ]
    .map(wire);
    assert_eq!(
        states,
        ["current", "incomplete", "stale", "rebuilding", "missing"]
    );
}

#[test]
fn out_of_date_reason_serializes_as_snake_case_strings() {
    // Exhaustive matches fail to compile when a variant is added without a new case here.
    match OutOfDateReason::SourceChanged {
        OutOfDateReason::SourceChanged
        | OutOfDateReason::Rebuilding
        | OutOfDateReason::AwaitingApproval
        | OutOfDateReason::CannotRebuild
        | OutOfDateReason::Failed => {}
    }
    let reasons = [
        OutOfDateReason::SourceChanged,
        OutOfDateReason::Rebuilding,
        OutOfDateReason::AwaitingApproval,
        OutOfDateReason::CannotRebuild,
        OutOfDateReason::Failed,
    ]
    .map(wire);
    assert_eq!(
        reasons,
        [
            "source_changed",
            "rebuilding",
            "awaiting_approval",
            "cannot_rebuild",
            "failed"
        ]
    );
}

#[test]
fn empty_kind_serializes_as_snake_case_strings() {
    // Exhaustive matches fail to compile when a variant is added without a new case here.
    match EmptyKind::NoVision {
        EmptyKind::NoVision
        | EmptyKind::SectionsNoVision
        | EmptyKind::NoMetaplan
        | EmptyKind::PlansNoMetaplan
        | EmptyKind::NeverBuilt
        | EmptyKind::Building => {}
    }
    let kinds = [
        EmptyKind::NoVision,
        EmptyKind::SectionsNoVision,
        EmptyKind::NoMetaplan,
        EmptyKind::PlansNoMetaplan,
        EmptyKind::NeverBuilt,
        EmptyKind::Building,
    ]
    .map(wire);
    assert_eq!(
        kinds,
        [
            "no_vision",
            "sections_no_vision",
            "no_metaplan",
            "plans_no_metaplan",
            "never_built",
            "building"
        ]
    );
}
