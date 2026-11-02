//! Feature checklists and the standalone plan shelf sent to planning clients.

use serde::{Deserialize, Serialize};

use super::files::PlanStateView;
use crate::planning::checklist::Offer;
use crate::planning::project::Project;
use crate::planning::slug::PlanState;

#[path = "checklist_card.rs"]
mod card;
#[path = "checklist_pitch.rs"]
mod pitch;
#[path = "checklist_rows.rs"]
mod rows;

pub use rows::{MetaplanRowView, PlanRowView};

/// All feature checklists, standalone plans and misplaced-document offers in a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectChecklistView {
    /// One checklist per feature, sorted by feature name.
    pub checklists: Vec<ChecklistView>,
    /// Standalone plans, sorted by repository-wide slug.
    pub standalone: Vec<StandalonePlanView>,
    /// Offers to move misplaced documents into a feature.
    pub moves: Vec<OfferView>,
}

/// A winning standalone plan, including its lifecycle state and document title.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandalonePlanView {
    /// Repository-wide plan identity.
    pub slug: String,
    /// Repository-relative path, including the `doc/` prefix.
    pub path: String,
    /// Lifecycle state of the winning copy.
    pub state: PlanStateView,
    /// The document's first level-one heading, or its filename.
    pub title: String,
}

/// The six-step checklist belonging to one feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecklistView {
    /// The feature directory name.
    pub feature: String,
    /// The vision's first paragraph as plain text, absent without a vision.
    pub pitch: Option<String>,
    /// Whether the feature's vision document exists.
    pub has_product: bool,
    /// Sections listed in the vision's section table, in table order.
    pub sections: Vec<SectionView>,
    /// Metaplan documents in feature order.
    pub metaplans: Vec<MetaplanRowView>,
    /// Plans in metaplan order, followed by unlisted feature plans.
    pub plan_rows: Vec<PlanRowView>,
    /// Listed plan count and plan file counts by lifecycle state.
    pub plans: PlanCountsView,
    /// Plans whose pressure-test item is untested.
    pub untested: u32,
    /// The earliest gap, or the ready state.
    pub current: CurrentView,
    /// Marks of the six planning steps.
    pub steps: Vec<StepView>,
    /// The next proposed action, when available.
    pub primary: Option<OfferView>,
    /// Other available actions.
    pub secondary: Vec<OfferView>,
    /// The feature's documents and gaps, each naming its step.
    pub items: Vec<ItemView>,
}

/// One named planning step and its current mark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepView {
    /// The one-based step number.
    pub number: u8,
    /// The display name of the step.
    pub name: String,
    /// Its completion or availability mark.
    pub mark: MarkView,
    /// The step's count as display text, such as `2/5`, absent when no count is available.
    pub count: Option<String>,
    /// What the count counts when that differs from the step, such as `Untested`; otherwise the
    /// step's name.
    pub label: String,
}

/// One section listed in the vision's section table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionView {
    /// The two-digit section number.
    pub number: String,
    /// The section's name in the vision.
    pub name: String,
    /// Whether the section document exists.
    pub written: bool,
    /// Open questions in the section document, or zero when absent.
    pub open_questions: u32,
}

/// Plan counts for a feature's metaplans and plan files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanCountsView {
    /// Plans listed by the feature's metaplans.
    pub listed: u32,
    /// Plan files in any lifecycle state.
    pub total: u32,
    /// Written plan files.
    pub written: u32,
    /// Running plan files.
    pub running: u32,
    /// Completed plan files.
    pub done: u32,
}

/// Completion and availability marks for planning steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkView {
    /// Complete.
    Done,
    /// Required work is missing.
    Gap,
    /// Work is running.
    Active,
    /// Available without blocking later steps.
    Optional,
}

/// The feature's next required step or readiness state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CurrentView {
    /// The earliest required step with a gap.
    Step {
        /// The one-based step number.
        number: u8,
    },
    /// Written plans are ready to run.
    Ready,
    /// Every plan has completed.
    ReadyAllDone,
}

/// A proposed action with its display label and variant-specific context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfferView {
    /// The planning core's stable offer id.
    pub id: String,
    /// The action's wire kind.
    pub kind: OfferKind,
    /// The human-readable action text.
    pub label: String,
    /// Repository-relative target document, when there is one.
    pub target: Option<String>,
    /// The owning feature, when the action belongs to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    /// The section number for section actions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    /// The plan slug for plan actions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    /// The metaplan naming a plan to write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metaplan: Option<String>,
}

