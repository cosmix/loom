//! Tests for `planning::kinds`.

use std::path::Path;

use crate::planning::kinds::{classify, doc_id, DocKind};
use crate::planning::slug::PlanState;

mod features;

fn check(rel: &str, kind: DocKind, id: &str) {
    assert_eq!(classify(Path::new(rel)), kind, "{rel}");
    assert_eq!(doc_id(Path::new(rel), &kind).0, id, "{rel}");
}

fn plan(slug: &str, state: PlanState, feature: Option<&str>) -> DocKind {
    DocKind::Plan {
        slug: slug.into(),
        state,
        feature: feature.map(str::to_owned),
    }
}

fn section(feature: Option<&str>, number: u32) -> DocKind {
    DocKind::ProductSection {
        feature: feature.map(str::to_owned),
        number,
    }
}

#[test]
fn top_level_product_roles_are_misplaced_and_other_files_are_other() {
    check(
        "doc/product/index.md",
        DocKind::Vision { feature: None },
        "product/index",
    );
    for (path, number, id) in [
        ("doc/product/section_7.md", 7, "product/section_07"),
        ("doc/product/section_007.md", 7, "product/section_07"),
        ("doc/product/section_123.md", 123, "product/section_123"),
        ("doc/product/section_0.md", 0, "product/section_00"),
    ] {
        check(path, section(None, number), id);
    }
    for path in [
        "doc/product/notes.md",
        "doc/product/section_x.md",
        "doc/product/section_.md",
        "doc/product/section_4294967296.md",
        "doc/product/index.MD",
    ] {
        check(path, DocKind::Other, &path[4..]);
    }
}

#[test]
fn feature_product_roles_and_notes_have_feature_scoped_ids() {
    check(
        "doc/product/f/index.md",
        DocKind::Vision {
            feature: Some("f".into()),
        },
        "product/f/index",
    );
    for (path, number, id) in [
        ("doc/product/f/section_7.md", 7, "product/f/section_07"),
        ("doc/product/f/section_007.md", 7, "product/f/section_07"),
        ("doc/product/f/section_123.md", 123, "product/f/section_123"),
        ("doc/product/f/section_0.md", 0, "product/f/section_00"),
    ] {
        check(path, section(Some("f"), number), id);
    }
    for path in [
        "doc/product/f/notes.md",
        "doc/product/f/section_x.md",
        "doc/product/f/section_.md",
        "doc/product/f/section_4294967296.md",
        "doc/product/f/sub/section_01.md",
        "doc/product/f/sub/index.md",
        "doc/product/f/sub/deep/note.txt",
        "doc/product/f/archive/index.md",
        "doc/product/f/briefs/x/common.md",
    ] {
        check(
            path,
            DocKind::ProductNote {
                feature: "f".into(),
            },
            &path[4..],
        );
    }
}

#[test]
fn metaplans_are_only_classified_at_the_top_level_or_feature_root() {
    for path in [
        "doc/plans/index.md",
        "doc/plans/round-2-metaplan.md",
        "doc/plans/metaplan-01.md",
    ] {
        check(path, DocKind::Metaplan { feature: None }, &path[4..]);
    }
    for path in [
        "doc/plans/f/index.md",
        "doc/plans/f/round-2-metaplan.md",
        "doc/plans/f/metaplan-01.md",
    ] {
        check(
            path,
            DocKind::Metaplan {
                feature: Some("f".into()),
            },
            &path[4..],
        );
    }
    for path in [
        "doc/plans/f/sub/index.md",
        "doc/plans/f/sub/x-metaplan.md",
        "doc/plans/f/archive/index.md",
        "doc/plans/f/archive/x-metaplan.md",
        "doc/plans/f/metaplan.txt",
        "doc/plans/f/METAPLAN.md",
    ] {
        check(path, DocKind::Other, &path[4..]);
    }
}

#[test]
fn plan_lifecycle_spellings_keep_ids_across_standalone_and_feature_parents() {
    for (dir, feature) in [("doc/plans", None), ("doc/plans/f", Some("f"))] {
        for (file, state) in [
            ("PLAN-x.md", PlanState::Written),
            ("IN_PROGRESS-PLAN-x.md", PlanState::Running),
            ("DONE-PLAN-x.md", PlanState::Done),
            ("RETIRED-PLAN-x.md", PlanState::Retired),
            ("archive/PLAN-x.md", PlanState::Done),
            ("archive/IN_PROGRESS-PLAN-x.md", PlanState::Done),
            ("archive/DONE-PLAN-x.md", PlanState::Done),
            ("archive/RETIRED-PLAN-x.md", PlanState::Retired),
        ] {
            check(
                &format!("{dir}/{file}"),
                plan("x", state, feature),
                "plan/x",
            );
        }
    }
}

