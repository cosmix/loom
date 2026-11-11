//! Check that planning wire enum values appear in their matching TypeScript arrays.

use super::checklist::{offer_view, CurrentView, MarkView, OfferKind};
use super::commands::CatalogueWireState;
use super::concept::ConceptChanged;
use super::message::{ClientMessage, PlanMessage};
use super::presence::{AuthoringView, PresenceList};
use super::server::{BindKind, PrereqKind, Refused};
use crate::planning::checklist::Offer;

const SCHEMA_TS: &str = include_str!("../../../../web/src/plan/api/schema.ts");

fn assert_all_in_schema<T: serde::Serialize + std::fmt::Debug>(
    values: &[T],
    enum_name: &str,
    array_name: &str,
    tag: Option<&str>,
) {
    assert_all_in_source(SCHEMA_TS, values, enum_name, array_name, tag);
}

pub(super) fn assert_all_in_source<T: serde::Serialize + std::fmt::Debug>(
    source: &str,
    values: &[T],
    enum_name: &str,
    array_name: &str,
    tag: Option<&str>,
) {
    let declaration = format!("export const {array_name} = [");
    let array = source
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

fn section_offers() -> Vec<Offer> {
    let feature = "demo".to_owned();
    vec![
        Offer::StartVision {
            feature: feature.clone(),
        },
        Offer::FixVisionTable {
            feature: feature.clone(),
        },
        Offer::WriteSection {
            feature: feature.clone(),
            number: 1,
        },
        Offer::AddSectionToVision {
            feature: feature.clone(),
            number: 1,
        },
        Offer::DraftAllSections { feature },
    ]
}

fn plan_offers() -> Vec<Offer> {
    let feature = "demo".to_owned();
    vec![
        Offer::StartMetaplan {
            feature: feature.clone(),
        },
        Offer::WritePlan {
            feature: feature.clone(),
            metaplan: "doc/plans/demo/index.md".into(),
            slug: "x".into(),
        },
        Offer::PressureTest {
            feature: feature.clone(),
            slug: "x".into(),
        },
        Offer::DraftAllPlans { feature },
    ]
}

fn all_offers() -> Vec<Offer> {
    match Offer::StartRun {
        Offer::StartVision { .. }
        | Offer::FixVisionTable { .. }
        | Offer::WriteSection { .. }
        | Offer::AddSectionToVision { .. }
        | Offer::StartMetaplan { .. }
        | Offer::WritePlan { .. }
        | Offer::PressureTest { .. }
        | Offer::StartRun
        | Offer::PlanFollowUps
        | Offer::ReviewFeature
        | Offer::DraftAllSections { .. }
        | Offer::DraftAllPlans { .. }
        | Offer::MoveIntoFeature { .. } => {}
    }
    let mut offers = section_offers();
    offers.extend(plan_offers());
    offers.extend([
        Offer::StartRun,
        Offer::PlanFollowUps,
        Offer::ReviewFeature,
        Offer::MoveIntoFeature {
            path: "doc/product/index.md".into(),
        },
    ]);
    offers
}

fn all_offer_kinds() -> Vec<OfferKind> {
    match OfferKind::StartRun {
        OfferKind::StartVision
        | OfferKind::FixVisionTable
        | OfferKind::WriteSection
        | OfferKind::AddSectionToVision
        | OfferKind::StartMetaplan
        | OfferKind::WritePlan
        | OfferKind::PressureTest
        | OfferKind::StartRun
        | OfferKind::PlanFollowUps
        | OfferKind::ReviewFeature
        | OfferKind::DraftAllSections
        | OfferKind::DraftAllPlans
        | OfferKind::MoveIntoFeature => {}
    }
    vec![
        OfferKind::StartVision,
        OfferKind::FixVisionTable,
        OfferKind::WriteSection,
        OfferKind::AddSectionToVision,
        OfferKind::StartMetaplan,
        OfferKind::WritePlan,
        OfferKind::PressureTest,
        OfferKind::StartRun,
        OfferKind::PlanFollowUps,
        OfferKind::ReviewFeature,
        OfferKind::DraftAllSections,
        OfferKind::DraftAllPlans,
        OfferKind::MoveIntoFeature,
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
        | PlanMessage::SummaryPanel(_)
        | PlanMessage::SummaryQuestion(_)
        | PlanMessage::SummaryQuestionClosed { .. }
        | PlanMessage::SummaryTurnCost(_)
        | PlanMessage::Chat(_)
        | PlanMessage::Kickoff(_)
        | PlanMessage::DraftTeam(_)
        | PlanMessage::KickoffDrafts { .. }
        | PlanMessage::Review(_)
        | PlanMessage::PressureRun(_)
        | PlanMessage::PressureOptionsChanged
        | PlanMessage::RunState(_)
        | PlanMessage::RunOutput(_)
        | PlanMessage::RunStrip(_)
        | PlanMessage::TraceChanged { .. }
        | PlanMessage::SettleCommit(_)
        | PlanMessage::Background { .. }
        | PlanMessage::Commands { .. } => {}
    }
}

fn all_plan_messages() -> Vec<PlanMessage> {
    assert_exhaustive(&PlanMessage::ServerStopping);
    let kickoff_drafts = serde_json::from_str(include_str!(
        "../../../../web/src/plan/api/fixtures/kickoff-drafts.json"
    ))
    .unwrap();
    let hello = serde_json::from_str(include_str!("fixtures/hello.json")).unwrap();
    let checklist = serde_json::from_str(include_str!("fixtures/checklist-message.json")).unwrap();
    let settle_commit = serde_json::from_str(include_str!(
        "../../../../web/src/plan/api/fixtures/trace-settle-commit.json"
    ))
    .unwrap();
    let background = serde_json::from_str(include_str!(
        "../../../../web/src/plan/api/fixtures/background.json"
    ))
    .unwrap();
    let mut messages = vec![
        hello,
        settle_commit,
        background,
        PlanMessage::Presence(PresenceList { entries: vec![] }),
        PlanMessage::Authoring(AuthoringView { holder: None }),
        checklist,
        PlanMessage::ServerStopping,
        PlanMessage::Commands {
            session: String::new(),
            state: CatalogueWireState::Loading,
            generation: None,
            reason: None,
            entries: vec![],
        },
        kickoff_drafts,
        PlanMessage::PressureOptionsChanged,
        PlanMessage::TraceChanged {
            feature: String::new(),
        },
        PlanMessage::Refused(Refused {
            command: String::new(),
            reason: String::new(),
        }),
    ];
    messages.extend(files_messages());
    messages.push(PlanMessage::ConceptChanged(ConceptChanged {
        feature: None,
        pages: vec![],
        out_of_date: vec![],
        sources: vec![],
    }));
    messages
}

/// The file-tree messages built from shared wire fixtures.
fn files_messages() -> Vec<PlanMessage> {
    let tree_changed: PlanMessage =
        serde_json::from_str(include_str!("fixtures/files-tree-changed.json")).unwrap();
    let doc_changed = serde_json::from_str(include_str!(
        "../../../../web/src/plan/api/fixtures/files-doc-changed.json"
    ))
    .unwrap();
    let validation = serde_json::from_str(include_str!(
        "../../../../web/src/plan/api/fixtures/files-validation.json"
    ))
    .unwrap();
    vec![
        tree_changed,
        PlanMessage::DocChanged(doc_changed),
        PlanMessage::Validation(validation),
    ]
}

fn all_client_messages() -> Vec<ClientMessage> {
    match (ClientMessage::Reading { path: None }) {
        ClientMessage::SetName { .. }
        | ClientMessage::Reading { .. }
        | ClientMessage::Chat(_)
        | ClientMessage::Review(_) => {}
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
    let offers: Vec<_> = all_offers().iter().map(offer_view).collect();
    assert_all_in_schema(&offers, "OfferView", "OFFER_KINDS", Some("kind"));
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
