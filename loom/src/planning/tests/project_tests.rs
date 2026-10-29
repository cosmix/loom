//! Project documents, feature isolation and refresh behavior.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::{tree, vision};
use crate::planning::checklist::Offer;
use crate::planning::discover::MAX_DOC_BYTES;
use crate::planning::project::{Feature, Project};
use crate::planning::sections::normalized_hash;
use crate::planning::vision::VisionProblem;
use crate::planning::watch::Batch;

const PLAN: &str = "doc/plans/PLAN-p.md";
const PLAN_TEXT: &str = "# Plan\n\nImplements: none\n\n## Notes\n\nBody text.\n\n## Other\n\nMore.\n\n\
    <!-- loom METADATA -->\n```yaml\nstages:\n  - id: alpha\n    name: A\n  - id: beta\n    name: B\n```\n\
    <!-- END loom METADATA -->\n";
const METADATA: &str = "<!-- loom METADATA -->\n```yaml\nstages:\n  - id: alpha\n    name: A\n  \
    - id: beta\n    name: B\n```\n<!-- END loom METADATA -->\n";

#[test]
fn section_text_is_the_span_text_with_the_span_hash() {
    let temp = tree(&[(PLAN, PLAN_TEXT)]);
    let project = Project::load(temp.path()).unwrap();
    let plan = Path::new(PLAN);
    let cases = [
        ("notes", "## Notes\n\nBody text.\n\n"),
        ("loom-metadata", METADATA),
        ("stage:alpha", "  - id: alpha\n    name: A\n"),
        ("stage:beta", "  - id: beta\n    name: B\n"),
    ];
    for (anchor, want) in cases {
        let (span, text) = project.section_text(plan, anchor).unwrap().unwrap();
        assert_eq!(text, want, "{anchor}");
        assert_eq!(span.hash, normalized_hash(&text), "{anchor}");
        assert_eq!(Some(&span), project.docs[plan].sections.find(anchor));
    }
}

#[test]
fn section_text_is_none_for_unknown_targets_and_after_an_edit() {
    let temp = tree(&[(PLAN, PLAN_TEXT)]);
    let project = Project::load(temp.path()).unwrap();
    let plan = Path::new(PLAN);
    let none = |rel: &str, anchor: &str| project.section_text(Path::new(rel), anchor).unwrap();
    assert!(none(PLAN, "missing").is_none());
    assert!(none(PLAN, "stage:gamma").is_none());
    assert!(none("doc/plans/PLAN-q.md", "notes").is_none());
    assert!(project.section_text(plan, "notes").unwrap().is_some());
    fs::write(temp.path().join(PLAN), format!("{PLAN_TEXT}\nAppended.\n")).unwrap();
    assert!(project.section_text(plan, "notes").unwrap().is_none());
}

#[test]
fn content_hash_is_the_file_hash_of_a_readable_markdown_document() {
    let temp = tree(&[(PLAN, PLAN_TEXT), ("doc/notes.txt", "plain\n")]);
    fs::write(temp.path().join("doc/bad.md"), [0xE9u8]).unwrap();
    let project = Project::load(temp.path()).unwrap();
    let plan = Path::new(PLAN);
    let want = normalized_hash(PLAN_TEXT);
    assert_eq!(project.content_hash(plan), Some(want.as_str()));
    let file_hash = project.docs[plan].sections.file_hash.as_str();
    assert_eq!(project.content_hash(plan), Some(file_hash));
    for rel in ["doc/notes.txt", "doc/bad.md", "doc/none.md"] {
        assert_eq!(project.content_hash(Path::new(rel)), None, "{rel}");
    }
    let bad = Path::new("doc/bad.md");
    assert!(project.section_text(bad, "").unwrap().is_none());
}

