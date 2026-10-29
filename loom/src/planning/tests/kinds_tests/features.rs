//! Feature naming, membership, path helpers and serialization.

use super::{plan, section, DocKind, Path, PlanState};
use crate::planning::kinds::{
    feature_dir_name, is_feature_name, plans_dir, product_dir, vision_path,
};

#[test]
fn feature_name_validation_matches_the_ascii_grammar_and_reserved_names() {
    for name in [
        "a",
        "0",
        "feature-2",
        "0-a",
        "a--b",
        "a-",
        "archives",
        "briefs-2",
    ] {
        assert!(is_feature_name(name), "{name}");
    }
    for name in [
        "", "Bad", "a_b", "-x", "archive", "briefs", "a.b", "a/b", "a b", "é", "a\n",
    ] {
        assert!(!is_feature_name(name), "{name:?}");
    }
}

#[test]
fn feature_directory_detection_preserves_invalid_names_for_diagnostics() {
    for (path, expected) in [
        ("doc/product/f/index.md", Some("f")),
        ("doc/product/f/sub/note.md", Some("f")),
        ("doc/plans/f/archive/PLAN-x.md", Some("f")),
        ("doc/plans/f/briefs/x/common.md", Some("f")),
        ("doc/product/Bad/index.md", Some("Bad")),
        ("doc/plans/a_b/PLAN-x.md", Some("a_b")),
        ("doc/plans/-x/index.md", Some("-x")),
        ("doc/product/archive/index.md", Some("archive")),
        ("doc/product/briefs/index.md", Some("briefs")),
        ("doc/plans/archive/PLAN-x.md", None),
        ("doc/plans/briefs/x/common.md", None),
        ("doc/product/index.md", None),
        ("doc/plans/index.md", None),
        ("doc/plans/PLAN-x.md", None),
        ("doc/product/f", None),
        ("doc/plans/f", None),
        ("doc/product", None),
        ("doc/plans", None),
        ("doc/productx/f/index.md", None),
        ("doc/other/f/index.md", None),
    ] {
        assert_eq!(feature_dir_name(Path::new(path)), expected, "{path}");
    }
}

#[test]
fn feature_path_helpers_build_repository_relative_paths() {
    assert_eq!(product_dir("f"), Path::new("doc/product/f"));
    assert_eq!(plans_dir(None), Path::new("doc/plans"));
    assert_eq!(plans_dir(Some("f")), Path::new("doc/plans/f"));
    assert_eq!(vision_path("f"), Path::new("doc/product/f/index.md"));
    assert_eq!(vision_path("0-a"), Path::new("doc/product/0-a/index.md"));
}

#[test]
fn feature_accessor_returns_only_explicit_variant_membership() {
    for kind in [
        DocKind::Vision {
            feature: Some("f".into()),
        },
        section(Some("f"), 1),
        DocKind::ProductNote {
            feature: "f".into(),
        },
        DocKind::Metaplan {
            feature: Some("f".into()),
        },
        plan("x", PlanState::Written, Some("f")),
    ] {
        assert_eq!(kind.feature(), Some("f"), "{kind:?}");
    }
    for kind in [
        DocKind::Vision { feature: None },
        section(None, 1),
        DocKind::Metaplan { feature: None },
        plan("x", PlanState::Written, None),
        DocKind::Brief { slug: "x".into() },
        DocKind::RunReview { slug: "x".into() },
        DocKind::Knowledge,
        DocKind::Other,
    ] {
        assert_eq!(kind.feature(), None, "{kind:?}");
    }
}

#[test]
fn serialization_keeps_snake_case_tags_and_feature_fields() {
    for (kind, expected) in [
        (
            plan("x", PlanState::Running, Some("f")),
            serde_json::json!({
                "kind": "plan", "slug": "x", "state": "running", "feature": "f"
            }),
        ),
        (
            plan("x", PlanState::Written, None),
            serde_json::json!({
                "kind": "plan", "slug": "x", "state": "written", "feature": null
            }),
        ),
        (
            section(Some("f"), 3),
            serde_json::json!({
                "kind": "product_section", "number": 3, "feature": "f"
            }),
        ),
        (
            DocKind::ProductNote {
                feature: "f".into(),
            },
            serde_json::json!({
                "kind": "product_note", "feature": "f"
            }),
        ),
        (
            DocKind::Vision { feature: None },
            serde_json::json!({
                "kind": "vision", "feature": null
            }),
        ),
        (
            DocKind::Metaplan {
                feature: Some("f".into()),
            },
            serde_json::json!({
                "kind": "metaplan", "feature": "f"
            }),
        ),
    ] {
        assert_eq!(serde_json::to_value(kind).unwrap(), expected);
    }
}

#[test]
fn plan_and_review_names_containing_metaplan_stay_plans_and_reviews() {
    for (rel, feature) in [
        ("doc/plans/PLAN-metaplan-sync.md", None),
        ("doc/plans/f/PLAN-metaplan-sync.md", Some("f")),
        ("doc/plans/f/IN_PROGRESS-PLAN-metaplan-sync.md", Some("f")),
        ("doc/plans/f/archive/DONE-PLAN-metaplan-sync.md", Some("f")),
    ] {
        match crate::planning::kinds::classify(Path::new(rel)) {
            DocKind::Plan {
                slug, feature: got, ..
            } => {
                assert_eq!(slug, "metaplan-sync", "{rel}");
                assert_eq!(got.as_deref(), feature, "{rel}");
            }
            other => panic!("{rel}: {other:?}"),
        }
    }
    for rel in [
        "doc/plans/REVIEW-PLAN-metaplan-x.md",
        "doc/plans/f/REVIEW-PLAN-metaplan-x.md",
    ] {
        assert_eq!(
            crate::planning::kinds::classify(Path::new(rel)),
            DocKind::RunReview {
                slug: "metaplan-x".into()
            },
            "{rel}"
        );
    }
}
