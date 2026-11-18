//! Contract: a feature's vision has a document summary of its own, `<feature>/product/vision`,
//! stored beside the section summaries and never counted as a product overview member.

use std::path::{Path, PathBuf};

use super::{expected_ids, source_of, summary_id_for, summary_path, Scope};
use crate::planning::summary::evaluate::fixtures::{load, two_sections, PRODUCT, VISION};

const VISION_ID: &str = "demo/product/vision";

#[test]
fn a_feature_vision_has_a_document_summary() {
    let f = two_sections();
    let project = load(&f.root);

    let ids = expected_ids(&project);
    assert!(
        ids.iter().any(|id| id == VISION_ID),
        "expected ids lack {VISION_ID}: {ids:?}"
    );
    assert_eq!(
        summary_id_for(&project, Path::new(VISION)).as_deref(),
        Some(VISION_ID),
        "the vision maps to no summary id"
    );
    assert_eq!(
        summary_path(VISION_ID),
        PathBuf::from("doc/loom/summaries/demo/product/vision.md")
    );
    assert_eq!(source_of(&project, VISION_ID), Some(PathBuf::from(VISION)));

    let scope = Scope::of(&project, PRODUCT).expect("product overview scope");
    for (name, list) in [
        ("product", &scope.product),
        ("engineering", &scope.engineering),
        ("outside", &scope.outside),
    ] {
        assert!(
            !list.iter().any(|id| id == VISION_ID),
            "the vision is in the overview's {name} list: {list:?}"
        );
    }
}
