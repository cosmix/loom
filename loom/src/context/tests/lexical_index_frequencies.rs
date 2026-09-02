//! The document-frequency coupling between the persistent lexical index and
//! the exact-rung gate (A.13), split out of `lexical_index.rs` to keep both
//! files under the size limit. The generators and the comparison helper arrive
//! through `use super::lexical_index::...`; the parity property test and the
//! index round-trip cases stay there.
//!
//! What these two assert is one fact from both ends. `lexical::ExactGate` reads
//! the document-frequency map to decide whether a candidate's NAME is
//! corpus-rare, and stopwording drops a ubiquitous term from scoring before the
//! gate ever runs. Nothing in the type system connects the two, so an index
//! that forgot a dropped term's frequency would readmit exactly the rung the
//! gate exists to reject — and only on a cache hit, which is the hardest kind
//! of divergence to notice in the field.

use super::lexical_index::assert_identical;
use super::source_fixtures::{full_node, graph};
use crate::context::config::RetrievalConfig;
use crate::context::graph_store::ResolvedGraph;
use crate::context::lexical_index::{LexicalCache, LexicalIndex};
use crate::context::rank::{tokenize, RankQuery};
use crate::context::rank_source::{rank_source_channel, rank_source_channel_cached};
use crate::context::schema::SelectionReason;
use tempfile::TempDir;

/// A corpus term the query mentions must keep its document frequency in the
/// map even after stopwording drops it, because `lexical::ExactGate` reads that
/// same map to decide whether a candidate's NAME is corpus-rare. Asserted
/// directly: nothing in the type system connects the two ends.
#[test]
fn dropped_terms_keep_their_document_frequencies_through_the_index() {
    let documents: Vec<Vec<(String, f32)>> = (0..20)
        .map(|_| vec![("point".to_string(), 2.0), ("write".to_string(), 1.0)])
        .collect();
    let ids: Vec<String> = (0..20).map(|index| format!("node-{index}")).collect();
    let doc_ids: Vec<&str> = ids.iter().map(String::as_str).collect();

    let index = LexicalIndex::build("rev", &doc_ids, &documents);
    let frequencies = index.document_frequencies(&tokenize("the point is written"));

    assert_eq!(
        frequencies.get("point"),
        Some(&20),
        "a term ubiquitous enough to be dropped must still report its real df"
    );
    assert_eq!(
        frequencies.get("the"),
        Some(&0),
        "a query term absent from the corpus is recorded as 0, not omitted"
    );
    assert!(
        !frequencies.contains_key("write"),
        "the map covers query terms only; widening it to the whole vocabulary \
         would change what the exact-rung gate rejects"
    );
}

/// The same coupling, end to end: a symbol named after a corpus-ubiquitous word
/// must not claim an exact-symbol rung on a cache hit when it cannot claim one
/// on a scan.
#[test]
fn a_ubiquitous_symbol_name_claims_no_rung_on_a_cache_hit() {
    let config = RetrievalConfig::default();
    let temp = TempDir::new().unwrap();
    let graph = ubiquitous_symbol_graph();
    let query = RankQuery {
        text: "the point is in collect_tokens".to_string(),
        ..RankQuery::default()
    };
    let cache = LexicalCache::source(temp.path(), &graph);

    let scanned = rank_source_channel(&query, &graph, &config);
    rank_source_channel_cached(&query, &graph, &config, Some(&cache));
    let hit = rank_source_channel_cached(&query, &graph, &config, Some(&cache));

    assert!(
        scanned.dropped_terms.contains(&"point".to_string()),
        "the fixture must actually drop the term: {:?}",
        scanned.dropped_terms
    );
    assert_identical(&scanned, &hit, "ubiquitous symbol name");
    assert!(
        !hit.candidates.is_empty(),
        "the rare symbol must rank, or the assertion below tests nothing"
    );
    assert!(
        hit.candidates
            .iter()
            .filter(|candidate| candidate.id.as_str().ends_with(":point"))
            .all(|candidate| !candidate.reasons.contains(&SelectionReason::ExactSymbol)),
        "a df of 0 for a dropped term would readmit the rung the gate exists to reject"
    );
}

/// Sixty files whose every symbol is named `point` — a real file stem and a
/// real English word — plus one corpus-rare symbol so the ranking is not empty.
fn ubiquitous_symbol_graph() -> ResolvedGraph {
    let mut paths = Vec::new();
    let mut per_file = Vec::new();
    for file in 0..60 {
        let path = format!("src/generated/file_{file}.rs");
        let node = full_node(
            &format!("{path}#function:point"),
            &path,
            &["point"],
            "fn point()",
        );
        paths.push(path);
        per_file.push(vec![node]);
    }
    paths.push("src/generated/rare.rs".to_string());
    per_file.push(vec![full_node(
        "src/generated/rare.rs#function:collect_tokens",
        "src/generated/rare.rs",
        &["collect_tokens"],
        "fn collect_tokens()",
    )]);
    graph(paths.iter().map(String::as_str).zip(per_file).collect())
}
