use super::*;
use crate::planning::guidance::team::tests::fixture as drafting_fixture;
use crate::planning::guidance::DraftRun;

#[test]
fn kickoff_openings_name_the_skill_and_confirmation_without_a_feature_target() {
    let (_temp, project) = drafting_fixture();
    for (small, step) in [(false, "Kickoff"), (true, "Kickoff, small work")] {
        let opening =
            opening_message(&AuthoringStart::Kickoff { small }, &project, Some("f")).unwrap();

        assert!(opening.starts_with(&format!("loom plan: {step}.\n")));
        assert!(opening.contains("Load the skill loom-plan:kickoff,"));
        assert!(opening.contains("confirm its size and name before writing anything under doc/"));
        assert!(!opening.contains("Target:"));
        assert!(!opening.contains("Feature:"));
        assert!(!opening.contains("doc/product/f"));
    }
}

fn assert_team_opening(id: &str, skill: &str) {
    let (_temp, project) = drafting_fixture();
    let start = AuthoringStart::from_offer_id(id, &project, Some("f")).unwrap();
    let run = draft_run(&project, id, "f").unwrap();

    let opening = opening_message(&start, &project, Some("f")).unwrap();

    assert!(opening.contains(&format!("Load the skill loom-plan:{skill},")));
    assert!(opening.contains("skill's team mode"));
    assert!(opening.contains("5 teammates (cap: 5)"));
    assert!(opening.contains("Write nothing outside these documents"));
    for document in &run.documents {
        assert!(
            opening.contains(&document.display().to_string()),
            "{opening}"
        );
    }
    for bundle in &run.bundles {
        assert!(
            opening.contains(&format!("{}/", bundle.display())),
            "{opening}"
        );
    }
    assert_individual_targets(&project, &run);
}

fn assert_individual_targets(project: &Project, run: &DraftRun) {
    let feature = project.feature("f").unwrap();
    let offers = feature
        .checklist
        .primary
        .iter()
        .chain(&feature.checklist.secondary);
    for offer in offers.filter(|offer| {
        matches!(
            (offer, run.plans),
            (Offer::WriteSection { .. }, false) | (Offer::WritePlan { .. }, true)
        )
    }) {
        let individual =
            opening_message(&AuthoringStart::Offer(offer.clone()), project, Some("f")).unwrap();
        let document = offer.target().unwrap();
        assert!(individual.contains(&document.display().to_string()));
        assert!(run.documents.contains(&document));
        if let Offer::WritePlan { slug, .. } = offer {
            assert!(individual.contains(&format!("{}/", plan_bundle("f", slug).display())));
        }
    }
}

#[test]
fn draft_all_sections_names_documents_team_mode_and_cap() {
    assert_team_opening("draft-all-sections", "section");
}

#[test]
fn draft_all_plans_names_documents_bundles_team_mode_and_cap() {
    assert_team_opening("draft-all-plans", "plan");
}

#[test]
fn new_starts_have_skill_display_names() {
    for (start, expected) in [
        (
            AuthoringStart::Kickoff { small: false },
            "loom plan: kickoff",
        ),
        (
            AuthoringStart::Kickoff { small: true },
            "loom plan: kickoff",
        ),
        (
            AuthoringStart::Offer(Offer::DraftAllSections {
                feature: "f".into(),
            }),
            "loom plan: section",
        ),
        (
            AuthoringStart::Offer(Offer::DraftAllPlans {
                feature: "f".into(),
            }),
            "loom plan: plan",
        ),
    ] {
        assert_eq!(display_name("authoring", Some(&start)), expected);
    }
}
