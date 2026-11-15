//! Tests for `link_path`, the relative-link resolution of `crate::planning::xref`.

use std::path::{Path, PathBuf};

use super::link_path;

const DOC: &str = "doc/product/demo/section_02.md";

fn link(raw: &str) -> Option<PathBuf> {
    link_path(Path::new(DOC), raw)
}

#[test]
fn link_climbing_above_the_repository_root_is_not_a_doc_link() {
    assert_eq!(link("../../../../doc/product/demo/section_01.md"), None);
    assert_eq!(link("../../../../../doc/x.md#rules"), None);
}

#[test]
fn link_climbing_within_the_repository_resolves() {
    assert_eq!(
        link("../other/./section_01.md#rules"),
        Some(PathBuf::from("doc/product/other/section_01.md"))
    );
    assert_eq!(
        link("../../../doc/plans/PLAN-a.md"),
        Some(PathBuf::from("doc/plans/PLAN-a.md"))
    );
    assert_eq!(link("#rules"), Some(PathBuf::from(DOC)));
}

#[test]
fn link_leaving_the_doc_tree_is_not_a_doc_link() {
    assert_eq!(link("../../../src/main.rs"), None);
}
