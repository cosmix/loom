//! Feature-card facts and the seven planning-step counters.

use std::collections::BTreeSet;
use std::path::Path;

use super::{ChecklistView, PlanCountsView, SectionView};
use crate::fs::knowledge::index::title_case;
use crate::planning::checklist::{ItemStatus, ProjectState};
use crate::planning::kinds::{section_path, vision_path};
use crate::planning::project::{Feature, Project};
use crate::planning::slug::PlanState;

const VISION_SUFFIXES: &[&str] = &["Product Vision", "Vision"];
const METAPLAN_SUFFIXES: &[&str] = &["Engineering Metaplan", "Metaplan"];

pub(super) fn card_facts(project: &Project, feature: &str, view: &mut ChecklistView) {
    let Some(data) = project.feature(feature) else {
        return;
    };
    let state = &data.checklist.state;
    let vision = vision_path(feature);
    view.has_product = project.docs.contains_key(&vision);
    view.title = document_title(project, &vision, VISION_SUFFIXES)
        .or_else(|| {
            let (path, _) = data.metaplans.first()?;
            document_title(project, path, METAPLAN_SUFFIXES)
        })
        .unwrap_or_else(|| title_case(feature));
    view.sections = sections(project, data);

    let listed: BTreeSet<_> = data
        .metaplans
        .iter()
        .flat_map(|(_, entries)| entries)
        .filter_map(|entry| entry.slug.as_deref())
        .collect();
    let existing = listed
        .iter()
        .filter(|slug| project.tree.plan(slug).is_some())
        .count();
    let count = |plan_state| wire_count(*state.plans_by_state.get(&plan_state).unwrap_or(&0));
    view.plans = PlanCountsView {
        listed: wire_count(listed.len()),
        total: wire_count(state.plans_by_state.values().sum()),
        written: count(PlanState::Written),
        running: count(PlanState::Running),
        done: count(PlanState::Done),
    };
    view.untested = wire_count(
        data.checklist
            .items
            .iter()
            .filter(|item| item.status == ItemStatus::Untested)
            .count(),
    );
    step_facts(state, existing, view);
}

fn sections(project: &Project, feature: &Feature) -> Vec<SectionView> {
    let Ok(rows) = &feature.vision_rows else {
        return Vec::new();
    };
    rows.iter()
        .map(|row| SectionView {
            number: format!("{:02}", row.number),
            name: row.name.clone(),
            written: feature
                .checklist
                .state
                .sections_written
                .contains(&row.number),
            open_questions: project
                .docs
                .get(&section_path(&feature.name, row.number))
                .map_or(0, |doc| wire_count(doc.open_questions)),
        })
        .collect()
}

fn step_facts(state: &ProjectState, existing: usize, view: &mut ChecklistView) {
    let written = view
        .sections
        .iter()
        .filter(|section| section.written)
        .count();
    let listed = view.sections.len();
    for step in &mut view.steps {
        let (count, label) = match step.number {
            1 => (view.has_product.then(|| "1".into()), "Vision"),
            2 => (
                (listed > 0).then(|| format!("{written}/{listed}")),
                "Product spec",
            ),
            3 => (
                (!state.metaplans.is_empty()).then(|| state.metaplans.len().to_string()),
                "Metaplan",
            ),
            4 => (
                if view.plans.listed > 0 {
                    Some(format!("{existing}/{}", view.plans.listed))
                } else {
                    (view.plans.total > 0).then(|| view.plans.total.to_string())
                },
                "Plans",
            ),
            5 if view.untested > 0 => (Some(view.untested.to_string()), "Untested"),
            5 => (None, "Pressure test"),
            6 => (None, "Review"),
            7 if view.plans.running > 0 => (Some(view.plans.running.to_string()), "Running"),
            7 if view.plans.done > 0 => (Some(view.plans.done.to_string()), "Done"),
            7 => (None, "Run"),
            _ => continue,
        };
        step.count = count;
        step.label = label.into();
    }
}

