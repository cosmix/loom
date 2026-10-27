//! Checklist data sent to planning clients.

use serde::{Deserialize, Serialize};

use crate::planning::checklist::{Checklist, CurrentStep, Offer, Step, StepMark};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecklistView {
    pub current: CurrentView,
    pub steps: Vec<StepView>,
    pub primary: Option<OfferView>,
    pub secondary: Vec<OfferView>,
    pub items: Vec<ItemView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepView {
    pub number: u8,
    pub name: String,
    pub mark: MarkView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkView {
    Done,
    Gap,
    Active,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CurrentView {
    Step { number: u8 },
    Ready,
    ReadyAllDone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfferView {
    pub id: String,
    pub kind: OfferKind,
    pub label: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferKind {
    StartVision,
    FixVisionTable,
    WriteSection,
    AddSectionToVision,
    StartMetaplan,
    WritePlan,
    AddPlanToMetaplan,
    PressureTest,
    StartRun,
    RefineSpec,
    NewMetaplanRound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemView {
    pub step: u8,
    pub text: String,
    pub path: Option<String>,
    pub offer: Option<OfferView>,
}

impl From<&Offer> for OfferView {
    fn from(offer: &Offer) -> Self {
        let (kind, label) = match offer {
            Offer::StartVision => (OfferKind::StartVision, "Start a vision discussion".into()),
            Offer::FixVisionTable => (
                OfferKind::FixVisionTable,
                "Fix the vision's section table".into(),
            ),
            Offer::WriteSection { number } => (
                OfferKind::WriteSection,
                format!("Write section {number:02}"),
            ),
            Offer::AddSectionToVision { number } => (
                OfferKind::AddSectionToVision,
                format!("Add section {number:02} to the vision's table"),
            ),
            Offer::StartMetaplan => (
                OfferKind::StartMetaplan,
                "Start a metaplan from the product spec".into(),
            ),
            Offer::WritePlan { slug, .. } => (OfferKind::WritePlan, format!("Write plan {slug}")),
            Offer::AddPlanToMetaplan { slug } => (
                OfferKind::AddPlanToMetaplan,
                format!("Add {slug} to a metaplan"),
            ),
            Offer::PressureTest { slug } => {
                (OfferKind::PressureTest, format!("Pressure-test {slug}"))
            }
            Offer::StartRun => (OfferKind::StartRun, "Start a run".into()),
            Offer::RefineSpec => (
                OfferKind::RefineSpec,
                "Refine the spec in light of what was built".into(),
            ),
            Offer::NewMetaplanRound => (
                OfferKind::NewMetaplanRound,
                "Start a new metaplan round".into(),
            ),
        };
        Self {
            id: offer.id(),
            kind,
            label,
            target: offer
                .target()
                .map(|path| path.to_string_lossy().into_owned()),
        }
    }
}

impl From<&Checklist> for ChecklistView {
    fn from(checklist: &Checklist) -> Self {
        let current = match checklist.current {
            CurrentStep::Step(step) => CurrentView::Step { number: step as u8 },
            CurrentStep::Ready => CurrentView::Ready,
            CurrentStep::ReadyAllDone => CurrentView::ReadyAllDone,
        };
        Self {
            current,
            steps: checklist
                .steps
                .iter()
                .map(|(step, mark)| StepView {
                    number: *step as u8,
                    name: step_name(*step).into(),
                    mark: match mark {
                        StepMark::Done => MarkView::Done,
                        StepMark::Gap => MarkView::Gap,
                        StepMark::Active => MarkView::Active,
                        StepMark::Optional => MarkView::Optional,
                    },
                })
                .collect(),
            primary: checklist.primary.as_ref().map(OfferView::from),
            secondary: checklist.secondary.iter().map(OfferView::from).collect(),
            items: checklist
                .items
                .iter()
                .map(|item| ItemView {
                    step: item.step as u8,
                    text: item.label.clone(),
                    path: item
                        .path
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned()),
                    offer: item.offer.as_ref().map(OfferView::from),
                })
                .collect(),
        }
    }
}

fn step_name(step: Step) -> &'static str {
    match step {
        Step::Vision => "Vision",
        Step::ProductSpec => "Product spec",
        Step::Metaplan => "Metaplan",
        Step::Plans => "Plans",
        Step::PressureTest => "Pressure test",
        Step::Review => "Review",
        Step::Run => "Run",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::project::Project;
    use serde_json::{json, Value};

    #[test]
    fn checklist_fixture_matches_serde_output() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../web/src/plan/api/fixtures/checklist.json"
        ))
        .unwrap();
        let view: ChecklistView = serde_json::from_value(fixture.clone()).unwrap();
        assert_eq!(serde_json::to_value(view).unwrap(), fixture);
    }

    #[test]
    fn project_checklist_converts_to_wire_view() {
        let temp = tempfile::tempdir().unwrap();
        let product = temp.path().join("doc/product");
        std::fs::create_dir_all(&product).unwrap();
        std::fs::write(product.join("index.md"), "# Vision\n\n## Specification sections\n\n| # | Name |\n| --- | --- |\n| 05 | Example |\n").unwrap();
        let project = Project::load(temp.path()).unwrap();
        let view = ChecklistView::from(&project.checklist);
        assert_eq!(view.current, CurrentView::Step { number: 2 });
        assert_eq!(view.primary.as_ref().unwrap().id, "write-section:05");
        assert_eq!(view.primary.as_ref().unwrap().label, "Write section 05");
        assert_eq!(
            view.primary.as_ref().unwrap().target.as_deref(),
            Some("doc/product/section_05.md")
        );
        assert!(view.items.iter().any(|item| item.path.as_deref()
            == Some("doc/product/section_05.md")
            && item.offer == view.primary));
        assert_eq!(view.steps.len(), 7);
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(serde_json::from_value::<CurrentView>(
            json!({"kind":"step", "number":2, "extra":true})
        )
        .is_err());
        assert!(serde_json::from_value::<ItemView>(
            json!({"step":1,"text":"x","path":null,"offer":null,"extra":true})
        )
        .is_err());
    }
}
