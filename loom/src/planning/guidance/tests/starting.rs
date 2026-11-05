use super::*;
use crate::planning::checklist::Offer;
use crate::planning::slug::PlanState;

#[test]
fn a_message_start_without_documents_names_the_refine_skill_and_reports_no_feature() {
    let text = common_brief();
    let (_, start) = text.split_once("## Starting point\n\n").unwrap();
    assert_eq!(
        start,
        concat!(
            "Step: any, Refine.\nSkill: loom-plan:refine.\n\n",
            "No feature selected; no feature checklist applies.\n",
        )
    );
}

#[test]
fn starting_point_reports_loaded_vision_metaplan_and_running_plan() {
    let (_temp, project) = fixture(&[
        (
            "doc/product/f/index.md",
            "# Vision\n\n## Specification sections\n\n\
            | # | Section |\n| --- | --- |\n| 01 | Alpha |\n| 07 | Beta |\n",
        ),
        ("doc/product/f/section_01.md", "# Alpha\n"),
        ("doc/product/f/section_07.md", "# Beta\n"),
        (
            "doc/plans/f/index.md",
            "# Metaplan\n\n## 1. Delivery (`PLAN-foo.md`)\n",
        ),
        (
            "doc/plans/f/IN_PROGRESS-PLAN-foo.md",
            "# Plan\n\nImplements: section_01\n",
        ),
    ]);
    let text = brief::common_brief(
        &AuthoringStart::Message { paths: Vec::new() },
        &project,
        Some("f"),
    )
    .unwrap();
    for line in [
        "- Vision: present, 2 sections listed, 2 written (01, 07).",
        "- Metaplans: doc/plans/f/index.md.",
        "- Plans: 0 written (0 unlisted), 1 running, 0 done, 0 retired.",
        "- Locked plan files: doc/plans/f/IN_PROGRESS-PLAN-foo.md.",
    ] {
        assert!(
            text.lines().any(|actual| actual == line),
            "missing {line:?}"
        );
    }
}

#[test]
fn starting_point_uses_checklist_counts_and_sorts_only_locked_plan_files() {
    let (_temp, mut project) = fixture(&[("doc/product/f/index.md", "# Vision\n")]);
    let state = &mut project.features[0].checklist.state;
    state.plans_by_state = [
        (PlanState::Written, 3),
        (PlanState::Running, 2),
        (PlanState::Done, 4),
        (PlanState::Retired, 1),
    ]
    .into();
    state.plans_unlisted = 1;
    state.metaplans = ["doc/plans/f/index.md"].map(Into::into).into();
    state.locked_files = [
        "doc/plans/f/RETIRED-PLAN-z.md",
        "doc/plans/f/briefs/foo/stage.md",
        "doc/plans/f/IN_PROGRESS-PLAN-m.md",
        "doc/plans/f/REVIEW-PLAN-foo.md",
        "doc/plans/f/DONE-PLAN-a.md",
    ]
    .map(Into::into)
    .into();
    let text = brief::common_brief(
        &AuthoringStart::Message { paths: Vec::new() },
        &project,
        Some("f"),
    )
    .unwrap();
    assert!(text.contains("- Plans: 3 written (1 unlisted), 2 running, 4 done, 1 retired.\n"));
    assert!(text.contains("- Metaplans: doc/plans/f/index.md.\n"));
    assert_eq!(
        text.lines().last().unwrap(),
        concat!(
            "- Locked plan files: doc/plans/f/DONE-PLAN-a.md, ",
            "doc/plans/f/IN_PROGRESS-PLAN-m.md, doc/plans/f/RETIRED-PLAN-z.md.",
        )
    );
}

#[test]
fn starting_point_names_vision_and_completed_feature_offer_skills() {
    let (_temp, project) = fixture(&[]);
    for (start, expected) in [
        (
            AuthoringStart::Offer(Offer::StartVision {
                feature: "f".into(),
            }),
            concat!(
                "Step: 1, Vision.\nTarget: Write doc/product/f/index.md, the product vision.\n",
                "Skill: loom-plan:vision.\n"
            ),
        ),
        (
            AuthoringStart::Offer(Offer::PlanFollowUps),
            concat!(
                "Step: done, Plan follow-ups.\nTarget: Append follow-up entries under ",
                "\"## Follow-ups\" in doc/plans/f/index.md.\nSkill: loom-plan:metaplan.\n"
            ),
        ),
    ] {
        let text = brief::common_brief(&start, &project, Some("f")).unwrap();
        let (_, block) = text.split_once("## Starting point\n\n").unwrap();
        assert!(block.starts_with(expected), "{block}");
    }
}

#[test]
fn starting_point_names_offer_refinement_knowledge_and_message_skills() {
    let (_temp, project) = fixture(&[]);
    for (start, expected) in [
        (
            AuthoringStart::Refine {
                target: "doc/product/f/section_07.md".into(),
                passage: true,
            },
            concat!(
                "Step: any, Refine.\nTarget: Refine doc/product/f/section_07.md. ",
                "The message that follows attaches the passage.\nSkill: loom-plan:refine.\n"
            ),
        ),
        (
            AuthoringStart::Knowledge {
                open_file: Some("doc/loom/knowledge/index.md".into()),
            },
            concat!(
                "Step: any, Knowledge.\nTarget: Change the knowledge base under ",
                "doc/loom/knowledge/, starting from doc/loom/knowledge/index.md.\n",
                "Skill: loom-plan:knowledge.\n"
            ),
        ),
        (
            AuthoringStart::Message { paths: vec![] },
            "Step: any, Refine.\nSkill: loom-plan:refine.\n",
        ),
    ] {
        let text = brief::common_brief(&start, &project, Some("f")).unwrap();
        let (_, block) = text.split_once("## Starting point\n\n").unwrap();
        assert!(block.starts_with(expected), "{block}");
    }
}
