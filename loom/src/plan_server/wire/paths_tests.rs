//! Tests for wire paths.

use super::paths::{to_doc_rel, to_repo_rel};

#[test]
fn strips_only_the_leading_doc_directory() {
    assert_eq!(to_doc_rel("doc/plans/PLAN-x.md"), Some("plans/PLAN-x.md"));
    assert_eq!(to_doc_rel("doc/"), Some(""));
    assert_eq!(to_doc_rel("doc/doc/file.md"), Some("doc/file.md"));
    for path in ["", "doc", "docs/file.md", "/doc/file.md", "src/doc/file.md"] {
        assert_eq!(to_doc_rel(path), None, "{path}");
    }
}

#[test]
fn repo_relative_paths_round_trip() {
    for path in ["", "plans/PLAN-x.md", "doc/file.md"] {
        let repo_rel = to_repo_rel(path);
        assert_eq!(repo_rel, format!("doc/{path}"));
        assert_eq!(to_doc_rel(&repo_rel), Some(path));
    }
}
