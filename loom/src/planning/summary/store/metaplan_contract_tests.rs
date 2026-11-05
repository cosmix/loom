//! Contract: a feature's only metaplan, `index.md`, has a document summary of its own,
//! `<feature>/metaplan/index`, stored beside the other summaries of the feature and never counted
//! as a member of either overview. A metaplan outside a feature directory has none, and no other
//! `*metaplan*.md` file has one.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{expected_ids, source_of, summary_id_for, summary_path, validate_id, Scope};
use crate::planning::project::Project;

const INDEX_ID: &str = "alpha/metaplan/index";

fn project() -> (TempDir, Project) {
    let dir = TempDir::new().unwrap();
    let files = [
        (
            "doc/product/alpha/index.md",
            "## Specification sections\n| # | Name |\n| - | - |\n| 1 | First |\n",
        ),
        ("doc/product/alpha/section_01.md", "# First\n"),
        (
            "doc/plans/alpha/index.md",
            "## Running (`PLAN-running.md`)\n",
        ),
        (
            "doc/plans/alpha/metaplan-02.md",
            "## Later (`PLAN-later.md`)\n",
        ),
        ("doc/plans/index.md", "## Misplaced (`PLAN-misplaced.md`)\n"),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let project = Project::load(dir.path()).unwrap();
    (dir, project)
}

#[test]
fn a_feature_metaplan_has_a_document_summary() {
    let (_dir, project) = project();

    let ids = expected_ids(&project);
    assert!(
        ids.iter().any(|got| got == INDEX_ID),
        "expected ids lack {INDEX_ID}: {ids:?}"
    );
    let metaplan_ids: Vec<_> = ids.iter().filter(|id| id.contains("metaplan/")).collect();
    assert_eq!(metaplan_ids.len(), 1, "only alpha's index: {ids:?}");
    assert_eq!(
        summary_id_for(&project, Path::new("doc/plans/index.md")),
        None
    );
    assert_eq!(
        summary_id_for(&project, Path::new("doc/plans/alpha/metaplan-02.md")),
        None
    );
    assert_eq!(
        summary_path(INDEX_ID),
        PathBuf::from("doc/loom/summaries/alpha/metaplan/index.md")
    );
    assert_eq!(
        source_of(&project, INDEX_ID),
        Some(PathBuf::from("doc/plans/alpha/index.md"))
    );
    for bad in [
        "alpha/metaplan/../x",
        "alpha/metaplan/Index",
        "alpha/metaplan/metaplan-02",
    ] {
        assert!(validate_id(bad).is_err(), "{bad} is accepted");
    }

    let scope = Scope::of(&project, "overview/alpha/engineering").expect("engineering scope");
    for (name, list) in [
        ("product", &scope.product),
        ("engineering", &scope.engineering),
        ("outside", &scope.outside),
    ] {
        assert!(
            !list.iter().any(|id| id.contains("/metaplan/")),
            "a metaplan summary is in the overview's {name} list: {list:?}"
        );
    }
}
