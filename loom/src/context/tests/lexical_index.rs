//! The persistent lexical index (A.13): that a cache hit scores exactly like a
//! cache miss, and that every way the file on disk can be wrong degrades to the
//! scan instead of to a wrong answer.
//!
//! The property test is the one that matters. The index exists to make a
//! retrieval cheaper, and the only acceptable observable difference between a
//! warm cache and a cold one is latency — a divergence here would surface as
//! "the ranking changed and nobody touched the code", which is close to
//! untrackable in the field.

use super::source_fixtures::{full_node, graph};
use crate::context::config::RetrievalConfig;
use crate::context::graph_store::ResolvedGraph;
use crate::context::lexical_index::{
    source_layer_key, LexicalCache, LexicalIndex, LEXICAL_RELATIVE_DIR,
};
use crate::context::rank::{tokenize, ChannelRanking, RankQuery};
use crate::context::rank_source::{rank_source_channel, rank_source_channel_cached};
use crate::context::schema::{SelectionReason, SourceNode, SourceNodeKind};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Words drawn on to build both corpora and queries, so terms repeat within a
/// document (exercising the weight accumulation), across documents (exercising
/// document frequency and stopwording) and between corpus and query.
const COMMON: &[&str] = &["cache", "stage", "signal", "context"];

/// Size of the long tail. A uniform vocabulary would give every term nearly the
/// same document frequency, so the one candidacy floor would either drop all of
/// them or none — and half the property test's iterations would be a comparison
/// of two empty rankings. A few ubiquitous words plus a wide tail gets both
/// paths, dropped and surviving, on every corpus.
const RARE_WORDS: usize = 48;

/// Words that appear in queries but never in a corpus, so the `df == 0` arm is
/// hit too.
const UNSEEN: &[&str] = &["beryllium", "quicksilver"];

/// The long tail, built rather than spelled out. `zeta7` tokenizes to itself —
/// no underscore to split on, no camel-case boundary — so a generated word
/// behaves exactly like the plain English word a real prompt would carry.
fn rare_vocabulary() -> Vec<String> {
    (0..RARE_WORDS)
        .map(|index| format!("zeta{index}"))
        .collect()
}

/// A 64-bit linear congruential generator (the constants are Knuth's MMIX).
///
/// Hand-rolled rather than pulled in as a dependency: the property test needs
/// reproducibility and nothing else — no distributions, no cryptographic
/// quality — and a seeded LCG in the test file makes a failure replayable by
/// pasting the seed, with no new crate in the tree.
pub(super) struct Lcg(u64);

impl Lcg {
    pub(super) fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn step(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    /// A value in `0..bound`, taken from the high bits because an LCG's low
    /// bits have short periods.
    fn below(&mut self, bound: usize) -> usize {
        (self.step() >> 33) as usize % bound
    }
}

fn pick<'a>(rng: &mut Lcg, words: &'a [String]) -> &'a str {
    words[rng.below(words.len())].as_str()
}

fn common_vocabulary() -> Vec<String> {
    COMMON.iter().map(|word| (*word).to_string()).collect()
}

/// A graph of `files` files, each with one to three symbol nodes.
///
/// Every node draws from both vocabularies, in scope AND in its signature, so a
/// term routinely occurs more than once in one document at two different
/// weights (`WEIGHT_SYMBOLS` and `WEIGHT_BODY`) — which is the accumulation the
/// index has to reproduce exactly.
pub(super) fn random_graph(rng: &mut Lcg, files: usize) -> ResolvedGraph {
    let common = common_vocabulary();
    let rare = rare_vocabulary();
    let mut paths: Vec<String> = Vec::new();
    let mut per_file: Vec<Vec<SourceNode>> = Vec::new();
    for file in 0..files {
        let path = format!("src/generated/module_{file}.rs");
        let node_count = 1 + rng.below(3);
        let mut nodes = Vec::new();
        for symbol in 0..node_count {
            let name = format!("symbol_{file}_{symbol}");
            let mut scope = vec![pick(rng, &common).to_string(), pick(rng, &rare).to_string()];
            scope.push(name.clone());
            let mut words: Vec<&str> = Vec::new();
            for _ in 0..2 + rng.below(4) {
                words.push(pick(rng, &common));
            }
            for _ in 0..1 + rng.below(3) {
                words.push(pick(rng, &rare));
            }
            let scope_refs: Vec<&str> = scope.iter().map(String::as_str).collect();
            nodes.push(full_node(
                &format!("{path}#function:{name}"),
                &path,
                &scope_refs,
                &words.join(" "),
            ));
        }
        paths.push(path);
        per_file.push(nodes);
    }
    graph(paths.iter().map(String::as_str).zip(per_file).collect())
}

