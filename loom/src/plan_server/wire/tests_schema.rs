//! Check that planning wire enum values appear in their matching TypeScript arrays.

use super::checklist::{ChecklistView, CurrentView, MarkView, OfferKind};
use super::concept::ConceptChanged;
use super::message::{ClientMessage, PlanMessage};
use super::presence::{AuthoringView, PresenceList};
use super::server::{BindKind, PrereqKind, Refused};

const SCHEMA_TS: &str = include_str!("../../../../web/src/plan/api/schema.ts");

fn assert_all_in_schema<T: serde::Serialize + std::fmt::Debug>(
    values: &[T],
    enum_name: &str,
    array_name: &str,
    tag: Option<&str>,
) {
    let declaration = format!("export const {array_name} = [");
    let array = SCHEMA_TS
        .split_once(&declaration)
        .unwrap_or_else(|| panic!("missing {array_name} in planning schema"))
        .1
        .split_once(']')
        .unwrap_or_else(|| panic!("unterminated {array_name} in planning schema"))
        .0;
    for value in values {
        let serialized = serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("failed to serialize {enum_name} {value:?}: {error}"));
        let wire = match tag {
            Some(field) => serialized.get(field).and_then(|value| value.as_str()),
            None => serialized.as_str(),
        }
        .unwrap_or_else(|| panic!("{enum_name} {value:?} has no string wire value"));
        assert!(
            array
                .split(|c: char| c == ',' || c.is_whitespace())
                .any(|part| part == format!("\"{wire}\"")),
            "{array_name} is missing {enum_name} variant {value:?} (wire value {wire:?})"
        );
    }
}

fn all_prereq_kinds() -> Vec<PrereqKind> {
    match PrereqKind::Tmux {
        PrereqKind::Tmux | PrereqKind::Claude | PrereqKind::ClaudeLogin | PrereqKind::Sandbox => {}
    }
    vec![
        PrereqKind::Tmux,
        PrereqKind::Claude,
        PrereqKind::ClaudeLogin,
        PrereqKind::Sandbox,
    ]
}

fn all_bind_kinds() -> Vec<BindKind> {
    match BindKind::Loopback {
        BindKind::Loopback | BindKind::Remote => {}
    }
    vec![BindKind::Loopback, BindKind::Remote]
}

fn all_mark_views() -> Vec<MarkView> {
    match MarkView::Done {
        MarkView::Done | MarkView::Gap | MarkView::Active | MarkView::Optional => {}
    }
    vec![
        MarkView::Done,
        MarkView::Gap,
        MarkView::Active,
        MarkView::Optional,
    ]
}

fn all_current_views() -> Vec<CurrentView> {
    match CurrentView::Ready {
        CurrentView::Step { .. } | CurrentView::Ready | CurrentView::ReadyAllDone => {}
    }
    vec![
        CurrentView::Step { number: 1 },
        CurrentView::Ready,
        CurrentView::ReadyAllDone,
    ]
}

fn all_offer_kinds() -> Vec<OfferKind> {
    match OfferKind::StartVision {
        OfferKind::StartVision
        | OfferKind::FixVisionTable
        | OfferKind::WriteSection
        | OfferKind::AddSectionToVision
        | OfferKind::StartMetaplan
        | OfferKind::WritePlan
        | OfferKind::AddPlanToMetaplan
        | OfferKind::PressureTest
        | OfferKind::StartRun
        | OfferKind::RefineSpec
        | OfferKind::NewMetaplanRound => {}
    }
    vec![
        OfferKind::StartVision,
        OfferKind::FixVisionTable,
        OfferKind::WriteSection,
        OfferKind::AddSectionToVision,
        OfferKind::StartMetaplan,
        OfferKind::WritePlan,
        OfferKind::AddPlanToMetaplan,
        OfferKind::PressureTest,
        OfferKind::StartRun,
        OfferKind::RefineSpec,
        OfferKind::NewMetaplanRound,
    ]
}

