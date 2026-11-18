//! The metaplan summary id grammar: `<feature>/metaplan/index`, one per feature.

use std::path::{Path, PathBuf};

use super::{
    is_metaplan, is_vision, metaplan_feature, resolve_link, summary_path, validate_id, STORE_DIR,
};

#[test]
fn only_the_index_stem_under_a_valid_feature_is_a_metaplan_id() {
    for id in ["alpha/metaplan/index", "a1/metaplan/index"] {
        assert!(validate_id(id).is_ok(), "{id} is rejected");
        assert!(is_metaplan(id), "{id}");
    }
    assert_eq!(metaplan_feature("alpha/metaplan/index"), Some("alpha"));
    for id in [
        "alpha/metaplan/metaplan-02",
        "a1/metaplan/0",
        "alpha/metaplan/",
        "alpha/metaplan/..",
        "alpha/metaplan/../x",
        "alpha/metaplan/a/b",
        "alpha/metaplan/Index",
        "alpha/metaplan/-a",
        "alpha/metaplan/a_b",
        "alpha/metaplan/a.md",
        "Bad/metaplan/index",
        "plans/metaplan/index",
        "/metaplan/index",
        "metaplan/index",
    ] {
        assert!(validate_id(id).is_err(), "{id} is accepted");
        assert!(!is_metaplan(id), "{id}");
    }
    assert!(!is_vision("alpha/metaplan/index"));
}

#[test]
fn a_metaplan_summary_lives_beside_the_features_other_summaries() {
    assert_eq!(
        summary_path("alpha/metaplan/index"),
        PathBuf::from(STORE_DIR).join("alpha/metaplan/index.md")
    );
    assert_eq!(
        resolve_link(
            "alpha/metaplan/index",
            "../../../../plans/alpha/index.md#e1"
        ),
        Some((PathBuf::from("doc/plans/alpha/index.md"), Some("e1".into())))
    );
    assert_eq!(summary_path("alpha/metaplan/../x"), Path::new(""));
}
