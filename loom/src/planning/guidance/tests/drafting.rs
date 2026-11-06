use super::*;
use crate::plan_session::kind::SessionDir;
use crate::planning::guidance::team::tests::fixture as drafting_fixture;
use crate::planning::guidance::{draft_run, plugin, GuidanceModels, SessionGuidanceKind};

#[test]
fn kickoff_brief_derives_step_and_skill_without_a_target_or_feature() {
    let (_temp, project) = drafting_fixture();
    for (small, step) in [(false, "Kickoff"), (true, "Kickoff, small work")] {
        let text =
            brief::common_brief(&AuthoringStart::Kickoff { small }, &project, Some("f")).unwrap();
        let (_, block) = text.split_once("## Starting point\n\n").unwrap();

        assert!(block.starts_with(&format!("Step: {step}.\nSkill: loom-plan:kickoff.\n")));
        assert!(block.contains("confirm its size and name before writing anything under doc/"));
        assert!(!block.contains("Target:"));
        assert!(!block.contains("Feature: f"));
    }
}

#[test]
fn draft_all_briefs_derive_step_target_skill_and_team_limits_from_the_opening() {
    let (_temp, project) = drafting_fixture();
    for (id, step, skill) in [
        (
            "draft-all-sections",
            "2, Draft all missing sections",
            "section",
        ),
        ("draft-all-plans", "4, Draft all missing plans", "plan"),
    ] {
        let start = AuthoringStart::from_offer_id(id, &project, Some("f")).unwrap();
        let run = draft_run(&project, id, "f").unwrap();

        let text = brief::common_brief(&start, &project, Some("f")).unwrap();
        let (_, block) = text.split_once("## Starting point\n\n").unwrap();

        assert_eq!(start.label(), "Draft all with a team");
        assert!(block.starts_with(&format!(
            "Step: {step}.\nTarget: Draft these missing documents"
        )));
        assert!(block.contains(&format!("Skill: loom-plan:{skill}.\n")));
        assert!(block.contains("skill's team mode with 5 teammates (cap: 5)"));
        assert!(block.contains("Write nothing outside these documents"));
        for path in run.documents.iter().chain(&run.bundles) {
            assert!(block.contains(&path.display().to_string()), "{block}");
        }
    }
}

#[test]
fn rendered_authoring_plugin_installs_all_skills_including_kickoff() {
    let temp = tempfile::tempdir().unwrap();
    let dir = SessionDir::new(temp.path().join("session"));
    fs::create_dir(dir.path()).unwrap();
    let models = GuidanceModels::resolve(&Default::default(), &Default::default());

    plugin::write_plugin(
        &SessionGuidanceKind::Authoring(AuthoringStart::Kickoff { small: false }),
        &dir,
        &models,
    )
    .unwrap();

    let mut skills: Vec<_> = fs::read_dir(dir.plugin().join("skills"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    skills.sort_unstable();
    assert_eq!(
        skills,
        [
            "brainstorming",
            "kickoff",
            "knowledge",
            "metaplan",
            "plan",
            "refine",
            "review",
            "section",
            "summaries",
            "vision",
        ]
    );
}
