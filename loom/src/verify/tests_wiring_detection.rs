//! Tests for wiring detection.
//!
//! Relocated out of `wiring_detection.rs` to keep that file under the 400-line
//! limit; see `loom/maintainability-baseline.txt`.

use super::*;

#[test]
fn test_extract_importable_name_nested() {
    assert_eq!(extract_importable_name("src/cache/store.rs"), "store");
}

#[test]
fn test_extract_importable_name_top_level() {
    assert_eq!(extract_importable_name("src/cache.rs"), "cache");
}

#[test]
fn test_extract_importable_name_no_extension() {
    assert_eq!(extract_importable_name("src/cache"), "cache");
}

#[test]
fn test_has_source_extension_rs() {
    assert!(has_source_extension("src/foo.rs"));
}

#[test]
fn test_has_source_extension_ts() {
    assert!(has_source_extension("src/foo.ts"));
}

#[test]
fn test_has_source_extension_txt() {
    assert!(!has_source_extension("src/foo.txt"));
}

#[test]
fn test_is_test_file_tests_dir() {
    assert!(is_test_file("src/tests/foo.rs"));
}

#[test]
fn test_is_test_file_suffix() {
    assert!(is_test_file("src/foo_test.rs"));
}

#[test]
fn test_is_test_file_spec() {
    assert!(is_test_file("src/foo.spec.ts"));
}

#[test]
fn test_is_not_test_file() {
    assert!(!is_test_file("src/cache/store.rs"));
}

#[test]
fn test_excluded_stem_mod() {
    let stem = file_stem("src/cache/mod.rs");
    assert!(EXCLUDED_STEMS.contains(&stem.as_str()));
}

#[test]
fn test_build_search_patterns_rust() {
    let patterns = build_search_patterns("rs", "store");
    assert!(patterns.iter().any(|p| p.contains("mod store")));
    assert!(patterns.iter().any(|p| p.contains("use .*store")));
}

#[test]
fn test_build_search_patterns_typescript() {
    let patterns = build_search_patterns("ts", "client");
    assert!(patterns.iter().any(|p| p.contains("client")));
}

#[test]
fn test_build_search_patterns_python() {
    let patterns = build_search_patterns("py", "utils");
    assert!(patterns.iter().any(|p| p.contains("import utils")));
}

#[test]
fn test_build_search_patterns_go() {
    let patterns = build_search_patterns("go", "cache");
    assert!(patterns.iter().any(|p| p.contains("cache")));
}

#[test]
fn test_is_safe_identifier_valid() {
    assert!(is_safe_identifier("store"));
    assert!(is_safe_identifier("my_module"));
    assert!(is_safe_identifier("Module123"));
}

#[test]
fn test_is_safe_identifier_invalid() {
    assert!(!is_safe_identifier(""));
    assert!(!is_safe_identifier("foo-bar"));
    assert!(!is_safe_identifier("foo.bar"));
    assert!(!is_safe_identifier("foo bar"));
    assert!(!is_safe_identifier("foo*"));
    assert!(!is_safe_identifier("../etc/passwd"));
}

// Regression test for a false "unwired" report: grep exits 2 whenever ANY file
// under the search root is unreadable, even when the search itself found a
// real reference. The old code treated exit 2 as fatal and `continue`d past
// stdout, so a file that genuinely was referenced elsewhere in the tree was
// silently reported as unwired whenever the tree also contained an unreadable
// file (e.g. under a sandbox that denies read on dotfiles).
#[cfg(unix)]
#[test]
fn test_is_referenced_survives_unreadable_sibling_file() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("caller.rs"), "mod target_mod;\n").unwrap();

    let unreadable = dir.path().join("blocked.rs");
    std::fs::write(&unreadable, "mod other;\n").unwrap();
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();

    // Root (and some sandboxes) ignore file permission bits entirely, in which
    // case this environment cannot exercise the unreadable-file path. Skip
    // loudly rather than silently asserting nothing.
    if std::fs::read_to_string(&unreadable).is_ok() {
        let _ = std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644));
        eprintln!(
            "SKIP test_is_referenced_survives_unreadable_sibling_file: this \
             environment does not enforce 0o000 file permissions (running as \
             root, or a sandbox that ignores mode bits)"
        );
        return;
    }

    let result = is_referenced(dir.path(), "target_mod.rs", "target_mod");

    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644)).unwrap();

    assert!(
        result.unwrap(),
        "expected target_mod to be found referenced in caller.rs despite an \
         unreadable sibling file causing grep to exit 2"
    );
}
