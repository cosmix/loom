//! The shared walk's node identity: signature and ordinal suffixes that keep
//! duplicate declarations apart, driven through the Rust grammar.

use std::collections::BTreeMap;

use super::ids::disambiguate;
use super::tests::{function, rust};
use crate::context::extract::FileExtraction;
use crate::context::source_graph::{SourceEdgeKind, SourceNode, SourceNodeKind};

/// Whether `id` is `base` followed by `@` and 8 lowercase hex digits.
fn is_signature_suffixed(id: &str, base: &str) -> bool {
    id.strip_prefix(base)
        .and_then(|rest| rest.strip_prefix('@'))
        .is_some_and(|sig| {
            sig.len() == 8 && sig.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
        })
}

fn nodes_of(extraction: &FileExtraction, kind: SourceNodeKind) -> Vec<&SourceNode> {
    extraction
        .nodes
        .iter()
        .filter(|node| node.kind == kind)
        .collect()
}

#[test]
fn definition_hash_uses_original_bytes_even_with_invalid_utf8() {
    use crate::context::extract::{rust::RustExtractor, SourceGraphExtractor};
    use crate::context::source_graph::body_hash;
    let body = b"fn f() { /* \xff */ }";
    let extraction = RustExtractor::new()
        .extract(std::path::Path::new("src/lib.rs"), body)
        .unwrap();
    let node = nodes_of(&extraction, SourceNodeKind::Function)[0];
    assert_eq!(node.id, function("f"));
    assert_eq!(node.body_hash, body_hash(body));
    assert_eq!(node.signature.as_str(), &*String::from_utf8_lossy(body));
}

#[test]
fn nested_definition_payload_is_bounded() {
    let source = format!(
        "{}/* {} */{}",
        "mod m {\n".repeat(60),
        "x".repeat(100_000),
        "}\n".repeat(60)
    );
    let extraction = rust(&source);
    let definitions: Vec<_> = extraction
        .nodes
        .iter()
        .filter(|node| node.kind != SourceNodeKind::File)
        .collect();
    assert_eq!(definitions.len(), 60);
    let old_body_bytes: usize = definitions
        .iter()
        .map(|node| node.span.end_byte - node.span.start_byte)
        .sum();
    let retained_bytes: usize = definitions
        .iter()
        .map(|node| node.signature.len() + node.body_hash.len())
        .sum();
    eprintln!("definition payload: old body copies {old_body_bytes} bytes; retained signatures and hashes {retained_bytes} bytes");
    assert!(retained_bytes * 100 < old_body_bytes);
}

#[test]
fn duplicate_impl_blocks_get_suffixed_ids_and_suffixed_parents() {
    let extraction = rust(
        "struct W;\nimpl From<u8> for W { fn from(v: u8) -> Self { W } }\n\
         impl From<u16> for W { fn from(v: u16) -> Self { W } }\n",
    );
    let impls = nodes_of(&extraction, SourceNodeKind::Implementation);
    let functions = nodes_of(&extraction, SourceNodeKind::Function);
    assert_eq!(
        (impls.len(), functions.len()),
        (2, 2),
        "{:#?}",
        extraction.nodes
    );

    for node in impls.iter().chain(&functions) {
        let base = format!("src/lib.rs#{}:{}", node.kind, node.scope.join("::"));
        assert!(is_signature_suffixed(&node.id, &base), "id: {}", node.id);
        assert_eq!(node.symbol_key, base, "node: {node:#?}");
    }
    assert_ne!(impls[0].id, impls[1].id);
    assert_ne!(functions[0].id, functions[1].id);

    for (method, owner) in functions.iter().zip(&impls) {
        let parents: Vec<&str> = extraction
            .edges
            .iter()
            .filter(|e| e.kind == SourceEdgeKind::Contains && e.to == method.id)
            .map(|e| e.from.as_str())
            .collect();
        assert_eq!(parents, [owner.id.as_str()], "method: {method:#?}");
    }
}

/// A duplicate's suffix comes from its own signature: neither the order of
/// the declarations nor their line positions enter it.
#[test]
fn a_duplicate_keeps_its_id_across_declaration_order_and_line_position() {
    let from_u8 = "impl From<u8> for W { fn from(v: u8) -> Self { W } }\n";
    let from_u16 = "impl From<u16> for W { fn from(v: u16) -> Self { W } }\n";
    let variants = [
        format!("struct W;\n{from_u8}{from_u16}"),
        format!("struct W;\n{from_u16}{from_u8}"),
        format!("struct W;\n\n\n{from_u8}\n\n{from_u16}"),
        format!("\n\nstruct W;\n{from_u16}\n\n\n\n{from_u8}"),
    ];
    let ids_by_signature = |source: &str| -> BTreeMap<(String, String), String> {
        rust(source)
            .nodes
            .into_iter()
            .filter(|node| !node.symbol_key.is_empty())
            .map(|node| ((node.kind.to_string(), node.signature), node.id))
            .collect()
    };

    let expected = ids_by_signature(variants[0].as_str());
    assert_eq!(expected.len(), 4, "{expected:#?}");
    for source in &variants[1..] {
        assert_eq!(
            ids_by_signature(source.as_str()),
            expected,
            "source:\n{source}"
        );
    }
}

#[test]
fn byte_identical_duplicates_get_ordinal_suffixes() {
    let extraction = rust("#[cfg(unix)]\nfn f() {}\n#[cfg(not(unix))]\nfn f() {}\n");

    let ids: Vec<&str> = nodes_of(&extraction, SourceNodeKind::Function)
        .iter()
        .map(|node| node.id.as_str())
        .collect();

    assert_eq!(ids.len(), 2, "ids: {ids:?}");
    let first = ids[0].strip_suffix(".1").expect("first twin ends .1");
    let second = ids[1].strip_suffix(".2").expect("second twin ends .2");
    assert_eq!(first, second);
    assert!(is_signature_suffixed(first, &function("f")), "id: {first}");
}

#[test]
fn disambiguation_collapses_whitespace_and_keeps_unique_ids() {
    let base = "src/lib.rs#function:f".to_string();
    let settled = disambiguate(&[
        (base.clone(), "fn f(x:  u8)"),
        ("src/lib.rs#function:g".to_string(), "fn g()"),
        (base.clone(), "fn f(x: u8)"),
    ]);

    assert_eq!(
        settled[1],
        ("src/lib.rs#function:g".to_string(), String::new())
    );
    let first = settled[0].0.strip_suffix(".1").expect("first twin ends .1");
    assert_eq!(settled[2].0.strip_suffix(".2"), Some(first));
    assert_eq!(
        (settled[0].1.as_str(), settled[2].1.as_str()),
        (base.as_str(), base.as_str())
    );
}
