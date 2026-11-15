//! Contracts for the fail-closed lexical path normalizer (plan Decision A9).

use std::path::{Path, PathBuf};

use crate::fs::lexical::normalize_lexically;

#[test]
fn relative_escape_is_refused() {
    assert_eq!(normalize_lexically(Path::new("a/../../b")), None);
    assert_eq!(
        normalize_lexically(Path::new("a/./b/../c")),
        Some(PathBuf::from("a/c"))
    );
}

#[test]
fn absolute_climb_above_root_is_refused() {
    assert_eq!(normalize_lexically(Path::new("/../etc/passwd")), None);
    assert_eq!(
        normalize_lexically(Path::new("/a/../etc")),
        Some(PathBuf::from("/etc"))
    );
}