fn document_title(project: &Project, path: &Path, suffixes: &[&str]) -> Option<String> {
    let heading = project
        .docs
        .get(path)?
        .sections
        .headings
        .iter()
        .find(|heading| heading.level == 1)?;
    strip_title(&heading.title, suffixes)
}

fn strip_title(title: &str, suffixes: &[&str]) -> Option<String> {
    let plain = title.replace('`', "");
    let title = plain.trim();
    let title = suffixes
        .iter()
        .find_map(|suffix| title.strip_suffix(*suffix))
        .map_or(title, |prefix| {
            prefix.trim_end_matches(|ch: char| ch.is_whitespace() || matches!(ch, '—' | '-'))
        });
    (!title.is_empty()).then(|| title.to_owned())
}

fn wire_count(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{strip_title, METAPLAN_SUFFIXES, VISION_SUFFIXES};
    use crate::plan_server::wire::checklist::{ChecklistView, ProjectChecklistView, StepView};
    use crate::planning::project::Project;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn tree(files: &[(&str, &str)]) -> TempDir {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("doc")).unwrap();
        for (rel, text) in files {
            let path = temp.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        temp
    }

    fn card(root: &Path, feature: &str) -> ChecklistView {
        let project = Project::load(root).unwrap();
        ProjectChecklistView::from(&project)
            .checklists
            .into_iter()
            .find(|view| view.feature == feature)
            .unwrap()
    }

    fn step(view: &ChecklistView, number: u8) -> &StepView {
        view.steps
            .iter()
            .find(|step| step.number == number)
            .unwrap()
    }

    #[test]
    fn test_card_title_falls_back_to_capitalised_feature_slug() {
        let temp = tree(&[(
            "doc/product/token-optimization/index.md",
            "# Product Vision\n",
        )]);
        let project = Project::load(temp.path()).unwrap();
        let feature = project.feature("token-optimization").unwrap();

        let initial = ChecklistView::from(&feature.checklist);
        let enriched = card(temp.path(), "token-optimization");

        assert_eq!(initial.title, "Token Optimization");
        assert_eq!(enriched.title, "Token Optimization");
    }

    #[test]
    fn test_step_two_counts_only_sections_listed_in_vision() {
        let vision = "# Demo\n\n## Specification sections\n\n\
                      | # | Section |\n| --- | --- |\n\
                      | 01 | Alpha |\n| 02 | Beta |\n| 03 | Gamma |\n";
        let temp = tree(&[
            ("doc/product/demo/index.md", vision),
            ("doc/product/demo/section_01.md", "# Alpha\n\nBody.\n"),
            ("doc/product/demo/section_02.md", "# Beta\n\nBody.\n"),
            ("doc/product/demo/section_04.md", "# Delta\n\nBody.\n"),
        ]);

        let view = card(temp.path(), "demo");

        assert_eq!(step(&view, 2).count.as_deref(), Some("2/3"));
    }

    #[test]
    fn test_step_two_has_no_count_without_listed_sections() {
        let temp = tree(&[
            ("doc/product/demo/index.md", "# Demo\n"),
            ("doc/product/demo/section_01.md", "# Alpha\n\nBody.\n"),
        ]);

        let view = card(temp.path(), "demo");

        assert_eq!(step(&view, 2).count, None);
    }

    #[test]
    fn test_step_four_counts_existing_listed_plans() {
        let meta = "# Metaplan\n\n## 1. Alpha (`PLAN-a.md`)\n\n\
                    ## 2. Missing (`PLAN-missing.md`)\n";
        let temp = tree(&[
            ("doc/plans/demo/index.md", meta),
            ("doc/plans/demo/PLAN-a.md", "# Alpha\n"),
            ("doc/plans/demo/PLAN-unlisted.md", "# Unlisted\n"),
        ]);

        let view = card(temp.path(), "demo");

        assert_eq!(step(&view, 4).count.as_deref(), Some("1/2"));
    }

    #[test]
    fn test_step_four_falls_back_to_total_without_listed_plans() {
        let temp = tree(&[
            ("doc/plans/demo/PLAN-a.md", "# Alpha\n"),
            ("doc/plans/demo/DONE-PLAN-b.md", "# Beta\n"),
        ]);

        let view = card(temp.path(), "demo");

        assert_eq!(step(&view, 4).count.as_deref(), Some("2"));
    }

    #[test]
    fn test_step_five_counts_written_plans_without_pressure_records() {
        let temp = tree(&[("doc/plans/demo/PLAN-a.md", "# Alpha\n")]);

        let view = card(temp.path(), "demo");
        let pressure = step(&view, 5);

        assert_eq!(
            (pressure.label.as_str(), pressure.count.as_deref()),
            ("Untested", Some("1"))
        );
    }

    #[test]
    fn test_step_five_uses_pressure_test_label_with_record() {
        let plan = "# Alpha\n\n## Pressure tests\n\n- 2026-10-26: Reviewed; 1 round\n";
        let temp = tree(&[("doc/plans/demo/PLAN-a.md", plan)]);

        let view = card(temp.path(), "demo");
        let pressure = step(&view, 5);

        assert_eq!(
            (pressure.label.as_str(), pressure.count.as_deref()),
            ("Pressure test", None)
        );
    }

    #[test]
    fn test_step_seven_running_takes_priority_over_done() {
        let temp = tree(&[
            ("doc/plans/demo/IN_PROGRESS-PLAN-a.md", "# Alpha\n"),
            ("doc/plans/demo/DONE-PLAN-b.md", "# Beta\n"),
        ]);

        let view = card(temp.path(), "demo");
        let run = step(&view, 7);

        assert_eq!(
            (run.label.as_str(), run.count.as_deref()),
            ("Running", Some("1"))
        );
    }

    #[test]
    fn test_listed_plan_count_deduplicates_across_metaplans() {
        let meta = "# Metaplan\n\n## 1. Alpha (`PLAN-a.md`)\n";
        let temp = tree(&[
            ("doc/plans/demo/index.md", meta),
            ("doc/plans/demo/round-2-metaplan.md", meta),
            ("doc/plans/demo/PLAN-a.md", "# Alpha\n"),
        ]);

        let view = card(temp.path(), "demo");

        assert_eq!(
            (step(&view, 3).count.as_deref(), view.plans.listed),
            (Some("2"), 1)
        );
    }

    #[test]
    fn test_strip_title_removes_vision_suffixes_and_separators() {
        for title in [
            "Loom Plan — Product Vision",
            "Loom Plan - Product Vision",
            "Loom Plan Product Vision",
            "Loom Plan Vision",
            "  `Loom Plan` — `Product Vision`  ",
        ] {
            assert_eq!(
                strip_title(title, VISION_SUFFIXES).as_deref(),
                Some("Loom Plan")
            );
        }
    }

    #[test]
    fn test_strip_title_removes_metaplan_suffixes() {
        for title in [
            "Token Optimization Metaplan",
            "Token Optimization Engineering Metaplan",
            "Token Optimization — Engineering Metaplan",
        ] {
            assert_eq!(
                strip_title(title, METAPLAN_SUFFIXES).as_deref(),
                Some("Token Optimization")
            );
        }
    }

    #[test]
    fn test_strip_title_empty_results_allow_fallback() {
        for title in [
            "",
            "  ` `  ",
            "Vision",
            "Product Vision",
            "— Product Vision",
        ] {
            assert_eq!(strip_title(title, VISION_SUFFIXES), None);
        }
        assert_eq!(strip_title("Engineering Metaplan", METAPLAN_SUFFIXES), None);
    }

    #[test]
    fn test_strip_title_preserves_case_and_nontrailing_words() {
        for title in ["Loom product vision", "Vision for Loom", "Loom-"] {
            assert_eq!(strip_title(title, VISION_SUFFIXES).as_deref(), Some(title));
        }
    }
}
