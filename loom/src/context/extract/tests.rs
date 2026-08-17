//! Foundation tests: the guarantees that hold for *every* extractor.
//!
//! Per-language extraction is tested in each language module against its own
//! fixture; what is asserted here is the contract those modules cannot break —
//! a file never vanishes, and a degraded mode is always labelled.

use super::*;
use crate::context::source_graph::MAX_EXTRACTED_FILE_BYTES;
use std::path::Path;

#[test]
fn an_unsupported_language_keeps_a_file_level_node() {
    let extractors = registry();
    let extraction = extract_file(&extractors, Path::new("docs/readme.md"), b"# Title\n");

    assert_eq!(extraction.nodes.len(), 1);
    assert_eq!(extraction.nodes[0].kind, SourceNodeKind::File);
    assert_eq!(extraction.nodes[0].id, "docs/readme.md");
    assert!(extraction.edges.is_empty());
    assert_eq!(extraction.coverage.status(), "lexical-only");
    assert!(!extraction.coverage.has_symbols());
}

#[test]
fn an_oversized_file_keeps_metadata_instead_of_disappearing() {
    let extractors = registry();
    let bytes = vec![b'\n'; MAX_EXTRACTED_FILE_BYTES + 1];
    let extraction = extract_file(&extractors, Path::new("src/generated.rs"), &bytes);

    assert_eq!(extraction.nodes.len(), 1);
    assert_eq!(extraction.nodes[0].id, "src/generated.rs");
    assert!(!extraction.nodes[0].body_hash.is_empty());
    match extraction.coverage {
        FileCoverage::Oversized { bytes, limit } => {
            assert_eq!(bytes, MAX_EXTRACTED_FILE_BYTES + 1);
            assert_eq!(limit, MAX_EXTRACTED_FILE_BYTES);
        }
        other => panic!("expected oversized coverage, got {other:?}"),
    }
}

#[test]
fn a_file_node_hashes_the_whole_file() {
    let extractors = registry();
    let extraction = extract_file(&extractors, Path::new("a.unknown"), b"hello");
    assert_eq!(
        extraction.nodes[0].body_hash,
        crate::context::source_graph::body_hash(b"hello")
    );
}

#[test]
fn an_empty_file_still_produces_one_node_with_a_valid_span() {
    let extractors = registry();
    let extraction = extract_file(&extractors, Path::new("empty.unknown"), b"");

    assert_eq!(extraction.nodes.len(), 1);
    let span = extraction.nodes[0].span;
    assert_eq!(span.start_byte, 0);
    assert_eq!(span.end_byte, 0);
    assert_eq!(span.line_start, 1);
    assert_eq!(span.line_end, 1);
}

#[test]
fn parser_version_encodes_grammar_query_and_extractor_revision() {
    let identity = ExtractorIdentity {
        grammar_version: "0.24.2",
        query_digest: "sha256:0123456789abcdefdeadbeef".to_string(),
        extractor_version: 3,
    };
    assert_eq!(identity.to_parser_version(), "0.24.2+0123456789ab+v3");
}

#[cfg(feature = "source-graph")]
#[test]
fn every_registered_extractor_claims_a_distinct_language() {
    let extractors = registry();
    assert!(
        !extractors.is_empty(),
        "the source-graph feature is on, so the registry must not be empty"
    );

    let mut languages: Vec<String> = extractors
        .iter()
        .map(|extractor| extractor.language().canonical_name().to_string())
        .collect();
    let before = languages.len();
    languages.sort();
    languages.dedup();
    assert_eq!(before, languages.len(), "two extractors claim one language");
}

#[cfg(feature = "source-graph")]
#[test]
fn every_registered_extractor_has_a_compilable_query() {
    // A malformed query is a build-time authoring error that would otherwise
    // only surface as a degraded extraction at runtime.
    for extractor in registry() {
        let extraction = extractor.extract(Path::new("probe.txt"), b"");
        assert!(
            extraction.is_ok(),
            "{} failed on empty input: {:?}",
            extractor.language(),
            extraction.err()
        );
    }
}
