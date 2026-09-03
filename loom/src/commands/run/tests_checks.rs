//! Pure-formatter tests for `checks.rs`.

/// `format_search_tools_warning` is a pure formatter, so its behaviour is
/// checked directly rather than by faking the machine's PATH.
#[test]
fn format_search_tools_warning_empty_is_none() {
    assert!(super::checks::format_search_tools_warning(&[]).is_none());
}

#[test]
fn format_search_tools_warning_names_rg_only() {
    let warning = super::checks::format_search_tools_warning(&["rg (ripgrep)"])
        .expect("a non-empty missing list must produce a warning");
    assert!(warning.contains("rg (ripgrep)"));
    assert!(!warning.contains("fd"));
}

#[test]
fn format_search_tools_warning_names_both() {
    let warning = super::checks::format_search_tools_warning(&["rg (ripgrep)", "fd"])
        .expect("a non-empty missing list must produce a warning");
    assert!(warning.contains("rg (ripgrep)"));
    assert!(warning.contains("fd"));
}
