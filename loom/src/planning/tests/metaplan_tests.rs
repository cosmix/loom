//! Tests for `crate::planning::metaplan`.

use std::path::Path;

use crate::planning::metaplan::{entries, is_metaplan};
use crate::planning::trace::tokens::SectionRef;

#[test]
fn recognizes_only_direct_index_paths() {
    for path in ["doc/plans/index.md", "doc/plans/a/index.md"] {
        assert!(is_metaplan(Path::new(path)), "{path}");
    }
    for path in [
        "doc/plans/optim-token-metaplan.md",
        "doc/plans/b/metaplan-02.md",
        "doc/plans/archive/metaplan.md",
        "doc/plans/metaplan.txt",
        "doc/product/metaplan.md",
        "doc/plans/PLAN-example.md",
        "doc/plans/a/archive/metaplan.md",
        "doc/plans/a/briefs/example/metaplan.md",
        "doc/plans/Bad/index.md",
    ] {
        assert!(!is_metaplan(Path::new(path)), "{path}");
    }
}

#[test]
fn extracts_lifecycle_names_and_real_heading_forms() {
    let text = concat!(
        "# Metaplan\n",
        "## 1. Document model and project checklist (`PLAN-feat-planner-01-documents.md`)\n",
        "## 1. Delivery truth (`PLAN-02-delivery-truth.md`)\n",
        "## Running (`IN_PROGRESS-PLAN-alpha.md`)\n",
        "## Done (`DONE-PLAN-beta.md`)\n",
        "## Retired (`RETIRED-PLAN-gamma.md`)\n",
        "## First valid (`notes` and `PLAN-delta.md` and `PLAN-other.md`)\n",
    );
    let found = entries(text);
    let expected = [
        (
            "PLAN-feat-planner-01-documents.md",
            "feat-planner-01-documents",
        ),
        ("PLAN-02-delivery-truth.md", "02-delivery-truth"),
        ("IN_PROGRESS-PLAN-alpha.md", "alpha"),
        ("DONE-PLAN-beta.md", "beta"),
        ("RETIRED-PLAN-gamma.md", "gamma"),
        ("PLAN-delta.md", "delta"),
    ];
    assert_eq!(found.len(), expected.len());
    for (index, (file, slug)) in expected.into_iter().enumerate() {
        assert_eq!(found[index].plan_file.as_deref(), Some(file));
        assert_eq!(found[index].slug.as_deref(), Some(slug));
        assert_eq!(found[index].line, index + 1);
    }
}

#[test]
fn numbered_gaps_opt_out_and_fenced_headings() {
    let text = concat!(
        "# Metaplan\n",
        "## 6. Policy notes (no plan file)\n",
        "## 7. Other notes (NO PLAN FILE)\n",
        "## Overview\n",
        "## 3. Missing name\n",
        "```markdown\n## 4. Hidden (`PLAN-hidden.md`)\n```\n",
        "## 8. No plan file yet (`PLAN-actual.md`)\n",
        "### 9. Nested (`PLAN-nested.md`)\n",
    );
    let found = entries(text);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].heading, "3. Missing name");
    assert_eq!(found[0].line, 4);
    assert_eq!(found[0].plan_file, None);
    assert_eq!(found[0].slug, None);
    assert_eq!(found[1].slug.as_deref(), Some("actual"));
}

#[test]
fn preserves_deduplicated_anchors_from_all_headings() {
    let text = "# Title\n### 3. Missing name\n## 3. Missing name\n## 3. Missing name\n";
    let found = entries(text);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].anchor, "3-missing-name-1");
    assert_eq!(found[1].anchor, "3-missing-name-2");
}

#[test]
fn covers_line_is_read_inside_the_entry_only() {
    let text = "# M\n\n## 1. A (`PLAN-a.md`)\n\nCovers: section_01#one, section_02.md, bogus, section_01#one\n\
                Covers: section_08\n\n### Notes\n\nA note.\n\n## 2. B (`PLAN-b.md`)\n\nNo covers here.\n\n\
                ## 3. C (`PLAN-c.md`)\n\nCovers: section_03\n";
    let found = entries(text);
    assert_eq!(found.len(), 3);
    let sref = |number, anchor: Option<&str>| SectionRef {
        number,
        anchor: anchor.map(str::to_owned),
    };
    assert_eq!(found[0].covers, [sref(1, Some("one")), sref(2, None)]);
    assert_eq!(found[0].bad_covers, ["bogus"]);
    assert!(found[1].covers.is_empty() && found[1].bad_covers.is_empty());
    assert_eq!(found[2].covers, [sref(3, None)]);
}

#[test]
fn fenced_covers_lines_and_span_hashes() {
    let fence = "`".repeat(3);
    let text = format!(
        "# M\n## 1. A (`PLAN-a.md`)\n{fence}\nCovers: section_09\n{fence}\nCovers: section_01\n\
         ## 2. B (`PLAN-b.md`)\nbody\n"
    );
    let found = entries(&text);
    assert_eq!(found[0].covers.len(), 1);
    assert_eq!(found[0].covers[0].number, 1);
    assert!(found[1].covers.is_empty());
    let edited = entries(&text.replace("body", "body two"));
    assert_eq!(found[0].span_hash, edited[0].span_hash);
    assert_ne!(found[1].span_hash, edited[1].span_hash);
}

#[test]
fn empty_covers_value_is_one_bad_token() {
    let found = entries("# M\n## 1. A (`PLAN-a.md`)\nCovers:\n");
    assert!(found[0].covers.is_empty());
    assert_eq!(found[0].bad_covers, [""]);
}
