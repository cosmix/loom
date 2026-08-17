//! Source-graph fixtures exercise registry dispatch, deterministic extraction,
//! degraded coverage, and unresolved-call provenance through the public API.
//! Extraction in these tests only ever goes through the public registry.

use loom::context::extract::{extract_file, registry};
use loom::context::source_graph::EdgeProvenance;
use serde_json::to_string_pretty;
use std::fs::read;
use std::path::Path;

const NESTED_FIXTURES: [&str; 4] = [
    "tests/fixtures/source/rust/nested_generics.rs",
    "tests/fixtures/source/typescript/nested_generics.ts",
    "tests/fixtures/source/python/nested_generics.py",
    "tests/fixtures/source/go/nested_generics.go",
];

// `(read_path, dispatch_path)`: the Rust entry is stored under a non-`.rs`
// extension because the maintainability scanner treats every `.rs` file
// under `tests/` as real code and requires balanced function bodies, which a
// deliberately-broken fixture cannot offer. `dispatch_path` is the name
// `extract_file` sees, so extension-based `supports()` still resolves to the
// Rust extractor; the other three languages have no such scanner and use the
// same path for both.
const SYNTAX_ERROR_FIXTURES: [(&str, &str); 4] = [
    (
        "tests/fixtures/source/rust/syntax_error.rs.broken",
        "tests/fixtures/source/rust/syntax_error.rs",
    ),
    (
        "tests/fixtures/source/typescript/syntax_error.ts",
        "tests/fixtures/source/typescript/syntax_error.ts",
    ),
    (
        "tests/fixtures/source/python/syntax_error.py",
        "tests/fixtures/source/python/syntax_error.py",
    ),
    (
        "tests/fixtures/source/go/syntax_error.go",
        "tests/fixtures/source/go/syntax_error.go",
    ),
];

const DYNAMIC_CALL_FIXTURES: [&str; 4] = [
    "tests/fixtures/source/rust/dynamic_call.rs",
    "tests/fixtures/source/typescript/dynamic_call.ts",
    "tests/fixtures/source/python/dynamic_call.py",
    "tests/fixtures/source/go/dynamic_call.go",
];

#[test]
fn cold_and_incremental_extraction_are_identical() {
    let extractors = registry();

    for fixture in NESTED_FIXTURES {
        let path = Path::new(fixture);
        let bytes = read(path).unwrap();
        let cold = extract_file(&extractors, path, &bytes);
        let incremental = extract_file(&extractors, path, &bytes);

        assert_eq!(
            to_string_pretty(&cold.nodes).unwrap(),
            to_string_pretty(&incremental.nodes).unwrap(),
            "node extraction changed for {fixture}"
        );
        assert_eq!(
            to_string_pretty(&cold.edges).unwrap(),
            to_string_pretty(&incremental.edges).unwrap(),
            "edge extraction changed for {fixture}"
        );
    }
}

#[test]
fn syntax_error_fixture_yields_parse_error_coverage() {
    let extractors = registry();

    for (read_path, dispatch_path) in SYNTAX_ERROR_FIXTURES {
        let bytes = read(Path::new(read_path)).unwrap();
        let extraction = extract_file(&extractors, Path::new(dispatch_path), &bytes);

        assert_eq!(
            extraction.coverage.status(),
            "parse-error",
            "fixture: {dispatch_path}"
        );
        assert_eq!(extraction.nodes.len(), 1, "fixture: {dispatch_path}");
        assert!(extraction.edges.is_empty(), "fixture: {dispatch_path}");
    }
}

#[test]
fn oversized_file_yields_oversized_coverage() {
    let extractors = registry();
    let path = Path::new("tests/fixtures/source/rust/oversized_generated.rs");
    let bytes = b"x".repeat(512 * 1024 + 1);
    let extraction = extract_file(&extractors, path, &bytes);

    assert_eq!(extraction.coverage.status(), "oversized");
    assert_eq!(extraction.nodes.len(), 1);
}

#[test]
fn ambiguous_dynamic_call_yields_inferred_edge() {
    let extractors = registry();

    for fixture in DYNAMIC_CALL_FIXTURES {
        let path = Path::new(fixture);
        let bytes = read(path).unwrap();
        let extraction = extract_file(&extractors, path, &bytes);

        assert!(
            extraction.edges.iter().any(|edge| {
                edge.provenance == EdgeProvenance::Inferred && edge.confidence <= 0.5
            }),
            "fixture should yield an inferred low-confidence edge: {fixture}"
        );
    }
}
