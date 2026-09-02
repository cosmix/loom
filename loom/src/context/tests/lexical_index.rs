//! The persistent lexical index (A.13): that a cache hit scores exactly like a
//! cache miss, and that every way the file on disk can be wrong degrades to the
//! scan instead of to a wrong answer.
//!
//! The property test is the one that matters. The index exists to make a
//! retrieval cheaper, and the only acceptable observable difference between a
//! warm cache and a cold one is latency — a divergence here would surface as
//! "the ranking changed and nobody touched the code", which is close to
//! untrackable in the field.
//!
//! One case the generator cannot reach: a query whose every term is dropped and
//! then rescued (A.16), since an unseen word has a df of 0 and always survives.
//! That agreement is asserted in `rank_stopwords.rs`, over the fixture below.
//!
//! Two neighbours share this file's generators through `use
//! super::lexical_index::...`: `lexical_index_cache.rs` holds the cases about
//! the file on disk being wrong, and `lexical_index_frequencies.rs` the ones
//! about a dropped term's document frequency reaching the exact-rung gate.

use super::source_fixtures::{full_node, graph};
use crate::context::config::RetrievalConfig;
use crate::context::graph_store::ResolvedGraph;
use crate::context::lexical::name_parts;
use crate::context::lexical_index::{source_layer_key, LexicalCache, LEXICAL_RELATIVE_DIR};
use crate::context::rank::{ChannelRanking, RankQuery};
use crate::context::rank_source::{rank_source_channel, rank_source_channel_cached};
use crate::context::schema::{SourceNode, SourceNodeKind};
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
            // Two words, one drawn from the long tail and one unique to this
            // node: `random_query` spells a drawn name out with spaces, and a
            // name has to be reachable that way for the node to clear
            // `rank_source::candidacy`'s floor at all. Both halves are drawn
            // from the tail rather than from `common` because the floor tests
            // name parts against the SURVIVING terms, and a common word is
            // exactly the one stopwording drops. The unique half carries no
            // underscore, so it survives tokenization as a single term.
            let name = format!("{}_uniq{file}x{symbol}", pick(rng, &rare));
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
/// both see varied input — and, three times in four, the words of one node's
/// own name.
///
/// That last part is not decoration. Since `rank_source::candidacy` a node with
/// no exact rung is a candidate only when the prompt supplied every word of its
/// name, and a query drawn purely from the vocabulary names nothing, so the
/// property test would spend every iteration comparing two empty rankings. The
/// name is spelled with SPACES, which admits it through the candidacy floor
/// while leaving the exact-symbol rung unfired — the case worth generating,
/// since a rung would decide candidacy before the floor was ever consulted.
fn random_query(rng: &mut Lcg, graph: &ResolvedGraph) -> RankQuery {
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
    if rng.below(4) != 0 {
        if let Some(name) = random_symbol_name(rng, graph) {
            words.extend(name_parts(&name));
        }
    }
    RankQuery {
        text: words.join(" "),
        ..RankQuery::default()
    }
}

/// One node's terminal scope segment, drawn uniformly. `None` only for a graph
/// with no symbol nodes at all, which [`random_graph`] never produces.
fn random_symbol_name(rng: &mut Lcg, graph: &ResolvedGraph) -> Option<String> {
    let names: Vec<&String> = graph.nodes().filter_map(|node| node.scope.last()).collect();
    (!names.is_empty()).then(|| names[rng.below(names.len())].clone())
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
        let query = random_query(&mut rng, &graph);
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

/// Forty single-node files, eight of them naming and returning both `Manifest`
/// and `Tokens`, against a floor of `max(40 * 0.10, 5) = 5` and a rescue ceiling
/// of 10: both terms of the query `manifest tokens` are ubiquitous enough to
/// drop and rare enough for the rescue floor (A.16) to put back.
///
/// Those eight share ONE name, `manifest_tokens`, so that the rescued terms are
/// also the two words of the name — under `rank_source::candidacy` a node the
/// query has not named is no candidate however its terms were rescued, and a
/// fixture of `item0`-style names would rank nothing at all and quietly compare
/// three empty rankings. Their ids stay distinct because each lives in its own
/// file. The other thirty-two say neither word, which is what keeps both
/// document frequencies inside the rescue window.
pub(super) fn rescued_source_graph() -> ResolvedGraph {
    let mut paths = Vec::new();
    let mut per_file = Vec::new();
    for file in 0..40 {
        let path = format!("src/rescue/m{file}.rs");
        let (name, returns) = if file < 8 {
            ("manifest_tokens", "Manifest")
        } else {
            ("item", "Value")
        };
        per_file.push(vec![full_node(
            &format!("{path}#function:{name}"),
            &path,
            &[name],
            &format!("fn {name}(raw: &str) -> {returns}"),
        )]);
        paths.push(path);
    }
    graph(paths.iter().map(String::as_str).zip(per_file).collect())
}