// Exhaustive matches fail to compile when a variant is added without a new case here.
fn assert_exhaustive(message: &PlanMessage) {
    match message {
        PlanMessage::Hello(_)
        | PlanMessage::Presence(_)
        | PlanMessage::Authoring(_)
        | PlanMessage::Checklist(_)
        | PlanMessage::ServerStopping
        | PlanMessage::Refused(_)
        | PlanMessage::TreeChanged(_)
        | PlanMessage::DocChanged(_)
        | PlanMessage::Validation(_)
        | PlanMessage::ConceptChanged(_)
        | PlanMessage::Chat(_) => {}
    }
}

fn all_plan_messages() -> Vec<PlanMessage> {
    assert_exhaustive(&PlanMessage::ServerStopping);
    let tree_changed: PlanMessage = serde_json::from_str(include_str!(
        "../../../../web/src/plan/api/fixtures/files-tree-changed.json"
    ))
    .unwrap();
    let doc_changed = serde_json::from_str(include_str!(
        "../../../../web/src/plan/api/fixtures/files-doc-changed.json"
    ))
    .unwrap();
    let validation = serde_json::from_str(include_str!(
        "../../../../web/src/plan/api/fixtures/files-validation.json"
    ))
    .unwrap();
    let hello: PlanMessage = serde_json::from_str(include_str!(
        "../../../../web/src/plan/api/fixtures/hello.json"
    ))
    .unwrap();
    let checklist = ChecklistView {
        current: CurrentView::Ready,
        steps: vec![],
        primary: None,
        secondary: vec![],
        items: vec![],
    };
    vec![
        hello,
        PlanMessage::Presence(PresenceList { entries: vec![] }),
        PlanMessage::Authoring(AuthoringView { holder: None }),
        PlanMessage::Checklist(checklist),
        PlanMessage::ServerStopping,
        PlanMessage::Refused(Refused {
            command: String::new(),
            reason: String::new(),
        }),
        tree_changed,
        PlanMessage::DocChanged(doc_changed),
        PlanMessage::Validation(validation),
        PlanMessage::ConceptChanged(ConceptChanged {
            pages: vec![],
            out_of_date: vec![],
            sources: vec![],
        }),
    ]
}

fn all_client_messages() -> Vec<ClientMessage> {
    match (ClientMessage::Reading { path: None }) {
        ClientMessage::SetName { .. } | ClientMessage::Reading { .. } | ClientMessage::Chat(_) => {}
    }
    vec![
        ClientMessage::SetName {
            name: String::new(),
        },
        ClientMessage::Reading { path: None },
    ]
}

#[test]
fn prereq_kind_wire_values_are_in_plan_schema() {
    assert_all_in_schema(&all_prereq_kinds(), "PrereqKind", "PREREQ_KINDS", None);
}

#[test]
fn bind_kind_wire_values_are_in_plan_schema() {
    assert_all_in_schema(&all_bind_kinds(), "BindKind", "BIND_KINDS", None);
}

#[test]
fn mark_view_wire_values_are_in_plan_schema() {
    assert_all_in_schema(&all_mark_views(), "MarkView", "STEP_MARKS", None);
}

#[test]
fn current_view_wire_values_are_in_plan_schema() {
    assert_all_in_schema(
        &all_current_views(),
        "CurrentView",
        "CURRENT_KINDS",
        Some("kind"),
    );
}

#[test]
fn offer_kind_wire_values_are_in_plan_schema() {
    assert_all_in_schema(&all_offer_kinds(), "OfferKind", "OFFER_KINDS", None);
}

#[test]
fn plan_message_wire_values_are_in_plan_schema() {
    assert_all_in_schema(
        &all_plan_messages(),
        "PlanMessage",
        "PLAN_MESSAGE_TYPES",
        Some("type"),
    );
}

#[test]
fn client_message_wire_values_are_in_plan_schema() {
    assert_all_in_schema(
        &all_client_messages(),
        "ClientMessage",
        "CLIENT_MESSAGE_TYPES",
        Some("type"),
    );
}
