//! Tests for `crate::planning::project`: `section_text`, `content_hash`, `implemented_by`
//! and the rule for which product sections count as written.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::{tree, vision};
use crate::planning::checklist::Offer;
use crate::planning::discover::MAX_DOC_BYTES;
use crate::planning::kinds::VISION_PATH;
use crate::planning::project::Project;
use crate::planning::sections::normalized_hash;

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
            "doc/plans/PLAN-b.md",
            plan("Implements: section_02, section_01, section_01"),
        ),
        ("doc/plans/PLAN-a.md", plan("Implements: section_01")),
        ("doc/plans/PLAN-n.md", plan("Implements: none")),
        ("doc/plans/PLAN-u.md", plan("No declaration.")),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let want = BTreeMap::from([
        (1, vec!["a".to_string(), "b".to_string()]),
        (2, vec!["b".to_string()]),
    ]);
    assert_eq!(project.implemented_by, want);
}

#[test]
fn an_unreadable_non_empty_section_counts_as_written() {
    let temp = tree(&[(VISION_PATH, vision(&[1, 2, 3, 4]))]);
    let product = temp.path().join("doc/product");
    fs::write(product.join("section_01.md"), [0xE9u8]).unwrap();
    fs::write(product.join("section_02.md"), "").unwrap();
    fs::write(product.join("section_03.md"), " \n\t\n").unwrap();
    let oversized = fs::File::create(product.join("section_04.md")).unwrap();
    oversized.set_len(MAX_DOC_BYTES as u64 + 1).unwrap();
    let project = Project::load(temp.path()).unwrap();
    let unreadable = project.docs.get(Path::new("doc/product/section_01.md"));
    assert!(unreadable.is_some_and(|doc| doc.read_error.is_some()));
    let checklist = &project.checklist;
    assert_eq!(checklist.state.sections_written, [1, 4]);
    let offers = checklist.primary.iter().chain(&checklist.secondary);
    let ids: Vec<String> = offers.map(Offer::id).collect();
    for (id, expected) in [
        ("write-section:01", false),
        ("write-section:02", true),
        ("write-section:03", true),
        ("write-section:04", false),
    ] {
        assert_eq!(ids.iter().any(|offered| offered == id), expected, "{id}");
    }
}
