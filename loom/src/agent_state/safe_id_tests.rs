use super::is_safe_id;

#[test]
fn plain_ids_with_dots_underscores_and_dashes_are_safe() {
    for id in ["a", "agent-a1", "b0.9_x-1", "A9", &"x".repeat(128)] {
        assert!(is_safe_id(id), "{id}");
    }
}

#[test]
fn empty_long_leading_punctuation_and_path_or_control_characters_are_not() {
    let long = "x".repeat(129);
    for id in [
        "", &long, ".hidden", "-flag", "_x", "a/b", "../x", "a b", "a\nb", "a\u{0}b", "é", "a:b",
    ] {
        assert!(!is_safe_id(id), "{id:?}");
    }
}
