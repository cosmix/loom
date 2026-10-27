//! Navigation tree grouping and ordering.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::plan_server::files::tree::build_tree;
use crate::plan_server::wire::files::{
    EntryKind, MetadataView, ValidationItem, ValidationView, ValidationWarnings,
};
use crate::planning::project::Project;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn write_fixture(root: &Path) {
    write(root, "doc/product/index.md", "# Vision\n\n## Specification sections\n\n| Number | Name |\n| --- | --- |\n| 2 | Second |\n| 1 | Missing |\n");
    write(root, "doc/product/section_02.md", "# Second title\n");
    write(root, "doc/product/section_03.md", "# Third title\n");
    write(
        root,
        "doc/plans/metaplan.md",
        "# Roadmap\n\n## 1. `PLAN-alpha.md`\n\n## 2. `PLAN-missing.md`\n",
    );
    write(
        root,
        "doc/plans/PLAN-alpha.md",
        "# Alpha\n\n## Pressure tests\n\n- 2025-01-02: first\n- 2025-02-03: last\n",
    );
    write(root, "doc/plans/briefs/alpha/common.md", "# Common\n");
    write(root, "doc/plans/briefs/alpha/stage/b.md", "# Stage brief\n");
    write(root, "doc/plans/briefs/orphan/common.md", "# Orphan\n");
    write(root, "doc/plans/archive/REVIEW-PLAN-old.md", "# Review\n");
    write(root, "doc/loom/knowledge/INDEX.md", "# Index\n\n## Tier 2 topics\n\n### architecture\n\n| Topic | Blurb | Lines |\n| --- | --- | --- |\n| [overview](architecture/overview.md) | Overview blurb | 4 |\n");
    write(
        root,
        "doc/loom/knowledge/architecture.md",
        "# Architecture\n",
    );
    write(
        root,
        "doc/loom/knowledge/architecture/overview.md",
        "# Overview\n",
    );
    write(root, "doc/misc.md", "no heading\n");
}

fn alpha_validation() -> ValidationView {
    ValidationView {
        valid: false,
        errors: vec![ValidationItem {
            message: "bad".into(),
            stage_id: None,
            line: None,
            column: None,
        }],
        warnings: ValidationWarnings {
            structural: vec![],
            knowledge: vec![],
            sandbox: vec![],
            baseline: vec![],
        },
        notes: vec![],
        metadata: MetadataView::Missing,
    }
}

#[test]
fn groups_documents_and_preserves_listed_order_and_gaps() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_fixture(root);
    let project = Project::load(root).unwrap();
    let validation = alpha_validation();
    let tree = build_tree(
        &project,
        &HashMap::from([("doc/plans/PLAN-alpha.md".into(), validation)]),
    );
    assert_eq!(
        tree.product
            .iter()
            .map(|e| e.title.as_str())
            .collect::<Vec<_>>(),
        ["Vision", "Second title", "Missing", "Third title"]
    );
    assert!(tree.product[2].gap);
    let meta = &tree.plans[0];
    assert_eq!(meta.kind, EntryKind::Metaplan);
    let plan = &meta.children[0];
    assert_eq!(plan.title, "Alpha");
    assert_eq!(plan.pressure_date.as_deref(), Some("2025-02-03"));
    assert_eq!(plan.errors, 1);
    assert_eq!(plan.children[0].title, "Common");
    assert_eq!(plan.children[1].kind, EntryKind::BriefFolder);
    assert_eq!(plan.children[1].children[0].title, "Stage brief");
    assert!(meta.children[1].gap);
    assert_eq!(tree.knowledge[0].title, "Index");
    assert_eq!(tree.knowledge[1].title, "Architecture");
    assert_eq!(
        tree.knowledge[2].children[0].blurb.as_deref(),
        Some("Overview blurb")
    );
    assert!(tree.archive.iter().any(|e| e.kind == EntryKind::Review));
    assert!(tree
        .archive
        .iter()
        .any(|e| e.title == "orphan" && e.children[0].title == "Orphan"));
    assert!(tree.other.iter().any(|e| e.title == "misc.md"));
}
