//! Request construction for [`super::retrieve_for_stage`]: the id check that runs
//! before ranking and the packer's request built after it.

use super::{reject_unknown_require_ids, StageQuery};
use crate::context::graph_store::ResolvedGraph;
use crate::context::pack::PackRequest;
use crate::context::rank_source::intent::{classify, QueryIntent};
use crate::context::schema::{Channel, TextSearchHint};
use crate::context::store::StoreState;
use crate::fs::knowledge::catalog::Catalog;
use anyhow::Result;

/// Reject a `--require-id` this query's scope cannot possibly hold.
///
/// Offer each corpus only when its channel will be ranked.
pub(super) fn check_require_ids(
    query: &StageQuery,
    catalog: &Catalog,
    graph: Option<&ResolvedGraph>,
) -> Result<()> {
    let source_graph = query
        .scope
        .contains(&Channel::Source)
        .then_some(graph)
        .flatten();
    let knowledge = query.scope.contains(&Channel::Knowledge).then_some(catalog);
    reject_unknown_require_ids(knowledge, source_graph, &query.required_ids)
}

pub(super) fn build_pack_request(
    query: &StageQuery,
    budget_tokens: usize,
    state: StoreState,
    dropped_terms: Vec<String>,
    surviving_terms: Vec<String>,
    degraded: Option<String>,
) -> PackRequest {
    PackRequest {
        query: query.text.clone(),
        scope: query.scope.clone(),
        budget_tokens,
        structural_freshness: state.structural,
        semantic_freshness: state.semantic,
        dropped_terms,
        surviving_terms,
        required_representation: query.required_representation,
        degraded,
        text_search: literal_search_hint(&query.text),
    }
}

/// The text-search hint for a literal query: the graph indexes no bodies, so
/// quoted text is answered by searching for it.
fn literal_search_hint(query_text: &str) -> Option<TextSearchHint> {
    match classify(query_text) {
        QueryIntent::Literal { text } => Some(TextSearchHint::for_pattern(&text)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::graph_store::FileEntry;
    use crate::context::schema::{
        FileCoverage, KnowledgeChunk, LifecycleState, NodeLanguage, SourceNode, SourceNodeKind,
        Span,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    fn corpora() -> (Catalog, ResolvedGraph) {
        let catalog = Catalog {
            revision: "test".into(),
            issues: vec![],
            chunks: vec![KnowledgeChunk {
                id: "knowledge-id".into(),
                file: "a.md".into(),
                anchor: String::new(),
                heading: String::new(),
                body: String::new(),
                content_hash: String::new(),
                estimated_tokens: 0,
                aliases: vec![],
                category: None,
                source_paths: vec![],
                symbols: vec![],
                links: vec![],
                state: LifecycleState::Active,
            }],
        };
        let node = SourceNode {
            id: "source-id".into(),
            kind: SourceNodeKind::Function,
            path: PathBuf::from("a.rs"),
            scope: vec![],
            span: Span::default(),
            signature: String::new(),
            body_hash: String::new(),
            language: NodeLanguage::Rust,
            parser_version: String::new(),
            coverage: FileCoverage::Full,
            symbol_key: String::new(),
        };
        let graph = ResolvedGraph {
            base_revision: "test".into(),
            overlaid: BTreeSet::new(),
            files: BTreeMap::from([(
                "a.rs".into(),
                FileEntry {
                    content_hash: String::new(),
                    nodes: vec![node],
                    edges: vec![],
                    coverage: FileCoverage::Full,
                    imports: vec![],
                },
            )]),
        };
        (catalog, graph)
    }

    #[test]
    fn required_ids_must_belong_to_a_requested_channel() {
        let (catalog, graph) = corpora();
        let mut query = StageQuery::new(".", "query");
        query.scope = vec![Channel::Source];
        query.required_ids = vec!["knowledge-id".into()];
        assert!(check_require_ids(&query, &catalog, Some(&graph)).is_err());
        query.scope = vec![Channel::Knowledge];
        query.required_ids = vec!["source-id".into()];
        assert!(check_require_ids(&query, &catalog, Some(&graph)).is_err());
        query.scope = Channel::all().to_vec();
        query.required_ids = vec!["knowledge-id".into(), "source-id".into()];
        assert!(check_require_ids(&query, &catalog, Some(&graph)).is_ok());
    }

    #[test]
    fn admitted_required_item_over_budget_is_reported() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        std::fs::create_dir_all(root.join(".loom/work")).unwrap();
        let knowledge = root.join("doc/loom/knowledge");
        std::fs::create_dir_all(&knowledge).unwrap();
        std::fs::write(
            knowledge.join("architecture.md"),
            format!(
                "# Architecture\n\n## Topic\n\n{}",
                "orchestrator ".repeat(500)
            ),
        )
        .unwrap();
        let mut query = StageQuery::new(root, "orchestrator");
        query.scope = vec![Channel::Knowledge];
        let discovery = super::super::retrieve_for_stage(&query, 10_000).unwrap();
        let id = discovery.items[0].id.as_str().to_owned();
        query.required_ids = vec![id.clone()];
        let pack = super::super::retrieve_for_stage(&query, 200).unwrap();
        assert!(pack
            .unmet_required
            .iter()
            .any(|item| item.id.as_str() == id));
    }
}
