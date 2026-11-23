//! Foundation tests: the guarantees that hold for *every* extractor.
//!
//! Per-language extraction is tested in each language module against its own
//! fixture; what is asserted here is the contract those modules cannot break —
//! a file never vanishes, and a degraded mode is always labelled.

use super::dialect::{dialect_by_id, GrammarPack, DIALECTS};
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
        dialect: "rust",
        grammar_version: "0.24.2",
        query_digest: "sha256:0123456789abcdefdeadbeef".to_string(),
        extractor_version: 3,
    };
    assert_eq!(identity.to_parser_version(), "rust:0.24.2+0123456789ab+v3");
}

#[cfg(feature = "source-graph")]
#[test]
fn every_registered_extractor_claims_a_distinct_dialect() {
    let extractors = registry();
    assert!(
        !extractors.is_empty(),
        "the source-graph feature is on, so the registry must not be empty"
    );

    let mut ids: Vec<&str> = extractors
        .iter()
        .map(|extractor| extractor.dialect().id)
        .collect();
    let before = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(before, ids.len(), "two extractors claim one dialect");
    for extractor in &extractors {
        let dialect = extractor.dialect();
        assert!(
            DIALECTS.iter().any(|row| row.id == dialect.id),
            "{} is not in DIALECTS",
            dialect.id
        );
        assert_eq!(extractor.cache_identity().dialect, dialect.id);
    }
}

#[test]
fn every_extension_appears_in_exactly_one_dialect() {
    let mut seen = std::collections::BTreeMap::new();
    for dialect in DIALECTS {
        for extension in dialect.extensions {
            assert_eq!(*extension, extension.to_ascii_lowercase());
            let previous = seen.insert(*extension, dialect.id);
            assert!(
                previous.is_none(),
                "{extension} claimed by {previous:?} and {}",
                dialect.id
            );
        }
    }
}

#[test]
fn every_dialect_id_is_its_language_name() {
    for dialect in DIALECTS {
        assert_eq!(dialect.id, dialect.language.as_str());
        assert!(dialect_by_id(dialect.id).is_some());
    }
}

#[test]
fn dialect_lookup_lowercases_the_extension() {
    assert_eq!(dialect_for_path(Path::new("a/B.H")).unwrap().id, "cpp");
    assert_eq!(
        dialect_for_path(Path::new("src/Main.RS")).unwrap().id,
        "rust"
    );
    assert!(dialect_for_path(Path::new("notes.md")).is_none());
    assert!(dialect_for_path(Path::new("Makefile")).is_none());
}

#[test]
fn a_known_dialect_without_an_extractor_is_a_named_gap() {
    let extractors = registry();

    if !GrammarPack::WaveB.compiled() {
        let extraction = extract_file(&extractors, Path::new("src/Main.java"), b"class A {}");
        assert_eq!(extraction.nodes.len(), 1);
        assert_eq!(extraction.nodes[0].language, NodeLanguage::Java);
        assert_eq!(
            extraction.nodes[0].parser_version,
            lexical::LEXICAL_PARSER_VERSION
        );
        assert_eq!(
            extraction.coverage,
            FileCoverage::LexicalOnly {
                detail: "grammar pack source-graph-wave-b not compiled in (java)".to_string()
            }
        );
    }

    if !GrammarPack::WaveC.compiled() {
        let extraction = extract_file(&extractors, Path::new("src/run.c"), b"int run(void);");
        assert_eq!(
            extraction.coverage,
            FileCoverage::LexicalOnly {
                detail: "grammar pack source-graph-wave-c not compiled in (c)".to_string()
            }
        );
    }
}

/// A build without the wave-b pack names its dialects as gaps. Only compiled
/// where the feature is off, so `--no-default-features --features source-graph`
/// exercises it.
#[cfg(not(feature = "source-graph-wave-b"))]
#[test]
fn uncompiled_wave_b_pack_is_a_named_gap() {
    let extractors = registry();
    let extraction = extract_file(&extractors, Path::new("src/Main.java"), b"class A {}");

    assert_eq!(extraction.nodes.len(), 1);
    assert_eq!(extraction.nodes[0].language, NodeLanguage::Java);
    assert_eq!(
        extraction.coverage,
        FileCoverage::LexicalOnly {
            detail: "grammar pack source-graph-wave-b not compiled in (java)".to_string()
        }
    );
}

#[test]
fn every_dialect_of_a_compiled_pack_has_exactly_one_extractor() {
    let extractors = registry();
    for dialect in DIALECTS.iter().filter(|dialect| dialect.pack.compiled()) {
        let registered = extractors
            .iter()
            .filter(|extractor| extractor.dialect().id == dialect.id)
            .count();
        assert_eq!(
            registered, 1,
            "dialect {} is not registered once",
            dialect.id
        );
    }
}

#[test]
fn a_gap_and_an_unknown_path_are_told_apart_by_lookup() {
    let extractors = registry();
    assert!(matches!(
        extractor_for(&extractors, Path::new("readme.md")),
        Lookup::Unknown
    ));
    assert!(matches!(
        extractor_for(&extractors, Path::new("src/lib.rs")),
        Lookup::Extractor(_) | Lookup::Gap { .. }
    ));
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
            extractor.dialect().id,
            extraction.err()
        );
    }
}

/// The version pinned (`=x.y.z`) for `crate_name` in the crate manifest.
fn manifest_pin(crate_name: &str) -> String {
    let manifest: toml_edit::DocumentMut = include_str!("../../../Cargo.toml").parse().unwrap();
    let version = manifest["dependencies"][crate_name]["version"]
        .as_str()
        .unwrap_or_else(|| panic!("no pinned version for {crate_name}"));
    version.trim_start_matches('=').to_string()
}

/// Crate name and version, the first two words of a dialect's `grammar`.
fn grammar_crate_and_version(grammar: &str) -> (&str, &str) {
    let mut words = grammar.split_whitespace();
    (words.next().unwrap(), words.next().unwrap())
}

#[test]
fn grammar_versions_match_the_manifest_pins() {
    for dialect in DIALECTS {
        let (crate_name, version) = grammar_crate_and_version(dialect.grammar);
        assert_eq!(
            version,
            manifest_pin(crate_name),
            "{}: grammar text disagrees with the manifest pin",
            dialect.id
        );
    }
}

#[cfg(feature = "source-graph")]
#[test]
fn extractor_grammar_versions_match_the_manifest_pins() {
    for extractor in registry() {
        let (crate_name, _) = grammar_crate_and_version(extractor.dialect().grammar);
        assert_eq!(
            extractor.cache_identity().grammar_version,
            manifest_pin(crate_name),
            "{}: cache identity disagrees with the manifest pin",
            extractor.dialect().id
        );
    }
}