/// A query mixing ubiquitous words (which stopwording drops), tail words (which
/// survive and actually rank something), the occasional word no corpus contains
/// and the occasional backticked one, so the partition and the exact-rung gate
/// both see varied input.
fn random_query(rng: &mut Lcg) -> RankQuery {
    let common = common_vocabulary();
    let rare = rare_vocabulary();
    let unseen: Vec<String> = UNSEEN.iter().map(|word| (*word).to_string()).collect();
    let mut words = Vec::new();
    for slot in 0..3 + rng.below(3) {
        let word = match slot {
            0 => pick(rng, &common),
            _ if rng.below(8) == 0 => pick(rng, &unseen),
            _ => pick(rng, &rare),
        };
        if rng.below(4) == 0 {
            words.push(format!("`{word}`"));
        } else {
            words.push(word.to_string());
        }
    }
    RankQuery {
        text: words.join(" "),
        ..RankQuery::default()
    }
}

/// The document identities `rank_source` derives, in corpus order — the same
/// filter it applies, so a test can ask the cache whether it accepts the file
/// the ranker just wrote.
pub(super) fn doc_ids(graph: &ResolvedGraph) -> Vec<&str> {
    graph
        .nodes()
        .filter(|node| !matches!(node.kind, SourceNodeKind::File))
        .map(|node| node.id.as_str())
        .collect()
}

/// A small, fixed corpus with a term that occurs several times in one document
/// and in more than one document, and that survives the candidacy floor. The
/// failure-mode tests rank against this rather than a random graph so their
/// assertions are about the file on disk and never about which words the
/// generator happened to draw.
pub(super) fn fixture_graph() -> ResolvedGraph {
    graph(vec![
        (
            "src/alpha.rs",
            vec![full_node(
                "src/alpha.rs#function:parse_manifest",
                "src/alpha.rs",
                &["manifest", "parse_manifest"],
                "fn parse_manifest(raw: &str) -> Manifest",
            )],
        ),
        (
            "src/beta.rs",
            vec![full_node(
                "src/beta.rs#function:render_manifest",
                "src/beta.rs",
                &["manifest", "render_manifest"],
                "fn render_manifest(manifest: &Manifest) -> String",
            )],
        ),
        (
            "src/gamma.rs",
            vec![full_node(
                "src/gamma.rs#function:collect_tokens",
                "src/gamma.rs",
                &["tokens", "collect_tokens"],
                "fn collect_tokens(text: &str) -> Vec<Token>",
            )],
        ),
    ])
}

pub(super) fn index_path(root: &Path, graph: &ResolvedGraph) -> PathBuf {
    root.join(LEXICAL_RELATIVE_DIR)
        .join(format!("source-{}.json", source_layer_key(graph)))
}

/// Assert two rankings are the same answer, down to the bit pattern of every
/// score.
///
/// `PartialEq` on `f32` would call `0.0` and `-0.0` equal and would let a
/// one-ULP drift through on any comparison that happened to be reordered, so
/// the scores are compared as bits on top of the structural equality.
pub(super) fn assert_identical(expected: &ChannelRanking, actual: &ChannelRanking, context: &str) {
    assert_eq!(
        expected.dropped_terms, actual.dropped_terms,
        "{context}: dropped terms diverged"
    );
    assert_eq!(
        expected.candidates, actual.candidates,
        "{context}: candidates diverged"
    );
    for (expected, actual) in expected.candidates.iter().zip(&actual.candidates) {
        assert_eq!(
            expected.score.to_bits(),
            actual.score.to_bits(),
            "{context}: score for {} differs in its bit pattern",
            expected.id.as_str()
        );
    }
}

/// The property: over fifty generated corpora and queries, a warm index and a
/// cold scan produce the same candidates, in the same order, with the same
/// scores and the same `matched_term_count`.
#[test]
fn indexed_scoring_equals_scan_scoring_over_random_corpora() {
    let config = RetrievalConfig::default();
    let mut rng = Lcg::new(0x5eed_0000_a13a_13a1);
    let mut ranked_something = 0;

    for iteration in 0..50 {
        let files = 2 + rng.below(9);
        let graph = random_graph(&mut rng, files);
        let query = random_query(&mut rng);
        let temp = TempDir::new().unwrap();
        let cache = LexicalCache::source(temp.path(), &graph);

        let scanned = rank_source_channel(&query, &graph, &config);
        let miss = rank_source_channel_cached(&query, &graph, &config, Some(&cache));
        let hit = rank_source_channel_cached(&query, &graph, &config, Some(&cache));

        let context = format!("iteration {iteration}, query {:?}", query.text);
        assert!(
            index_path(temp.path(), &graph).is_file(),
            "{context}: the miss must have written the index"
        );
        // Without this the whole test would still pass if `load` always
        // returned `None`: `hit` would be a second miss, and a second scan is
        // trivially equal to the first.
        assert!(
            cache.load(&doc_ids(&graph)).is_some(),
            "{context}: the file just written must be accepted back"
        );
        assert_identical(&scanned, &miss, &format!("{context} (miss)"));
        assert_identical(&scanned, &hit, &format!("{context} (hit)"));
        if !scanned.candidates.is_empty() {
            ranked_something += 1;
        }
    }

    assert!(
        ranked_something >= 10,
        "only {ranked_something}/50 iterations ranked anything — the generator \
         has drifted into comparing empty lists"
    );
}

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
