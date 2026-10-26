//! Tests for `planning::kinds`.

use std::path::Path;

use crate::planning::kinds::{classify, doc_id, DocKind};
use crate::planning::slug::PlanState;

fn check(rel: &str, kind: DocKind, id: &str) {
    assert_eq!(classify(Path::new(rel)), kind, "{rel}");
    assert_eq!(doc_id(Path::new(rel), &kind).0, id, "{rel}");
}

fn plan(slug: &str, state: PlanState) -> DocKind {
    DocKind::Plan {
        slug: slug.into(),
        state,
    }
}

#[test]
fn product_rows() {
    check("doc/product/index.md", DocKind::Vision, "product/index");
    check(
        "doc/product/section_07.md",
        DocKind::ProductSection { number: 7 },
        "product/section_07",
    );
    check(
        "doc/product/section_123.md",
        DocKind::ProductSection { number: 123 },
        "product/section_123",
    );
    check(
        "doc/product/notes.md",
        DocKind::ProductNote,
        "product/notes.md",
    );
    check(
        "doc/product/section_x.md",
        DocKind::ProductNote,
        "product/section_x.md",
    );
    check(
        "doc/product/sub/section_01.md",
        DocKind::ProductNote,
        "product/sub/section_01.md",
    );
    check("doc/productx/index.md", DocKind::Other, "productx/index.md");
}

#[test]
fn metaplan_rows() {
    check("doc/plans/index.md", DocKind::Metaplan, "plans/index.md");
    check(
        "doc/plans/round-2-metaplan.md",
        DocKind::Metaplan,
        "plans/round-2-metaplan.md",
    );
    check(
        "doc/plans/sub/x-metaplan.md",
        DocKind::Other,
        "plans/sub/x-metaplan.md",
    );
}

#[test]
fn plan_rows_use_the_slug_for_every_spelling() {
    for (path, state) in [
        ("doc/plans/PLAN-x.md", PlanState::Written),
        ("doc/plans/IN_PROGRESS-PLAN-x.md", PlanState::Running),
        ("doc/plans/DONE-PLAN-x.md", PlanState::Done),
        ("doc/plans/RETIRED-PLAN-x.md", PlanState::Retired),
        ("doc/plans/archive/PLAN-x.md", PlanState::Done),
        ("doc/plans/archive/DONE-PLAN-x.md", PlanState::Done),
        ("doc/plans/archive/RETIRED-PLAN-x.md", PlanState::Retired),
    ] {
        check(path, plan("x", state), "plan/x");
    }
}

#[test]
fn plan_in_a_non_archive_subdirectory_is_other() {
    check(
        "doc/plans/sub/PLAN-y.md",
        DocKind::Other,
        "plans/sub/PLAN-y.md",
    );
}

#[test]
fn brief_rows() {
    check(
        "doc/plans/briefs/x/common.md",
        DocKind::Brief { slug: "x".into() },
        "plans/briefs/x/common.md",
    );
    check(
        "doc/plans/briefs/x/stage/c1.md",
        DocKind::Brief { slug: "x".into() },
        "plans/briefs/x/stage/c1.md",
    );
}

#[test]
fn run_review_rows() {
    let review = |slug: &str| DocKind::RunReview { slug: slug.into() };
    check(
        "doc/plans/REVIEW-IN_PROGRESS-PLAN-x.md",
        review("x"),
        "plans/REVIEW-IN_PROGRESS-PLAN-x.md",
    );
    check(
        "doc/plans/archive/REVIEW-DONE-PLAN-x.md",
        review("x"),
        "plans/archive/REVIEW-DONE-PLAN-x.md",
    );
    check(
        "doc/plans/REVIEW-notes.md",
        DocKind::Other,
        "plans/REVIEW-notes.md",
    );
}

#[test]
fn knowledge_and_other_rows() {
    check(
        "doc/loom/knowledge/INDEX.md",
        DocKind::Knowledge,
        "loom/knowledge/INDEX.md",
    );
    check(
        "doc/loom/knowledge/patterns/a.md",
        DocKind::Knowledge,
        "loom/knowledge/patterns/a.md",
    );
    check(
        "doc/loom/knowledgeable.md",
        DocKind::Other,
        "loom/knowledgeable.md",
    );
    check("doc/other/readme.md", DocKind::Other, "other/readme.md");
}

#[test]
fn kind_serializes_with_a_snake_case_tag() {
    let json = serde_json::to_value(plan("x", PlanState::Running)).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"kind": "plan", "slug": "x", "state": "running"})
    );
    let json = serde_json::to_value(DocKind::ProductSection { number: 3 }).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"kind": "product_section", "number": 3})
    );
}
