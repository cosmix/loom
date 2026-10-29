//! Overview evaluation keeps each feature's sources and members isolated.

use super::fixtures::*;
use super::*;

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
