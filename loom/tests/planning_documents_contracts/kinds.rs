use super::p;
use loom::planning::kinds::{
    classify, doc_id, feature_dir_name, is_feature_name, plans_dir, product_dir, vision_path,
    DocKind,
};
use loom::planning::slug::PlanState;
use std::path::Path;

fn kind_and_id(rel: &str, kind: DocKind, id: &str) {
    assert_eq!(classify(Path::new(rel)), kind, "{rel}");
    assert_eq!(doc_id(Path::new(rel), &kind).0, id, "{rel}");
}

#[test]
fn feature_product_kinds_and_ids_from_path() {
    let feature = Some("demo".into());
    kind_and_id(
        "doc/product/demo/index.md",
        DocKind::Vision {
            feature: feature.clone(),
        },
        "product/demo/index",
    );
    kind_and_id(
        "doc/product/demo/section_07.md",
        DocKind::ProductSection { feature, number: 7 },
        "product/demo/section_07",
    );
    kind_and_id(
        "doc/product/demo/notes.md",
        DocKind::ProductNote {
            feature: "demo".into(),
        },
        "product/demo/notes.md",
    );
}

#[test]
fn misplaced_product_kinds_keep_their_ids() {
    kind_and_id(
        "doc/product/index.md",
        DocKind::Vision { feature: None },
        "product/index",
    );
    kind_and_id(
        "doc/product/section_07.md",
        DocKind::ProductSection {
            feature: None,
            number: 7,
        },
        "product/section_07",
    );
    kind_and_id("doc/product/notes.md", DocKind::Other, "product/notes.md");
}

#[test]
fn plans_kinds_and_ids_in_standalone_and_feature_homes() {
    for (home, feature) in [("doc/plans", None), ("doc/plans/demo", Some("demo".into()))] {
        for name in ["index.md", "round-2-metaplan.md"] {
            let rel = format!("{home}/{name}");
            kind_and_id(
                &rel,
                DocKind::Metaplan {
                    feature: feature.clone(),
                },
                &rel[4..],
            );
        }
        for (name, state) in [
            ("IN_PROGRESS-PLAN-x.md", PlanState::Running),
            ("archive/DONE-PLAN-x.md", PlanState::Done),
        ] {
            let kind = DocKind::Plan {
                slug: "x".into(),
                state,
                feature: feature.clone(),
            };
            kind_and_id(&format!("{home}/{name}"), kind, "plan/x");
        }
        let brief = format!("{home}/briefs/x/common.md");
        kind_and_id(&brief, DocKind::Brief { slug: "x".into() }, &brief[4..]);
        let review = format!("{home}/REVIEW-IN_PROGRESS-PLAN-x.md");
        kind_and_id(
            &review,
            DocKind::RunReview { slug: "x".into() },
            &review[4..],
        );
    }
}

#[test]
fn other_document_kinds_and_feature_nesting_boundary() {
    let index = "doc/loom/knowledge/INDEX.md";
    kind_and_id(index, DocKind::Knowledge, "loom/knowledge/INDEX.md");
    kind_and_id("doc/other/readme.md", DocKind::Other, "other/readme.md");
    kind_and_id(
        "doc/plans/sub/PLAN-y.md",
        DocKind::Plan {
            slug: "y".into(),
            state: PlanState::Written,
            feature: Some("sub".into()),
        },
        "plan/y",
    );
    kind_and_id(
        "doc/plans/demo/sub/PLAN-y.md",
        DocKind::Other,
        "plans/demo/sub/PLAN-y.md",
    );
}

#[test]
fn feature_names_and_directory_helpers() {
    for name in ["demo", "a", "0", "2fa", "a-b", "a-"] {
        assert!(is_feature_name(name), "{name}");
    }
    for name in [
        "", "-a", "Bad", "a_b", "a.b", "é", "archive", "briefs", "plans", "overview",
    ] {
        assert!(!is_feature_name(name), "{name}");
    }
    assert_eq!(product_dir("demo"), p("doc/product/demo"));
    assert_eq!(vision_path("demo"), p("doc/product/demo/index.md"));
    assert_eq!(plans_dir(Some("demo")), p("doc/plans/demo"));
    assert_eq!(plans_dir(None), p("doc/plans"));
    for (rel, want) in [
        ("doc/product/demo/index.md", Some("demo")),
        ("doc/plans/demo/archive/DONE-PLAN-x.md", Some("demo")),
        ("doc/product/Bad/index.md", Some("Bad")),
        ("doc/product/index.md", None),
        ("doc/plans/PLAN-x.md", None),
        ("doc/plans/archive/DONE-PLAN-x.md", None),
        ("doc/plans/briefs/x/common.md", None),
    ] {
        assert_eq!(feature_dir_name(Path::new(rel)), want, "{rel}");
    }
}
