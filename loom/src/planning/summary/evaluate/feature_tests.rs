//! Overview evaluation keeps each feature's sources and members isolated.

use super::fixtures::*;
use super::*;
use crate::planning::summary::{brief, claims, cost::estimate, format, hashes, seal, store, write};

#[test]
fn removed_overview_ids_are_rejected_without_claiming_or_writing() {
    let fx = two_features();
    let project = load(&fx.root);
    let id = ["overview", "demo", "engineering"].join("/");
    assert!(store::validate_id(&id).is_err());
    assert!(!store::is_overview(&id));
    assert!(store::summary_path(&id).as_os_str().is_empty());
    assert!(store::Scope::of(&project, &id).is_none());
    assert!(overview_inputs(&project, &id).is_none());
    assert!(!store::expected_ids(&project).contains(&id));
    assert!(brief::brief(&fx.root, &fx.paths, std::slice::from_ref(&id), None).is_err());
    assert!(claims::create_claim(&fx.paths, &id).is_err());
    assert!(claims::list_claims(&fx.paths).unwrap().is_empty());
    assert!(seal::seal(&fx.root, &fx.paths, &id, false).is_err());
    assert!(write::write_summary(&fx.root, &fx.paths, &id, "# Removed\n", None).is_err());
    assert!(format::parse_draft(&id, "# Removed\n").is_err());
    assert!(!fx.root.join(store::STORE_DIR).exists());
}

#[test]
fn metaplans_and_plans_add_no_engineering_overview_to_evaluation_or_estimates() {
    let fx = two_features();
    let project = load(&fx.root);
    let evaluation = evaluate(&project, &[]);
    let overviews: Vec<_> = evaluation
        .summaries
        .keys()
        .filter(|id| store::is_overview(id))
        .collect();
    assert_eq!(overviews, [PRODUCT, "overview/other/product"]);
    let sizes = estimate::source_sizes(&project, &evaluation);
    let hashes = hashes::input_hashes(&project, &evaluation);
    for id in [
        METAPLAN_ID,
        "other/metaplan/index",
        "plans/x",
        "plans/y",
        "plans/solo",
    ] {
        assert!(evaluation.summaries.contains_key(id), "{id}");
        assert!(sizes.contains_key(id), "{id}");
        assert!(hashes.contains_key(id), "{id}");
    }
    assert_eq!(sizes.len(), evaluation.summaries.len());
    assert_eq!(hashes.len(), evaluation.summaries.len());
    assert!(sizes.keys().all(|id| !id.ends_with("/engineering")));
    assert!(hashes.keys().all(|id| !id.ends_with("/engineering")));
}

#[test]
fn editing_one_feature_only_invalidates_its_product_overview() {
    let fx = two_features();
    let other = "other/product/section_01";
    let overview = "overview/other/product";
    for id in [ID1, other] {
        seal_document(&fx.root, id, &[("Alpha holds.", "a")]);
    }
    for (id, member) in [(PRODUCT, ID1), (overview, other)] {
        seal_overview(&fx.root, id, &[(member, "Alpha holds.")], &[]);
    }

    put(&fx.root, S1, &section(1, "Edited.", "Beta."));
    let eval = evaluate(&load(&fx.root), &[]);

    assert_eq!(eval.summaries[PRODUCT].state, SummaryState::Stale);
    assert_eq!(eval.summaries[overview].state, SummaryState::Current);
    assert_eq!(eval.scopes["demo"].product, [ID1]);
    assert_eq!(eval.scopes["other"].product, [other]);
    assert_eq!(eval.scopes["demo"].engineering, ["plans/x"]);
    assert_eq!(eval.scopes["other"].engineering, ["plans/y"]);
}

#[test]
fn overview_rejects_another_features_member_and_raw_source() {
    let fx = two_features();
    let other = "other/product/section_01";
    for id in [ID1, other] {
        seal_document(&fx.root, id, &[("Alpha holds.", "a")]);
    }
    seal_overview(
        &fx.root,
        PRODUCT,
        &[(ID1, "Alpha holds."), (other, "Alpha holds.")],
        &[("doc/product/other/index.md", "specification-sections")],
    );

    let eval = evaluate(&load(&fx.root), &[]);
    let states: Vec<_> = eval.summaries[PRODUCT]
        .sentences
        .iter()
        .map(|s| s.state)
        .collect();

    assert_eq!(
        states,
        [
            SentenceState::Current,
            SentenceState::Stale,
            SentenceState::Stale
        ]
    );
}
