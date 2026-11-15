//! Fail-closed lexical path normalization: the one place `.` and `..` are folded without
//! reading the filesystem.

use std::path::{Component, Path, PathBuf};

/// `path` with `.` dropped and each `..` popping the previous normal component; a leading
/// `/` stays. `None` when a `..` would climb above the start of a relative path or above `/`
/// of an absolute one, or when the path carries a Windows prefix: every caller is a
/// containment or matching check, and folding `../x` into `x` would move an outside path
/// inside. `.` and `a/..` give an empty path. Symlinks are not followed: a caller that must
/// catch a `..` after a link compares canonical forms itself.
pub(crate) fn normalize_lexically(path: &Path) -> Option<PathBuf> {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) => return None,
            Component::RootDir => normal.push(Component::RootDir),
            Component::CurDir => {}
            Component::ParentDir => {
                if !matches!(normal.components().next_back(), Some(Component::Normal(_))) {
                    return None;
                }
                normal.pop();
            }
            Component::Normal(part) => normal.push(part),
        }
    }
    Some(normal)
}

#[cfg(test)]
#[path = "lexical_tests.rs"]
mod tests;
