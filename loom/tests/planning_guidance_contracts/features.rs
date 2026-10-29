use super::*;
use loom::planning::guidance::{render_session_guidance, StartError};
use loom::planning::project::Project;

#[test]
fn offer_id_lookup_selects_feature_b_without_using_feature_a() {
    let fx = Fixture::new();
    for name in ["a", "b"] {
        put(&fx.root, &format!("doc/product/{name}/index.md"),
            "# Vision\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n| 01 | Missing |\n");
    }
    let project = Project::load(&fx.root).unwrap();

    let start = AuthoringStart::from_offer_id("write-section:01", &project, Some("b")).unwrap();
    let message = opening_message(&start, &project, Some("b")).unwrap();

    assert_eq!(
        start,
        AuthoringStart::Offer(Offer::WriteSection {
            feature: "b".into(),
            number: 1
        })
    );
    assert!(message.contains("Write doc/product/b/section_01.md"));
    assert!(message.contains("Feature: b."));
    assert!(!message.contains("doc/product/a/"));
}

#[test]
#[serial]
fn feature_session_brief_uses_only_its_checklist() {
    let fx = Fixture::new();
    put(
        &fx.root,
        "doc/product/b/index.md",
        "# B\n\n## Specification sections\n\n\
        | # | Section |\n| --- | --- |\n| 01 | B section |\n",
    );
    let project = Project::load(&fx.root).unwrap();
    let start = AuthoringStart::from_offer_id("write-section:01", &project, Some("b")).unwrap();
    let kind = SessionGuidanceKind::Authoring(start);
    let models = fx.models("");

    let guidance = render_session_guidance(
        &kind,
        &project,
        &fx.runtime,
        &fx.session,
        &models,
        Some("b"),
    )
    .unwrap();
    let brief = fs::read_to_string(guidance.brief_path).unwrap();

    for expected in [
        "Feature: b.",
        "Vision: doc/product/b/index.md.",
        "Sections: doc/product/b/section_NN.md.",
        "Metaplan: doc/plans/b/index.md.",
        "Plans: doc/plans/b/PLAN-<slug>.md.",
        "Briefs: doc/plans/b/briefs/<slug>/.",
        "- Vision: present, 1 sections listed, 0 written ().",
    ] {
        assert!(brief.contains(expected), "missing {expected:?}: {brief}");
    }
    assert!(!brief.contains("doc/plans/f/IN_PROGRESS-PLAN-b.md"));
}

#[test]
#[serial]
fn unavailable_offer_refusal_does_not_write_session_artifacts() {
    let fx = Fixture::new();
    let models = fx.models("");
    let kind = SessionGuidanceKind::Authoring(AuthoringStart::Offer(Offer::DraftAllSections {
        feature: "f".into(),
    }));

    let error = fx.render(&kind, &fx.session, &models).unwrap_err();

    assert_eq!(
        error.downcast_ref::<StartError>(),
        Some(&StartError::NotAvailableYet("draft-all-sections".into()))
    );
    assert!(!fx.session.brief().exists());
    assert!(!fx.session.plugin().exists());
}