#[test]
fn implemented_by_lists_each_slug_once_per_section_sorted() {
    let plan = |line: &str| format!("# P\n\n{line}\n");
    let temp = tree(&[
        (
            "doc/plans/alpha/PLAN-b.md",
            plan("Implements: section_02, section_01, section_01"),
        ),
        ("doc/plans/alpha/PLAN-a.md", plan("Implements: section_01")),
        ("doc/plans/alpha/PLAN-n.md", plan("Implements: none")),
        ("doc/plans/alpha/PLAN-u.md", plan("No declaration.")),
        (
            "doc/plans/beta/PLAN-other.md",
            plan("Implements: section_01"),
        ),
        ("doc/plans/PLAN-alone.md", plan("Implements: section_01")),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let want = BTreeMap::from([
        (1, vec!["a".to_string(), "b".to_string()]),
        (2, vec!["b".to_string()]),
    ]);
    assert_eq!(project.feature("alpha").unwrap().implemented_by, want);
    assert_eq!(project.implements.len(), 6);
}

#[test]
fn an_unreadable_non_empty_section_counts_as_written() {
    let temp = tree(&[("doc/product/alpha/index.md", vision(&[1, 2, 3, 4]))]);
    let product = temp.path().join("doc/product/alpha");
    fs::write(product.join("section_01.md"), [0xE9u8]).unwrap();
    fs::write(product.join("section_02.md"), "").unwrap();
    fs::write(product.join("section_03.md"), " \n\t\n").unwrap();
    let oversized = fs::File::create(product.join("section_04.md")).unwrap();
    oversized.set_len(MAX_DOC_BYTES as u64 + 1).unwrap();
    let project = Project::load(temp.path()).unwrap();
    let unreadable = project
        .docs
        .get(Path::new("doc/product/alpha/section_01.md"));
    assert!(unreadable.is_some_and(|doc| doc.read_error.is_some()));
    let checklist = &project.feature("alpha").unwrap().checklist;
    assert_eq!(checklist.state.sections_written, [1, 4]);
    for (number, expected) in [(1, false), (2, true), (3, true), (4, false)] {
        let offered = checklist
            .primary
            .iter()
            .chain(&checklist.secondary)
            .any(|offer| {
                matches!(offer, Offer::WriteSection { number: offered, .. } if *offered == number)
            });
        assert_eq!(offered, expected, "section {number}");
    }
}

#[test]
fn features_keep_their_own_documents_and_standalone_plans_are_separate() {
    let temp = tree(&[
        ("doc/product/beta/index.md", vision(&[7])),
        ("doc/product/alpha/index.md", vision(&[1])),
        ("doc/product/alpha/section_01.md", "# A\n".into()),
        ("doc/product/beta/section_07.md", "# B\n".into()),
        (
            "doc/plans/beta/index.md",
            "# B\n\n## 1. `PLAN-b.md`\n".into(),
        ),
        (
            "doc/plans/alpha/metaplan-02.md",
            "# A2\n\n## 1. `PLAN-a.md`\n".into(),
        ),
        (
            "doc/plans/alpha/index.md",
            "# A\n\n## 1. `PLAN-a.md`\n".into(),
        ),
        (
            "doc/plans/alpha/PLAN-a.md",
            "# A\n\nImplements: section_01\n".into(),
        ),
        (
            "doc/plans/beta/PLAN-b.md",
            "# B\n\nImplements: section_07\n".into(),
        ),
        (PLAN, PLAN_TEXT.into()),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let names: Vec<_> = project
        .features
        .iter()
        .map(|feature| feature.name.as_str())
        .collect();
    assert_eq!(names, ["alpha", "beta"]);
    assert_eq!(project.standalone, ["p"]);
    assert!(project.moves.is_empty());
    assert!(project.feature("missing").is_none());
    let alpha = project.feature("alpha").unwrap();
    assert_feature(alpha, 1, "a", &["index.md", "metaplan-02.md"]);
    assert_feature(project.feature("beta").unwrap(), 7, "b", &["index.md"]);
}

fn assert_feature(feature: &Feature, number: u32, slug: &str, files: &[&str]) {
    let rows: Vec<_> = feature
        .vision_rows
        .as_ref()
        .unwrap()
        .iter()
        .map(|row| row.number)
        .collect();
    assert_eq!(rows, [number]);
    assert_eq!(feature.checklist.feature, feature.name);
    assert_eq!(feature.checklist.state.sections_written, [number]);
    let expected = BTreeMap::from([(number, vec![slug.into()])]);
    assert_eq!(feature.implemented_by, expected);
    let paths: Vec<_> = feature
        .metaplans
        .iter()
        .map(|(path, entries)| {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].slug.as_deref(), Some(slug));
            path.clone()
        })
        .collect();
    let expected: Vec<_> = files
        .iter()
        .map(|file| Path::new("doc/plans").join(&feature.name).join(file))
        .collect();
    assert_eq!(paths, expected);
}

#[test]
fn misplaced_top_level_documents_offer_moves_without_creating_a_feature() {
    let paths = [
        "doc/product/index.md",
        "doc/product/section_01.md",
        "doc/plans/index.md",
    ];
    let temp = tree(&[
        (paths[0], vision(&[1])),
        (paths[1], "# S\n".into()),
        (paths[2], "# M\n".into()),
    ]);
    let project = Project::load(temp.path()).unwrap();
    assert!(project.features.is_empty());
    assert!(project.standalone.is_empty());
    assert_eq!(project.moves.len(), paths.len());
    assert!(project
        .moves
        .iter()
        .all(|offer| matches!(offer, Offer::MoveIntoFeature { .. })));
    let mut misplaced: Vec<_> = project
        .problems
        .iter()
        .filter(|problem| problem.code == "needs-feature")
        .map(|problem| problem.path.as_path())
        .collect();
    misplaced.sort();
    let mut expected: Vec<_> = paths.iter().map(Path::new).collect();
    expected.sort();
    assert_eq!(misplaced, expected);
}

#[test]
fn a_feature_can_have_only_plans_and_standalone_winners_are_sorted() {
    let temp = tree(&[
        (
            "doc/plans/only/PLAN-first.md",
            "# First\n\nImplements: none\n",
        ),
        (
            "doc/plans/only/archive/DONE-PLAN-second.md",
            "# Second\n\nImplements: none\n",
        ),
        ("doc/plans/PLAN-z.md", "# Z\n\nImplements: none\n"),
        (
            "doc/plans/archive/DONE-PLAN-z.md",
            "# Z\n\nImplements: none\n",
        ),
        ("doc/plans/PLAN-a.md", "# A\n\nImplements: none\n"),
    ]);
    let project = Project::load(temp.path()).unwrap();
    assert_eq!(project.features.len(), 1);
    let feature = project.feature("only").unwrap();
    assert_eq!(feature.vision_rows, Err(VisionProblem::NoHeading));
    assert!(feature.metaplans.is_empty());
    assert!(feature.implemented_by.is_empty());
    assert_eq!(feature.checklist.feature, "only");
    assert_eq!(project.standalone, ["a", "z"]);
}

#[test]
fn refresh_matches_a_fresh_load_after_adding_a_feature() {
    let alpha = "doc/product/alpha/index.md";
    let section = "doc/product/alpha/section_01.md";
    let added_section = "doc/product/alpha/section_02.md";
    let beta = "doc/product/beta/index.md";
    let moved = "doc/plans/beta/PLAN-p.md";
    let temp = tree(&[
        (alpha, vision(&[1])),
        (section, "# S\n".into()),
        (PLAN, PLAN_TEXT.into()),
    ]);
    let mut project = Project::load(temp.path()).unwrap();
    fs::create_dir_all(temp.path().join("doc/product/beta")).unwrap();
    fs::create_dir_all(temp.path().join("doc/plans/beta")).unwrap();
    fs::write(temp.path().join(alpha), vision(&[2])).unwrap();
    fs::remove_file(temp.path().join(section)).unwrap();
    fs::write(temp.path().join(added_section), "# S2\n").unwrap();
    fs::write(temp.path().join(beta), vision(&[7])).unwrap();
    fs::rename(temp.path().join(PLAN), temp.path().join(moved)).unwrap();
    fs::write(temp.path().join(moved), "# P\n\nImplements: section_07\n").unwrap();
    let batch = Batch {
        added: vec![added_section.into(), beta.into()],
        changed: vec![moved.into(), alpha.into()],
        removed: vec![section.into()],
        renamed: vec![(PLAN.into(), moved.into())],
        ..Batch::default()
    };
    project.refresh(&batch).unwrap();
    assert_eq!(project, Project::load(temp.path()).unwrap());
    assert_eq!(project.features.len(), 2);
    assert!(project.standalone.is_empty());
    assert_eq!(project.feature("beta").unwrap().implemented_by[&7], ["p"]);
}

#[test]
fn implements_resolves_against_the_plans_own_feature() {
    let own = "doc/plans/alpha/PLAN-own.md";
    let wrong = "doc/plans/beta/PLAN-wrong.md";
    let temp = tree(&[
        ("doc/product/alpha/index.md", vision(&[1])),
        ("doc/product/beta/index.md", vision(&[2])),
        (own, "# Own\n\nImplements: section_01\n".into()),
        (wrong, "# Wrong\n\nImplements: section_01\n".into()),
        (PLAN, "# Standalone\n\nImplements: none\n".into()),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let broken = |path: &str| {
        project
            .problems
            .iter()
            .any(|problem| problem.code == "implements-broken" && problem.path == Path::new(path))
    };
    assert!(!broken(own));
    assert!(broken(wrong));
    assert!(!broken(PLAN));
    assert_eq!(
        project.feature("alpha").unwrap().implemented_by[&1],
        ["own"]
    );
    assert_eq!(
        project.feature("beta").unwrap().implemented_by[&1],
        ["wrong"]
    );
}

#[test]
fn only_the_winning_plan_home_receives_its_implementation() {
    let temp = tree(&[
        (
            "doc/plans/alpha/PLAN-shared.md",
            "# A\n\nImplements: section_01\n",
        ),
        (
            "doc/plans/beta/archive/DONE-PLAN-shared.md",
            "# B\n\nImplements: section_02\n",
        ),
        (
            "doc/plans/PLAN-shared.md",
            "# S\n\nImplements: section_03\n",
        ),
    ]);
    let project = Project::load(temp.path()).unwrap();
    assert!(project.feature("alpha").unwrap().implemented_by.is_empty());
    let expected = BTreeMap::from([(2, vec!["shared".into()])]);
    assert_eq!(project.feature("beta").unwrap().implemented_by, expected);
    assert!(project.standalone.is_empty());
}

#[test]
fn a_feature_without_a_vision_does_not_use_a_top_level_vision() {
    let temp = tree(&[
        ("doc/product/index.md", vision(&[1])),
        (
            "doc/plans/only/PLAN-p.md",
            "# P\n\nImplements: none\n".into(),
        ),
    ]);
    let project = Project::load(temp.path()).unwrap();
    assert_eq!(project.features.len(), 1);
    assert_eq!(
        project.feature("only").unwrap().vision_rows,
        Err(VisionProblem::NoHeading)
    );
    assert_eq!(project.moves.len(), 1);
}
