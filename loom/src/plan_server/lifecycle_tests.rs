//! Tests for lifecycle lines.

use super::lifecycle::safe;

#[test]
fn safe_preserves_ordinary_text() {
    assert_eq!(safe("project/example"), "project/example");
}

#[test]
fn safe_flattens_untrusted_line_controls() {
    assert_eq!(
        safe("first\n\x1b[31m\u{202e}second `name`"),
        "first [31m second ˋnameˋ"
    );
}

#[test]
fn safe_bounds_long_values() {
    let value = "a".repeat(201);
    assert_eq!(safe(&value), format!("{}…", "a".repeat(199)));
}
