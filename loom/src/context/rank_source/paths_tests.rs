use super::*;

#[test]
fn joins_working_dir_to_relative_path() {
    assert_eq!(
        join_working_dir(Some("loom"), "src/a.rs"),
        Some("loom/src/a.rs".to_string())
    );
}

#[test]
fn drops_dot_prefix_and_dot_working_dir() {
    assert_eq!(
        join_working_dir(None, "./src/a.rs"),
        Some("src/a.rs".to_string())
    );
    assert_eq!(
        join_working_dir(Some("."), "src/a.rs"),
        Some("src/a.rs".to_string())
    );
}

#[test]
fn resolves_parent_segments_within_root() {
    assert_eq!(
        join_working_dir(Some("loom"), "../web/x.ts"),
        Some("web/x.ts".to_string())
    );
}

#[test]
fn rejects_parent_segments_above_root() {
    assert_eq!(join_working_dir(Some("loom"), "../../x"), None);
}

#[test]
fn rejects_absolute_and_empty_paths() {
    assert_eq!(join_working_dir(None, "/abs/x"), None);
    assert_eq!(join_working_dir(Some("loom"), ""), None);
}

#[test]
fn passes_glob_characters_through() {
    assert_eq!(
        join_working_dir(Some("loom"), "src/**/*.rs"),
        Some("loom/src/**/*.rs".to_string())
    );
}

#[test]
fn normalizes_backslashes_and_trailing_slash_in_working_dir() {
    assert_eq!(
        join_working_dir(Some("loom/"), "src\\a.rs"),
        Some("loom/src/a.rs".to_string())
    );
}
