//! Tests for `crate::planning::metaplan`.

use std::path::Path;

use crate::planning::metaplan::{entries, is_metaplan};

#[test]
fn recognizes_only_direct_metaplan_paths() {
    for path in ["doc/plans/index.md", "doc/plans/optim-token-metaplan.md"] {
        assert!(is_metaplan(Path::new(path)), "{path}");
    }
    for path in [
        "doc/plans/archive/metaplan.md",
        "doc/plans/metaplan.txt",
        "doc/product/metaplan.md",
        "doc/plans/PLAN-example.md",
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
