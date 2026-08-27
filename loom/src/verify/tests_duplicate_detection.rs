//! Tests for duplicate symbol detection.
//!
//! Relocated out of `duplicate_detection.rs` to keep that file under the
//! maintainability line-count baseline; see `loom/maintainability-baseline.txt`.

use super::*;

#[test]
fn test_extract_rust_symbols_pub_fn() {
    let content = "pub fn my_function() {}\npub struct MyStruct {}\n";
    let symbols = extract_with_patterns(content, &RUST_PATTERNS);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"my_function"));
    assert!(names.contains(&"MyStruct"));
}

#[test]
fn test_extract_rust_symbols_private_excluded() {
    let content = "fn private_fn() {}\nstruct PrivateStruct {}\n";
    let symbols = extract_with_patterns(content, &RUST_PATTERNS);
    // Private symbols without `pub` should not be captured by pub patterns
    assert!(symbols.is_empty());
}

#[test]
fn test_extract_rust_symbols_enum_and_trait() {
    let content = "pub enum Color { Red, Green }\npub trait Display {}\n";
    let symbols = extract_with_patterns(content, &RUST_PATTERNS);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Color"));
    assert!(names.contains(&"Display"));
}

#[test]
fn test_extract_ts_symbols() {
    let content =
        "export function greet() {}\nexport class Greeter {}\nexport const VERSION = '1';\n";
    let symbols = extract_with_patterns(content, &TS_PATTERNS);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"greet"));
    assert!(names.contains(&"Greeter"));
    assert!(names.contains(&"VERSION"));
}

#[test]
fn test_extract_python_symbols() {
    let content = "def my_func():\n    pass\nclass MyClass:\n    pass\n";
    let symbols = extract_with_patterns(content, &PYTHON_PATTERNS);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"my_func"));
    assert!(names.contains(&"MyClass"));
}

#[test]
fn test_extract_go_symbols() {
    let content = "func HandleRequest() {}\ntype Server struct {}\n";
    let symbols = extract_with_patterns(content, &GO_PATTERNS);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"HandleRequest"));
    assert!(names.contains(&"Server"));
}

#[test]
fn test_word_boundary_match_exact() {
    assert!(word_boundary_match("pub fn store()", "store"));
}

#[test]
fn test_word_boundary_match_no_match() {
    assert!(!word_boundary_match("pub fn storage()", "store"));
}

#[test]
fn test_word_boundary_match_prefix() {
    assert!(!word_boundary_match("fn restore()", "store"));
}

#[test]
fn test_has_source_extension() {
    assert!(has_source_extension("src/foo.rs"));
    assert!(has_source_extension("src/bar.ts"));
    assert!(has_source_extension("src/baz.py"));
    assert!(!has_source_extension("src/baz.md"));
    assert!(!has_source_extension("src/baz.toml"));
}

#[test]
fn test_noise_names_filtered() {
    // Verify that common noise names are in the list
    assert!(NOISE_NAMES.contains(&"new"));
    assert!(NOISE_NAMES.contains(&"default"));
    assert!(NOISE_NAMES.contains(&"main"));
}

#[test]
fn test_symbol_line_number() {
    let content = "// comment\npub fn first() {}\npub fn second() {}\n";
    let symbols = extract_with_patterns(content, &RUST_PATTERNS);
    let first = symbols.iter().find(|s| s.name == "first").unwrap();
    let second = symbols.iter().find(|s| s.name == "second").unwrap();
    assert_eq!(first.line, 2);
    assert_eq!(second.line, 3);
}

#[test]
fn test_build_grep_pattern_function() {
    let pat = build_grep_pattern("my_func", "function");
    assert!(pat.contains("fn"));
    assert!(pat.contains("def"));
    assert!(pat.contains("function"));
    assert!(pat.contains("my_func"));
}

#[test]
fn test_build_grep_pattern_struct() {
    let pat = build_grep_pattern("MyStruct", "struct");
    assert!(pat.contains("struct"));
    assert!(pat.contains("MyStruct"));
    // struct pattern should not include fn/def
    assert!(!pat.contains("fn"));
}

#[test]
fn test_build_grep_pattern_kind_precision() {
    // enum pattern should only contain enum keyword
    let pat = build_grep_pattern("Color", "enum");
    assert!(pat.contains("enum"));
    assert!(pat.contains("Color"));
    assert!(!pat.contains("fn"));
    assert!(!pat.contains("struct"));
}

// Regression test for a false PASS: grep exits 2 whenever ANY file under the
// search root is unreadable, even when the search itself found real matches.
// The old code treated exit 2 as fatal and discarded stdout, so a symbol that
// genuinely existed elsewhere in the tree was silently reported as "not found"
// whenever the tree also contained an unreadable file (e.g. under a sandbox
// that denies read on dotfiles).
#[cfg(unix)]
#[test]
fn test_find_symbol_survives_unreadable_sibling_file() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let readable = dir.path().join("readable.rs");
    fs::write(&readable, "pub fn target_symbol() {}\n").unwrap();

    let unreadable = dir.path().join("unreadable.rs");
    fs::write(&unreadable, "pub fn other_symbol() {}\n").unwrap();
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();

    // Root (and some sandboxes) ignore file permission bits entirely, in which
    // case this environment cannot exercise the unreadable-file path. Skip
    // loudly rather than silently asserting nothing.
    if fs::read_to_string(&unreadable).is_ok() {
        let _ = fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644));
        eprintln!(
            "SKIP test_find_symbol_survives_unreadable_sibling_file: this \
             environment does not enforce 0o000 file permissions (running as \
             root, or a sandbox that ignores mode bits)"
        );
        return;
    }

    let result = find_symbol_in_codebase(dir.path(), "target_symbol", "function", &[]);

    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644)).unwrap();

    let matches = result.unwrap();
    assert!(
        matches.iter().any(|(path, _)| path == "readable.rs"),
        "expected to find target_symbol in readable.rs despite an unreadable \
         sibling file causing grep to exit 2; matches = {:?}",
        matches
    );
}