/// Action kinds exposed to planning clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferKind {
    /// Discuss the product vision.
    StartVision,
    /// Repair the vision's section table.
    FixVisionTable,
    /// Draft a product section.
    WriteSection,
    /// List a section in the vision.
    AddSectionToVision,
    /// Draft the first metaplan.
    StartMetaplan,
    /// Draft an implementation plan.
    WritePlan,
    /// Pressure-test a plan.
    PressureTest,
    /// Run the plans.
    StartRun,
    /// Refine the product spec.
    RefineSpec,
    /// Start another metaplan round.
    NewMetaplanRound,
    /// Draft all missing product sections with a team.
    DraftAllSections,
    /// Draft all missing plans with a team.
    DraftAllPlans,
    /// Move a misplaced document into a feature.
    MoveIntoFeature,
}

/// Construct the typed wire action while preserving the core id and target.
pub fn offer_view(offer: &Offer) -> OfferView {
    OfferView::from(offer)
}

impl From<&Offer> for OfferKind {
    fn from(offer: &Offer) -> Self {
        match offer {
            Offer::StartVision { .. } => Self::StartVision,
            Offer::FixVisionTable { .. } => Self::FixVisionTable,
            Offer::WriteSection { .. } => Self::WriteSection,
            Offer::AddSectionToVision { .. } => Self::AddSectionToVision,
            Offer::StartMetaplan { .. } => Self::StartMetaplan,
            Offer::WritePlan { .. } => Self::WritePlan,
            Offer::PressureTest { .. } => Self::PressureTest,
            Offer::StartRun => Self::StartRun,
            Offer::RefineSpec => Self::RefineSpec,
            Offer::NewMetaplanRound => Self::NewMetaplanRound,
            Offer::DraftAllSections { .. } => Self::DraftAllSections,
            Offer::DraftAllPlans { .. } => Self::DraftAllPlans,
            Offer::MoveIntoFeature { .. } => Self::MoveIntoFeature,
        }
    }
}

impl From<&Offer> for OfferView {
    fn from(offer: &Offer) -> Self {
        let (number, slug, metaplan) = match offer {
            Offer::WriteSection { number, .. } | Offer::AddSectionToVision { number, .. } => {
                (Some(*number), None, None)
            }
            Offer::WritePlan { slug, metaplan, .. } => (
                None,
                Some(slug.clone()),
                Some(metaplan.to_string_lossy().into_owned()),
            ),
            Offer::PressureTest { slug, .. } => (None, Some(slug.clone()), None),
            _ => (None, None, None),
        };
        Self {
            id: offer.id(),
            kind: OfferKind::from(offer),
            label: offer.label(),
            target: offer
                .target()
                .map(|path| path.to_string_lossy().into_owned()),
            feature: offer.feature().map(str::to_owned),
            number,
            slug,
            metaplan,
        }
    }
}

/// One document or gap displayed in a checklist step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemView {
    /// The one-based step number.
    pub step: u8,
    /// The item's display text.
    pub text: String,
    /// Repository-relative document path, when the item names a document.
    pub path: Option<String>,
    /// An action associated with this item.
    pub offer: Option<OfferView>,
}

impl From<&Project> for ProjectChecklistView {
    fn from(project: &Project) -> Self {
        let mut checklists: Vec<_> = project
            .features
            .iter()
            .map(|feature| {
                let mut view = ChecklistView::from(&feature.checklist);
                view.pitch = pitch::feature_pitch(project, &feature.name);
                card::card_facts(project, &feature.name, &mut view);
                view
            })
            .collect();
        checklists.sort_by(|a, b| a.feature.cmp(&b.feature));
        let standalone = project
            .standalone
            .iter()
            .filter_map(|slug| {
                let plan = project.tree.plan(slug)?;
                let doc = project.docs.get(&plan.path)?;
                Some(StandalonePlanView {
                    slug: plan.slug.clone(),
                    path: plan.path.to_string_lossy().into_owned(),
                    state: match plan.state {
                        PlanState::Written => PlanStateView::Written,
                        PlanState::Running => PlanStateView::Running,
                        PlanState::Done => PlanStateView::Done,
                        PlanState::Retired => PlanStateView::Retired,
                    },
                    title: doc.title.clone(),
                })
            })
            .collect();
        Self {
            checklists,
            standalone,
            moves: project.moves.iter().map(offer_view).collect(),
        }
    }
}

#[cfg(test)]
#[path = "checklist_tests.rs"]
mod tests;
