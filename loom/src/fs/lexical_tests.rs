use std::path::{Path, PathBuf};

use super::normalize_lexically;

fn normal(path: &str) -> Option<PathBuf> {
    normalize_lexically(Path::new(path))
}

#[test]
fn current_dir_alone_and_cancelled_components_are_empty() {
    assert_eq!(normal("."), Some(PathBuf::new()));
    assert_eq!(normal("a/.."), Some(PathBuf::new()));
    assert_eq!(normal(""), Some(PathBuf::new()));
}

#[test]
fn leading_parent_of_relative_path_is_refused() {
    assert_eq!(normal(".."), None);
    assert_eq!(normal("../x"), None);
    assert_eq!(normal("./../x"), None);
    assert_eq!(normal("a/../../x"), None);
}

#[test]
fn root_is_kept_and_parent_of_root_is_refused() {
    assert_eq!(normal("/"), Some(PathBuf::from("/")));
    assert_eq!(normal("/.."), None);
    assert_eq!(normal("/a/../.."), None);
    assert_eq!(normal("/a/./b/../c"), Some(PathBuf::from("/a/c")));
}

#[test]
fn repeated_and_trailing_separators_fold() {
    assert_eq!(normal("a//b/"), Some(PathBuf::from("a/b")));
    assert_eq!(normal("a/b/./"), Some(PathBuf::from("a/b")));
}

#[test]
fn parent_after_parent_pops_twice() {
    assert_eq!(normal("a/b/c/../../d"), Some(PathBuf::from("a/d")));
}
