use super::*;

#[test]
fn two_features_have_independent_checklists() {
    let (_temp, root) = tree(&[
        (VISION, vision(&[1, 2])),
        section(1),
        metaplan(&["demo-next"]),
    ]);
    let before = checklist(&root);
    put(&root, "doc/product/other/index.md", vision(&[1, 2]));
    put(&root, "doc/product/other/section_01.md", "# Other 01\n");
    put(&root, "doc/product/other/section_02.md", "# Other 02\n");
    put(
        &root,
        "doc/plans/other/index.md",
        "# Metaplan\n\n## 1. Ready (`PLAN-other-ready.md`)\n",
    );
    put(
        &root,
        "doc/plans/other/PLAN-other-ready.md",
        format!("{}{RECORD}", plan("Ready", "section_02")),
    );

    let project = Project::load(&root).unwrap();
    let demo = project.feature("demo").unwrap();
    let other = project.feature("other").unwrap();
    let names: Vec<_> = project.features.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["demo", "other"]);
    assert_eq!(demo.checklist, before);
    assert_eq!(demo.checklist.current, CurrentStep::Step(Step::ProductSpec));
    assert_eq!(demo.checklist.state.sections_written, [1]);
    assert!(demo.implemented_by.is_empty());
    assert_eq!(other.checklist.feature, "other");
    assert_eq!(other.checklist.current, CurrentStep::Ready);
    assert_eq!(other.checklist.state.sections_written, [1, 2]);
    assert_eq!(
        other.implemented_by.get(&2),
        Some(&vec!["other-ready".to_string()])
    );
}

#[test]
fn a_metaplan_entry_naming_another_features_plan_offers_no_duplicate_write() {
    let mut files = spec(vec![metaplan(&["foreign"])]);
    files.push(("doc/plans/other/PLAN-foreign.md", plan("Foreign", "none")));
    let (_temp, root) = tree(&files);
    let project = Project::load(&root).unwrap();
    let c = &project.feature("demo").unwrap().checklist;
    // Writing the plan here would create a second `foreign` slug: no WritePlan offer.
    assert_eq!(c.current, CurrentStep::Ready);
    assert!(
        !matches!(c.primary, Some(Offer::WritePlan { .. })),
        "{:?}",
        c.primary
    );
    assert!(c.state.plans_by_state.is_empty());
    assert_eq!(
        problems(&project, "metaplan-entry-foreign-plan").len(),
        1,
        "{:?}",
        project.problems
    );
    let other = &project.feature("other").unwrap().checklist;
    assert_eq!(other.state.plans_unlisted, 1);
}

#[test]
fn top_level_vision_offers_a_move_without_creating_a_feature() {
    let rel = "doc/product/index.md";
    let (_temp, root) = tree(&[(rel, vision(&[1, 2]))]);
    let project = Project::load(&root).unwrap();
    assert!(project.features.is_empty(), "{:?}", project.features);
    assert!(project.feature("demo").is_none());
    assert_eq!(project.moves, [Offer::MoveIntoFeature { path: p(rel) }]);
    assert_eq!(project.moves[0].target(), Some(p(rel)));
    assert_eq!(problems(&project, "needs-feature"), [(p(rel), None)]);
}

#[test]
fn top_level_sections_and_metaplans_also_need_a_feature() {
    let (_temp, root) = tree(&[
        ("doc/product/section_01.md", "# Section 01\n".into()),
        ("doc/plans/index.md", "# Metaplan\n".into()),
    ]);
    let project = Project::load(&root).unwrap();
    assert!(project.features.is_empty());
    assert_eq!(
        project.moves,
        [
            Offer::MoveIntoFeature {
                path: p("doc/plans/index.md")
            },
            Offer::MoveIntoFeature {
                path: p("doc/product/section_01.md")
            },
        ]
    );
    assert_eq!(
        problems(&project, "needs-feature"),
        [
            (p("doc/plans/index.md"), None),
            (p("doc/product/section_01.md"), None),
        ]
    );
}

#[test]
fn standalone_plans_do_not_change_a_features_checklist() {
    let (_temp, root) = tree(&spec(vec![metaplan(&[])]));
    let before = checklist(&root);
    put(&root, "doc/plans/PLAN-solo.md", plan("Solo", "none"));
    let project = Project::load(&root).unwrap();
    assert_eq!(project.standalone, ["solo"]);
    assert_eq!(project.features.len(), 1);
    assert_eq!(project.feature("demo").unwrap().checklist, before);
    assert!(project.moves.is_empty());
    assert!(problems(&project, "needs-feature").is_empty());
}

#[test]
fn one_missing_section_has_no_draft_all_offer() {
    let (_temp, root) = tree(&[(VISION, vision(&[1, 2, 3])), section(1), section(3)]);
    let c = checklist(&root);
    assert_eq!(
        c.primary,
        Some(Offer::WriteSection {
            feature: "demo".into(),
            number: 2
        })
    );
    assert_eq!(offers(&c), ["write-section:02", "start-metaplan"]);
    assert_eq!(c.primary.as_ref().unwrap().target(), Some(p(SECTIONS[1])));
}

