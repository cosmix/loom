//! Document rows and checklist conversion for planning clients.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{offer_view, ChecklistView, CurrentView, ItemView, MarkView, PlanCountsView, StepView};
use crate::plan_server::concept::links::plan_state_name;
use crate::plan_server::files::plan_view::latest_pressure_date;
use crate::planning::checklist::{Checklist, CurrentStep, Step, StepMark};
use crate::planning::project::{Feature, Project};
use crate::planning::slug::PlanEntry;

/// A metaplan document belonging to a feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetaplanRowView {
    /// Repository-relative document path, including the `doc/` prefix.
    pub path: String,
    /// The document's title.
    pub title: String,
}

/// A feature plan and its pressure-test state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRowView {
    /// Repository-wide plan identity.
    pub slug: String,
    /// Repository-relative document path, including the `doc/` prefix.
    pub path: String,
    /// The document's title.
    pub title: String,
    /// Lifecycle state using the file tree's state string.
    pub state: String,
    /// Latest pressure-test date, absent when the plan has no record.
    pub pressure_date: Option<String>,
    /// Whether a feature metaplan lists this plan.
    pub listed: bool,
}

/// Feature documents in metaplan order, followed by unlisted winning plans.
pub fn document_rows(
    project: &Project,
    feature: &Feature,
) -> (Vec<MetaplanRowView>, Vec<PlanRowView>) {
    let metaplans = feature
        .metaplans
        .iter()
        .map(|(path, _)| MetaplanRowView {
            path: path.to_string_lossy().into_owned(),
            title: project.docs.get(path).map_or_else(
                || path.to_string_lossy().into_owned(),
                |doc| doc.title.clone(),
            ),
        })
        .collect();
    (metaplans, plan_rows(project, feature))
}

fn plan_rows(project: &Project, feature: &Feature) -> Vec<PlanRowView> {
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    for (_, entries) in &feature.metaplans {
        for slug in entries.iter().filter_map(|entry| entry.slug.as_deref()) {
            let Some(plan) = project
                .tree
                .plan(slug)
                .filter(|plan| plan.feature.as_deref() == Some(feature.name.as_str()))
            else {
                continue;
            };
            if seen.insert(slug) {
                rows.push(plan_row(project, plan, true));
            }
        }
    }
    for plan in project.tree.plans.values() {
        if plan.feature.as_deref() == Some(feature.name.as_str()) && seen.insert(plan.slug.as_str())
        {
            rows.push(plan_row(project, plan, false));
        }
    }
    rows
}

fn plan_row(project: &Project, plan: &PlanEntry, listed: bool) -> PlanRowView {
    PlanRowView {
        slug: plan.slug.clone(),
        path: plan.path.to_string_lossy().into_owned(),
        title: project
            .docs
            .get(&plan.path)
            .map_or_else(|| plan.slug.clone(), |doc| doc.title.clone()),
        state: plan_state_name(plan.state).into(),
        pressure_date: project.text(&plan.path).and_then(latest_pressure_date),
        listed,
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
            feature: checklist.feature.clone(),
            pitch: None,
            has_product: false,
            sections: Vec::new(),
            metaplans: Vec::new(),
            plan_rows: Vec::new(),
            plans: PlanCountsView::default(),
            untested: 0,
            current,
            steps: checklist.steps.iter().map(step_view).collect(),
            primary: checklist.primary.as_ref().map(offer_view),
            secondary: checklist.secondary.iter().map(offer_view).collect(),
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
                    offer: item.offer.as_ref().map(offer_view),
                })
                .collect(),
            candidates: Vec::new(),
        }
    }
}

fn step_view((step, mark): &(Step, StepMark)) -> StepView {
    StepView {
        number: *step as u8,
        name: step.name().into(),
        mark: match mark {
            StepMark::Done => MarkView::Done,
            StepMark::Gap => MarkView::Gap,
            StepMark::Active => MarkView::Active,
            StepMark::Optional => MarkView::Optional,
        },
        count: None,
        label: step.name().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{document_rows, MetaplanRowView};
    use crate::planning::project::Project;
    use std::fs;

    fn project(files: &[(&str, &str)]) -> Project {
        let temp = tempfile::tempdir().unwrap();
        for (rel, text) in files {
            let path = temp.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        // Rows must use the loaded snapshot even after the source files disappear.
        Project::load(temp.path()).unwrap()
    }

    #[test]
    fn test_metaplan_rows_preserve_document_headings_and_hold_one_row() {
        let project = project(&[
            ("doc/plans/demo/index.md", "# Demo Engineering Metaplan\n"),
            ("doc/plans/demo/round-2-metaplan.md", "# Second round\n"),
        ]);

        let (metaplans, _) = document_rows(&project, project.feature("demo").unwrap());

        assert_eq!(
            metaplans,
            vec![MetaplanRowView {
                path: "doc/plans/demo/index.md".into(),
                title: "Demo Engineering Metaplan".into(),
            }]
        );
    }

    #[test]
    fn test_plan_rows_follow_headings_once_then_unlisted_feature_plans() {
        let first = "# First\n\n## 1. B (`PLAN-b.md`)\n\n\
                     ## 2. A (`PLAN-a.md`)\n\n## 3. Gap (`PLAN-missing.md`)\n\n\
                     ## 4. Foreign (`PLAN-foreign.md`)\n\n## Follow-ups\n\n\
                     ## 5. B (`PLAN-b.md`)\n\n## 6. C (`PLAN-c.md`)\n";
        let project = project(&[
            ("doc/plans/demo/index.md", first),
            ("doc/plans/demo/PLAN-a.md", "# A\n"),
            ("doc/plans/demo/PLAN-b.md", "# B\n"),
            ("doc/plans/demo/PLAN-c.md", "# C\n"),
            ("doc/plans/demo/PLAN-d.md", "# D\n"),
            ("doc/plans/other/PLAN-foreign.md", "# Foreign\n"),
            ("doc/plans/PLAN-standalone.md", "# Standalone\n"),
        ]);

        let (_, rows) = document_rows(&project, project.feature("demo").unwrap());
        let plans: Vec<_> = rows
            .iter()
            .map(|row| (row.slug.as_str(), row.listed))
            .collect();

        assert_eq!(plans, [("b", true), ("a", true), ("c", true), ("d", false)]);
    }

    #[test]
    fn test_plan_rows_use_winning_path_state_title_and_latest_pressure_date() {
        let tested = "# Archived A\n\n## Pressure tests\n\n\
                      - 2025-01-01: first\n- 2025-03-03: latest\n- 2025-02-02: last\n";
        let project = project(&[
            ("doc/plans/demo/PLAN-a.md", "# Old A\n"),
            ("doc/plans/demo/archive/DONE-PLAN-a.md", tested),
            ("doc/plans/demo/IN_PROGRESS-PLAN-b.md", "# Running B\n"),
            ("doc/plans/demo/RETIRED-PLAN-c.md", "# Retired C\n"),
            ("doc/plans/demo/PLAN-d.md", "# Written D\n"),
        ]);

        let (_, rows) = document_rows(&project, project.feature("demo").unwrap());
        let first = &rows[0];
        let states: Vec<_> = rows.iter().map(|row| row.state.as_str()).collect();

        assert_eq!(first.path, "doc/plans/demo/archive/DONE-PLAN-a.md");
        assert_eq!(first.title, "Archived A");
        assert_eq!(first.pressure_date.as_deref(), Some("2025-03-03"));
        assert_eq!(rows[1].pressure_date, None);
        assert_eq!(states, ["done", "running", "retired", "written"]);
    }
}
