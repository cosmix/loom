//! Boundary tests for the `sun_path` length check ahead of the daemon's
//! socket bind (see `socket_limit.rs`). Exercised at 103/104/105 bytes
//! rather than a realistic path, since every realistic loom socket path
//! passes and would prove nothing about the boundary itself.

use super::{socket_path_fits, SUN_PATH_MAX};
use std::path::PathBuf;

fn path_of_len(len: usize) -> PathBuf {
    PathBuf::from("a".repeat(len))
}

#[test]
fn fits_one_byte_under_the_limit() {
    let path = path_of_len(SUN_PATH_MAX - 1);
    assert_eq!(path.as_os_str().len(), 103);
    assert!(socket_path_fits(&path));
}

#[test]
fn rejects_exactly_at_the_limit() {
    // At exactly SUN_PATH_MAX bytes there is no room left for the kernel's
    // NUL terminator, so this must NOT fit.
    let path = path_of_len(SUN_PATH_MAX);
    assert_eq!(path.as_os_str().len(), 104);
    assert!(!socket_path_fits(&path));
}

#[test]
fn rejects_one_byte_over_the_limit() {
    let path = path_of_len(SUN_PATH_MAX + 1);
    assert_eq!(path.as_os_str().len(), 105);
    assert!(!socket_path_fits(&path));
}
