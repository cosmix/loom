//! Plan filename prefix operations
//!
//! This module handles adding and removing status prefixes
//! (IN_PROGRESS-, DONE-) from plan filenames.

use std::path::{Path, PathBuf};

pub const IN_PROGRESS_PREFIX: &str = "IN_PROGRESS-";
pub const DONE_PREFIX: &str = "DONE-";

/// Add a prefix to the plan filename, preserving the directory
pub fn add_prefix_to_filename(path: &Path, prefix: &str) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("."));
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("plan.md");

    parent.join(format!("{prefix}{filename}"))
}

/// Remove a prefix from the plan filename if present
pub fn remove_prefix_from_filename(path: &Path, prefix: &str) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("."));
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("plan.md");

    if let Some(stripped) = filename.strip_prefix(prefix) {
        parent.join(stripped)
    } else {
        path.to_path_buf()
    }
}

/// Check if the filename has a specific prefix
pub fn has_prefix(path: &Path, prefix: &str) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_prefix_to_filename() {
        let path = PathBuf::from("doc/plans/PLAN-feature.md");
        let result = add_prefix_to_filename(&path, IN_PROGRESS_PREFIX);
        assert_eq!(
            result,
            PathBuf::from("doc/plans/IN_PROGRESS-PLAN-feature.md")
        );
    }

    #[test]
    fn test_add_prefix_preserves_nested_path() {
        let path = PathBuf::from("/home/user/project/doc/plans/PLAN-auth.md");
        let result = add_prefix_to_filename(&path, DONE_PREFIX);
        assert_eq!(
            result,
            PathBuf::from("/home/user/project/doc/plans/DONE-PLAN-auth.md")
        );
    }

    #[test]
    fn test_remove_prefix_from_filename() {
        let path = PathBuf::from("doc/plans/IN_PROGRESS-PLAN-feature.md");
        let result = remove_prefix_from_filename(&path, IN_PROGRESS_PREFIX);
        assert_eq!(result, PathBuf::from("doc/plans/PLAN-feature.md"));
    }

    #[test]
    fn test_remove_prefix_not_present() {
        let path = PathBuf::from("doc/plans/PLAN-feature.md");
        let result = remove_prefix_from_filename(&path, IN_PROGRESS_PREFIX);
        assert_eq!(result, PathBuf::from("doc/plans/PLAN-feature.md"));
    }

    #[test]
    fn test_has_prefix() {
        assert!(has_prefix(
            Path::new("doc/plans/IN_PROGRESS-PLAN.md"),
            IN_PROGRESS_PREFIX
        ));
        assert!(!has_prefix(
            Path::new("doc/plans/PLAN.md"),
            IN_PROGRESS_PREFIX
        ));
        assert!(has_prefix(Path::new("doc/plans/DONE-PLAN.md"), DONE_PREFIX));
    }
}