#[test]
fn two_missing_sections_offer_draft_all_after_first_guided_offer() {
    let (_temp, root) = tree(&[(VISION, vision(&[3, 1, 2])), section(2)]);
    let c = checklist(&root);
    assert_eq!(
        c.primary,
        Some(Offer::WriteSection {
            feature: "demo".into(),
            number: 3
        })
    );
    assert_eq!(
        c.secondary[0],
        Offer::DraftAllSections {
            feature: "demo".into()
        }
    );
    assert_eq!(
        offers(&c),
        [
            "write-section:03",
            "draft-all-sections",
            "write-section:01",
            "start-metaplan"
        ]
    );
}

#[test]
fn one_missing_plan_has_no_draft_all_offer() {
    let written = format!("{}{RECORD}", plan("A", "section_01"));
    let (_temp, root) = tree(&spec(vec![
        metaplan(&["b", "a"]),
        ("doc/plans/demo/PLAN-a.md", written),
    ]));
    let c = checklist(&root);
    assert_eq!(c.current, CurrentStep::Step(Step::Plans));
    assert_eq!(
        c.primary,
        Some(Offer::WritePlan {
            feature: "demo".into(),
            metaplan: p(META),
            slug: "b".into(),
        })
    );
    assert_eq!(offers(&c), ["write-plan:b"]);
    assert_eq!(
        c.primary.as_ref().unwrap().target(),
        Some(p("doc/plans/demo/PLAN-b.md"))
    );
}

#[test]
fn two_missing_plans_offer_draft_all_after_first_guided_offer() {
    let (_temp, root) = tree(&spec(vec![metaplan(&["b", "a"])]));
    let c = checklist(&root);
    assert_eq!(c.current, CurrentStep::Step(Step::Plans));
    assert_eq!(
        c.primary,
        Some(Offer::WritePlan {
            feature: "demo".into(),
            metaplan: p(META),
            slug: "b".into(),
        })
    );
    assert_eq!(
        c.secondary[0],
        Offer::DraftAllPlans {
            feature: "demo".into()
        }
    );
    assert_eq!(
        offers(&c),
        ["write-plan:b", "draft-all-plans", "write-plan:a"]
    );
}

#[test]
fn repeated_missing_plan_entries_do_not_trigger_draft_all() {
    let (_temp, root) = tree(&spec(vec![metaplan(&["a", "a"])]));
    let c = checklist(&root);
    assert_eq!(offers(&c), ["write-plan:a"]);
    let gaps: Vec<_> = c.items.iter().filter(|i| i.step == Step::Plans).collect();
    assert_eq!(gaps.len(), 2);
    assert!(gaps.iter().all(|i| i.status == ItemStatus::Gap));
}

#[test]
fn complete_documents_have_no_draft_all_offer() {
    let tested = format!("{}{RECORD}", plan("A", "section_01"));
    let (_temp, root) = tree(&spec(vec![
        metaplan(&["a"]),
        ("doc/plans/demo/PLAN-a.md", tested),
    ]));
    let c = checklist(&root);
    assert_eq!(c.current, CurrentStep::Ready);
    assert_eq!(offers(&c), ["start-run"]);
}

#[test]
fn earliest_gap_and_offer_order_include_both_draft_all_choices() {
    let (_temp, root) = tree(&[
        (VISION, vision(&[2, 1])),
        section(3),
        metaplan(&["b", "a"]),
        ("doc/plans/demo/PLAN-extra.md", plan("Extra", "none")),
    ]);
    let c = checklist(&root);
    assert_eq!(c.current, CurrentStep::Step(Step::ProductSpec));
    assert_eq!(
        offers(&c),
        [
            "write-section:02",
            "draft-all-sections",
            "write-section:01",
            "write-plan:b",
            "draft-all-plans",
            "write-plan:a",
            "pressure-test:extra",
            "add-section-to-vision:03",
        ]
    );
}

#[test]
fn unlisted_plans_never_offer_add_plan_to_metaplan() {
    let (_temp, root) = tree(&spec(vec![
        metaplan(&[]),
        ("doc/plans/demo/PLAN-x.md", plan("X", "none")),
        ("doc/plans/demo/IN_PROGRESS-PLAN-y.md", plan("Y", "none")),
    ]));
    let c = checklist(&root);
    assert_eq!(c.state.plans_unlisted, 2);
    let unlisted: Vec<_> = c
        .items
        .iter()
        .filter(|i| i.status == ItemStatus::Unlisted)
        .collect();
    assert_eq!(unlisted.len(), 2);
    assert!(unlisted.iter().all(|i| i.offer.is_none()), "{unlisted:?}");
    let item_offers = c.items.iter().filter_map(|i| i.offer.as_ref());
    let mut all = item_offers.chain(c.primary.iter()).chain(&c.secondary);
    assert!(all.all(|o| !o.id().starts_with("add-plan-to-metaplan")));
    // The running plan `Y` makes the feature reviewable, so `review-feature` follows.
    assert_eq!(
        offers(&c),
        ["pressure-test:x", "start-run", "review-feature"]
    );
}
