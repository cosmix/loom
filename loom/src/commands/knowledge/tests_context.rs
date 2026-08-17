//! Tests for `commands/knowledge/context.rs`.

use super::*;
// `reject_unknown_require_ids` moved into the shared retrieval pipeline when
// this command was refactored onto it; the flag it guards is still this
// command's, so its tests stay here.
use crate::context::retrieve::reject_unknown_require_ids;
use crate::context::schema::{KnowledgeChunk, LifecycleState};
use crate::fs::knowledge::catalog::Catalog;
use std::path::PathBuf;

/// A minimal chunk with only the id populated — enough to exercise
/// `reject_unknown_require_ids`, which only looks at `chunk.id`.
fn chunk_with_id(id: &str) -> KnowledgeChunk {
    KnowledgeChunk {
        id: id.to_string(),
        file: PathBuf::from(format!("{id}.md")),
        anchor: String::new(),
        heading: String::new(),
        body: String::new(),
        content_hash: String::new(),
        estimated_tokens: 0,
        aliases: Vec::new(),
        category: None,
        source_paths: Vec::new(),
        symbols: Vec::new(),
        links: Vec::new(),
        state: LifecycleState::Active,
    }
}

fn catalog_with_ids(ids: &[&str]) -> Catalog {
    Catalog {
        revision: "test-revision".to_string(),
        chunks: ids.iter().map(|id| chunk_with_id(id)).collect(),
        issues: Vec::new(),
    }
}

#[test]
fn parse_scope_is_case_insensitive_and_names_every_channel() {
    assert_eq!(parse_scope("knowledge").unwrap(), vec![Channel::Knowledge]);
    assert_eq!(parse_scope("SOURCE").unwrap(), vec![Channel::Source]);
    assert_eq!(parse_scope("all").unwrap(), Channel::all().to_vec());
}

#[test]
fn parse_scope_rejects_an_unknown_channel_by_name() {
    let error = parse_scope("everything").unwrap_err();
    assert!(
        error.to_string().contains("everything"),
        "the error should name the rejected scope, got: {error}"
    );
}

#[test]
fn reject_unknown_require_ids_allows_every_id_present() {
    let catalog = catalog_with_ids(&["a", "b"]);
    let result = reject_unknown_require_ids(&catalog, &["a".to_string(), "b".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn reject_unknown_require_ids_allows_an_empty_list() {
    let catalog = catalog_with_ids(&["a"]);
    let result = reject_unknown_require_ids(&catalog, &[]);
    assert!(result.is_ok());
}

#[test]
fn reject_unknown_require_ids_fails_on_one_unknown_id() {
    let catalog = catalog_with_ids(&["a"]);
    let error = reject_unknown_require_ids(&catalog, &["missing".to_string()]).unwrap_err();
    assert!(error.to_string().contains("missing"));
}

#[test]
fn reject_unknown_require_ids_names_every_unknown_id_in_a_single_error() {
    let catalog = catalog_with_ids(&["a"]);
    let error = reject_unknown_require_ids(
        &catalog,
        &["first-missing".to_string(), "second-missing".to_string()],
    )
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("first-missing") && message.contains("second-missing"),
        "expected a single error naming both unknown ids, got: {message}"
    );
}
