//! Cost rows and overview source sizes retain feature-qualified ids.

use super::estimate::{estimate, source_sizes, EstimateBasis};
use super::{CostLine, Level, TokenVector, Trigger};
use crate::planning::summary::evaluate::evaluate;
use crate::planning::summary::evaluate::fixtures::*;
use crate::planning::summary::store::is_overview;

#[test]
fn overview_sizes_and_starting_estimates_have_one_row_per_feature() {
    let fx = two_features();
    put(
        &fx.root,
        S2,
        &section(2, "Outside.", "Outside the product overview."),
    );
    let project = load(&fx.root);
    let evaluation = evaluate(&project, &[]);
    let sizes = source_sizes(&project, &evaluation);
    let pending: Vec<_> = sizes
        .iter()
        .filter(|(id, _)| is_overview(id))
        .map(|(id, bytes)| (id.clone(), Level::Overview, *bytes))
        .collect();

    let estimate = estimate(&[], &pending);

    assert_eq!(
        estimate.per_summary,
        [
            (PRODUCT.to_string(), 150_000),
            ("overview/other/product".to_string(), 150_000),
        ]
    );
    assert_eq!(estimate.tokens, 300_000);
    assert_eq!(sizes[PRODUCT], sizes[ID1]);
    assert_eq!(
        sizes["overview/other/product"],
        sizes["other/product/section_01"]
    );
    assert_ne!(sizes[PRODUCT], sizes["overview/other/product"]);
}

#[test]
fn last_overview_cost_is_keyed_by_feature_as_well_as_kind() {
    let record = |id: &str, tokens| CostLine {
        time: "2026-10-28T09:00:00Z".into(),
        trigger: Trigger::Background,
        summaries: vec![id.into()],
        level: Level::Overview,
        model: None,
        source_bytes: None,
        tokens: TokenVector {
            fresh_input: Some(tokens),
            cache_write: Some(0),
            output: Some(0),
            ..TokenVector::default()
        },
    };
    let other = "overview/other/product";
    let log = [record(PRODUCT, 100), record(other, 900)];
    let pending = [
        (PRODUCT.into(), Level::Overview, 1),
        (other.into(), Level::Overview, 1),
    ];

    let result = estimate(&log, &pending);

    assert_eq!(
        result.per_summary,
        [(PRODUCT.into(), 100), (other.into(), 900)]
    );
    assert_eq!(result.basis, EstimateBasis::Last);
}

#[test]
fn empty_overviews_have_zero_bytes_and_absent_overviews_have_no_rows() {
    let fx = fixture(&[
        (VISION, &vision(&[])),
        ("doc/plans/other/index.md", &metaplan(&[])),
        ("doc/plans/PLAN-solo.md", "# Solo\n\nImplements: none\n"),
    ]);
    let project = load(&fx.root);

    let sizes = source_sizes(&project, &evaluate(&project, &[]));

    assert_eq!(sizes[PRODUCT], 0);
    assert!(sizes.keys().all(|id| !id.ends_with("/engineering")));
    assert!(!sizes.contains_key("overview/other/product"));
    assert!(sizes.contains_key("plans/solo"));
}
