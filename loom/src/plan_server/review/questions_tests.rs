//! Tests for open-question counts.
use super::questions::*;

use std::collections::BTreeMap;
use std::fs;

use crate::planning::project::Project;
use tempfile::TempDir;

fn tree(files: &[(&str, &str)]) -> TempDir {
    let temp = TempDir::new().unwrap();
    for (path, content) in files {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    temp
}

#[test]
fn zero_count_kept_for_empty_heading() {
    let temp = tree(&[
        (
            "doc/product/demo/section_02.md",
            "# Product\n\n## Open questions\n",
        ),
        (
            "doc/plans/demo/index.md",
            "# Metaplan\n\n## Open questions\n- One\n- Two\n",
        ),
        (
            "doc/plans/demo/PLAN-example.md",
            "# Plan\n\n## Open questions\n- One\n",
        ),
    ]);
    let project = Project::load(temp.path()).unwrap();

    assert_eq!(
        open_question_counts(&project),
        BTreeMap::from([
            ("doc/plans/demo/PLAN-example.md".to_string(), 1),
            ("doc/plans/demo/index.md".to_string(), 2),
            ("doc/product/demo/section_02.md".to_string(), 0),
        ])
    );
}

#[test]
fn documents_without_the_heading_omitted() {
    let temp = tree(&[
        ("doc/product/demo/section_01.md", "# Product\n\n## Notes\n"),
        (
            "doc/plans/demo/index.md",
            "# Metaplan\n\n## open questions\n- One\n",
        ),
        ("doc/plans/demo/PLAN-example.md", "# Plan\n"),
        (
            "doc/product/demo/index.md",
            "# Vision\n\n## Open questions\n- One\n",
        ),
        ("doc/other.md", "# Other\n\n## Open questions\n- One\n"),
    ]);
    let project = Project::load(temp.path()).unwrap();

    assert_eq!(open_question_counts(&project), BTreeMap::new());
}