#[test]
fn briefs_keep_the_bundle_slug_at_any_depth() {
    for path in [
        "doc/plans/briefs/x/common.md",
        "doc/plans/briefs/x/stage/c1.md",
        "doc/plans/f/briefs/x/common.md",
        "doc/plans/f/briefs/x/stage/c1.md",
        "doc/plans/f/briefs/x/index.md",
        "doc/plans/f/briefs/x/PLAN-y.md",
        "doc/plans/f/briefs/x/REVIEW-PLAN-y.md",
        "doc/plans/f/briefs/x/metaplan.md",
    ] {
        check(path, DocKind::Brief { slug: "x".into() }, &path[4..]);
    }
    for path in [
        "doc/plans/briefs/x",
        "doc/plans/f/briefs/x",
        "doc/plans/archive/briefs/x/common.md",
        "doc/plans/f/archive/briefs/x/common.md",
    ] {
        check(path, DocKind::Other, &path[4..]);
    }
}

#[test]
fn run_reviews_accept_all_four_plan_parents() {
    for dir in [
        "doc/plans",
        "doc/plans/archive",
        "doc/plans/f",
        "doc/plans/f/archive",
    ] {
        for file in [
            "REVIEW-PLAN-x.md",
            "REVIEW-IN_PROGRESS-PLAN-x.md",
            "REVIEW-DONE-PLAN-x.md",
            "REVIEW-RETIRED-PLAN-x.md",
        ] {
            let path = format!("{dir}/{file}");
            check(&path, DocKind::RunReview { slug: "x".into() }, &path[4..]);
        }
    }
    for path in [
        "doc/plans/REVIEW-notes.md",
        "doc/plans/f/REVIEW-PLAN-.md",
        "doc/plans/f/sub/REVIEW-PLAN-x.md",
        "doc/plans/archive/sub/REVIEW-PLAN-x.md",
    ] {
        check(path, DocKind::Other, &path[4..]);
    }
}

#[test]
fn invalid_feature_names_are_other_for_every_document_role() {
    for feature in ["Bad", "a_b", "-x", "archive", "briefs"] {
        for file in ["index.md", "section_01.md", "notes.md", "sub/section_01.md"] {
            let path = format!("doc/product/{feature}/{file}");
            check(&path, DocKind::Other, &path[4..]);
        }
    }
    for feature in ["Bad", "a_b", "-x"] {
        for file in [
            "index.md",
            "round-metaplan.md",
            "PLAN-x.md",
            "archive/PLAN-x.md",
            "briefs/x/common.md",
            "REVIEW-PLAN-x.md",
            "archive/REVIEW-DONE-PLAN-x.md",
        ] {
            let path = format!("doc/plans/{feature}/{file}");
            check(&path, DocKind::Other, &path[4..]);
        }
    }
    for path in [
        "doc/plans/archive/index.md",
        "doc/plans/briefs/index.md",
        "doc/plans/archive/x-metaplan.md",
        "doc/plans/briefs/x-metaplan.md",
        "doc/plans/archive/archive/PLAN-x.md",
        "doc/plans/archive/briefs/x/common.md",
    ] {
        check(path, DocKind::Other, &path[4..]);
    }
}

#[test]
fn knowledge_and_unrecognized_paths_keep_path_ids() {
    for path in [
        "doc/loom/knowledge",
        "doc/loom/knowledge/INDEX.md",
        "doc/loom/knowledge/patterns/a.md",
    ] {
        check(path, DocKind::Knowledge, &path[4..]);
    }
    for path in [
        "doc/loom/knowledgeable.md",
        "doc/other/readme.md",
        "doc/productx/index.md",
        "doc/plansx/PLAN-x.md",
        "doc/plans/f/sub/PLAN-x.md",
        "doc/plans/f/PLAN-.md",
        "doc/plans/f/PLAN-x.txt",
        "doc/plans/f/notes.md",
        "doc/product/f",
        "doc/plans/f",
        "doc/product",
        "doc/plans",
        "README.md",
        "src/lib.rs",
    ] {
        let id = path.strip_prefix("doc/").unwrap_or(path);
        check(path, DocKind::Other, id);
    }
}
