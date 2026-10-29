use super::Fixture;
use crate::plan_server::chat::guidance::GuidanceProvider;
use crate::planning::project::Project;

#[test]
#[serial_test::serial]
fn authoring_trait_starts_the_selected_features_section_offer() {
    let fixture = Fixture::new();
    let root = fixture.ctx.root();
    std::fs::remove_file(root.join("doc/product/demo/section_01.md")).unwrap();
    *fixture.ctx.project_mut() = Project::load(root).unwrap();
    let provider: &dyn GuidanceProvider = fixture.provider.as_ref();

    let guidance = provider
        .authoring(
            &fixture.ctx,
            &fixture.session,
            Some("write-section:01"),
            None,
            Some("demo"),
        )
        .unwrap();

    assert!(
        guidance
            .opening_message
            .contains("doc/product/demo/section_01.md"),
        "{}",
        guidance.opening_message
    );
}

#[test]
#[serial_test::serial]
fn authoring_trait_rejects_a_section_offer_in_an_unknown_feature() {
    let fixture = Fixture::new();
    let provider: &dyn GuidanceProvider = fixture.provider.as_ref();

    let error = provider
        .authoring(
            &fixture.ctx,
            &fixture.session,
            Some("write-section:01"),
            None,
            Some("missing"),
        )
        .unwrap_err();

    assert_eq!(error.to_string(), "unknown feature: missing");
    assert!(!fixture.session.brief().exists());
    assert!(!fixture.session.plugin().exists());
}

#[test]
#[serial_test::serial]
fn selected_feature_is_written_into_the_brief() {
    let fixture = Fixture::new();

    let guidance = fixture.render(Some("write-plan:delivery"), None).unwrap();

    let brief = std::fs::read_to_string(guidance.brief_path).unwrap();
    assert!(brief.contains("Feature: demo."), "{brief}");
    assert!(guidance
        .opening_message
        .contains("doc/plans/demo/PLAN-delivery.md"));
}

#[test]
#[serial_test::serial]
fn identical_offer_ids_resolve_in_the_selected_feature() {
    let fixture = Fixture::with_files(&[("doc/plans/other/index.md", "# Other metaplan\n")]);
    let mut project = fixture.ctx.project_mut();
    for feature in &mut project.features {
        feature.checklist.primary = Some(crate::planning::checklist::Offer::StartMetaplan {
            feature: feature.name.clone(),
        });
    }
    drop(project);

    let guidance = fixture
        .provider
        .authoring(
            &fixture.ctx,
            &fixture.session,
            Some("start-metaplan"),
            None,
            Some("other"),
        )
        .unwrap();

    assert!(guidance
        .opening_message
        .contains("doc/plans/other/index.md"));
    assert!(!guidance.opening_message.contains("doc/plans/demo/index.md"));
}

#[test]
#[serial_test::serial]
fn invalid_or_unknown_feature_is_rejected_before_writing_guidance() {
    let fixture = Fixture::new();

    for (feature, message) in [
        ("../demo", "invalid feature name: ../demo"),
        ("Demo", "invalid feature name: Demo"),
        ("", "invalid feature name: "),
        ("missing", "unknown feature: missing"),
    ] {
        let error = fixture
            .provider
            .authoring(&fixture.ctx, &fixture.session, None, None, Some(feature))
            .unwrap_err();

        assert_eq!(error.to_string(), message);
        assert!(!fixture.session.brief().exists());
        assert!(!fixture.session.plugin().exists());
    }
}

#[test]
#[serial_test::serial]
fn feature_offer_without_feature_keeps_the_not_offered_error() {
    let fixture = Fixture::new();

    let error = fixture
        .provider
        .authoring(
            &fixture.ctx,
            &fixture.session,
            Some("write-plan:delivery"),
            None,
            None,
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "the step \"write-plan:delivery\" is not offered now"
    );
    assert!(!fixture.session.brief().exists());
}

#[test]
#[serial_test::serial]
fn no_feature_renders_a_general_chat_without_a_feature_checklist() {
    let fixture = Fixture::new();

    let guidance = fixture
        .provider
        .authoring(&fixture.ctx, &fixture.session, None, None, None)
        .unwrap();

    let brief = std::fs::read_to_string(guidance.brief_path).unwrap();
    assert!(
        brief.contains("No feature selected; no feature checklist applies."),
        "{brief}"
    );
}

fn draft_all_fixture() -> Fixture {
    let fixture = Fixture::with_files(&[
        (
            "doc/product/demo/index.md",
            concat!(
                "# Vision\n\n## Specification sections\n\n",
                "| # | Section |\n| --- | --- |\n| 01 | Alpha |\n| 02 | Beta |\n| 03 | Gamma |\n",
            ),
        ),
        (
            "doc/plans/demo/index.md",
            concat!(
                "# Metaplan\n\n## 1. Delivery (`PLAN-delivery.md`)\n\nDeliver it.\n",
                "\n## 2. More (`PLAN-more.md`)\n\nMore.\n",
            ),
        ),
        ("doc/product/index.md", "# Needs a feature\n"),
    ]);
    let root = fixture.ctx.root();
    std::fs::remove_file(root.join("doc/product/demo/section_01.md")).unwrap();
    *fixture.ctx.project_mut() = Project::load(root).unwrap();
    fixture
}

#[test]
#[serial_test::serial]
fn draft_all_renders_a_team_opening_and_file_move_keeps_its_refusal() {
    let fixture = draft_all_fixture();
    let plans = fixture.render(Some("draft-all-plans"), None).unwrap();
    assert!(plans
        .opening_message
        .contains("doc/plans/demo/PLAN-delivery.md"));
    assert!(plans
        .opening_message
        .contains("doc/plans/demo/PLAN-more.md"));
    assert!(plans.opening_message.contains("team mode"));
    let fixture = draft_all_fixture();
    let sections = fixture.render(Some("draft-all-sections"), None).unwrap();
    assert!(sections
        .opening_message
        .contains("doc/product/demo/section_01.md"));
    assert!(sections
        .opening_message
        .contains("doc/product/demo/section_02.md"));
    assert!(sections
        .opening_message
        .contains("doc/product/demo/section_03.md"));
    assert!(sections.opening_message.contains("team mode"));

    let id = "move-into-feature:doc/product/index.md";
    let error = fixture.render(Some(id), None).unwrap_err();
    assert_eq!(
        error.to_string(),
        "move-into-feature:doc/product/index.md is a file move, not a session"
    );
}
