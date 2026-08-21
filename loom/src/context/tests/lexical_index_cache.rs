//! Every way the persisted index can be wrong, and the one answer to all of
//! them: fall back to the scan, then repair the file.
//!
//! Split from `lexical_index.rs` — which owns the scan-equals-index property
//! and the corpus generators these reuse — so neither file outgrows the line
//! limit. The generators arrive through `use super::lexical_index::...`, the
//! same way `source_fixtures` reaches its siblings.
//!
//! Not one of these tests asserts an `Err`. A retrieval that returns an error
//! because a cache file was unreadable would cost the user their context to
//! save them some latency, so every failure here has to be invisible except in
//! a `tracing::debug!` line.

use super::lexical_index::{assert_identical, fixture_graph, index_path};
use super::source_fixtures::{full_node, graph};
use crate::context::config::RetrievalConfig;
use crate::context::graph_store::ResolvedGraph;
use crate::context::lexical_index::{
    source_layer_key, IndexChannel, LexicalCache, LexicalIndex, LEXICAL_RELATIVE_DIR,
};
use crate::context::rank::{ChannelRanking, RankQuery};
use crate::context::rank_source::{rank_source_channel, rank_source_channel_cached};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Parse the index currently on disk, so a test can doctor one field of it.
fn read_index(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// A file from a future (or past) schema is a miss, and the miss repairs it.
#[test]
fn a_version_mismatch_falls_back_to_the_scan_and_rewrites() {
    let (temp, graph, config, scanned) = warm_cache();
    let path = index_path(temp.path(), &graph);
    let mut stored = read_index(&path);
    stored["version"] = json!(2);
    fs::write(&path, stored.to_string()).unwrap();

    let cache = LexicalCache::source(temp.path(), &graph);
    let recovered = rank_source_channel_cached(&query_text(), &graph, &config, Some(&cache));

    assert_identical(&scanned, &recovered, "version mismatch");
    assert_eq!(
        read_index(&path)["version"],
        json!(1),
        "the miss must rewrite the index in the current schema"
    );
}

/// A file whose recorded revision is not the one being asked for describes some
/// other corpus, whatever its name says.
#[test]
fn a_revision_mismatch_falls_back_to_the_scan_and_rewrites() {
    let (temp, graph, config, scanned) = warm_cache();
    let path = index_path(temp.path(), &graph);
    let mut stored = read_index(&path);
    stored["revision"] = json!("not-the-revision");
    fs::write(&path, stored.to_string()).unwrap();

    let cache = LexicalCache::source(temp.path(), &graph);
    let recovered = rank_source_channel_cached(&query_text(), &graph, &config, Some(&cache));

    assert_identical(&scanned, &recovered, "revision mismatch");
    assert_eq!(
        read_index(&path)["revision"],
        json!(source_layer_key(&graph)),
        "the rewritten file must carry the revision its name embeds"
    );
}

/// Valid JSON that is structurally wrong is the dangerous case: a binary search
/// over an out-of-range or unsorted postings list does not fail loudly, it
/// scores plausibly and differently.
#[test]
fn a_structurally_corrupt_index_falls_back_to_the_scan() {
    let (temp, graph, config, scanned) = warm_cache();
    let path = index_path(temp.path(), &graph);
    let mut stored = read_index(&path);
    let term = stored["postings"]
        .as_object()
        .and_then(|postings| postings.keys().next().cloned())
        .expect("the fixture corpus has at least one term");
    stored["postings"][&term][0][0] = json!(10_000);
    fs::write(&path, stored.to_string()).unwrap();

    let cache = LexicalCache::source(temp.path(), &graph);
    let recovered = rank_source_channel_cached(&query_text(), &graph, &config, Some(&cache));

    assert_identical(&scanned, &recovered, "posting outside the corpus");
}

/// A half-written file — a crash mid-write, a full disk — is a miss, not a
/// parse error escaping into the caller.
#[test]
fn a_truncated_index_falls_back_to_the_scan() {
    let (temp, graph, config, scanned) = warm_cache();
    let path = index_path(temp.path(), &graph);
    let stored = fs::read_to_string(&path).unwrap();
    fs::write(&path, &stored[..stored.len() / 2]).unwrap();

    let cache = LexicalCache::source(temp.path(), &graph);
    let recovered = rank_source_channel_cached(&query_text(), &graph, &config, Some(&cache));

    assert_identical(&scanned, &recovered, "truncated index");
}

/// Writing an index bounds its own channel to `KEEP_INDEXES` files and leaves
/// the other channel's alone.
///
/// The count is what is asserted, never WHICH file survives: eviction is by
/// modification time, and eight writes inside one filesystem timestamp tick all
/// tie. Bounded-but-unspecified is the honest contract, and it is the one that
/// matters — the alternative, keeping exactly the file just written, would have
/// each parallel stage unlink every sibling stage's index on every prompt.
#[test]
fn writing_an_index_bounds_its_own_channel_only() {
    let temp = TempDir::new().unwrap();
    let lexical = temp.path().join(LEXICAL_RELATIVE_DIR);
    let foreign = lexical.join("knowledge-deadbeef.json");

    for revision in 0..8 {
        let name = format!("rev{revision}");
        let cache = LexicalCache::new(temp.path(), IndexChannel::Source, &name);
        cache.save(&LexicalIndex::build(&name, &["only"], &[Vec::new()]));
        if revision == 0 {
            fs::write(&foreign, "{}").unwrap();
        }
    }

    let surviving = fs::read_dir(&lexical)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("source-"))
        .count();
    assert_eq!(surviving, 6, "the channel must be bounded, not unbounded");
    assert!(
        lexical.join("source-rev7.json").is_file(),
        "the file just written is never the one evicted"
    );
    assert!(
        foreign.is_file(),
        "the knowledge channel's index is not this channel's to delete"
    );
}

/// The negative control for the entire suite: doctor a WEIGHT in an otherwise
/// valid index and the score must change.
///
/// Everything else here asserts "falls back to the scan", which is exactly what
/// a `load` that always returned `None` would do. This is the one assertion that
/// fails if the indexed branch is never taken.
#[test]
fn a_doctored_posting_weight_changes_the_score() {
    let (temp, graph, config, scanned) = warm_cache();
    let path = index_path(temp.path(), &graph);
    let mut stored = read_index(&path);
    assert!(
        stored["postings"].get("manifest").is_some(),
        "the fixture corpus must contain the term this test doctors"
    );
    stored["postings"]["manifest"][0][1] = json!(2000.0f32.to_bits());
    fs::write(&path, stored.to_string()).unwrap();

    let cache = LexicalCache::source(temp.path(), &graph);
    let doctored = rank_source_channel_cached(&query_text(), &graph, &config, Some(&cache));

    assert!(
        !scanned.candidates.is_empty(),
        "the fixture must rank something for this comparison to mean anything"
    );
    assert!(
        scanned.candidates != doctored.candidates,
        "scoring ignored the persisted weights - the indexed path never ran"
    );
}

/// The key covers the file BYTES, not just the file names: an overlay that
/// rewrote a file without adding or removing one must not be served the old
/// index.
#[test]
fn the_source_key_covers_content_and_extractor_not_just_paths() {
    let baseline = keyed_graph("sha256:one", "parser+v1");

    assert_ne!(
        source_layer_key(&baseline),
        source_layer_key(&keyed_graph("sha256:two", "parser+v1")),
        "changed file content must change the key"
    );
    assert_ne!(
        source_layer_key(&baseline),
        source_layer_key(&keyed_graph("sha256:one", "parser+v2")),
        "a re-extraction that derived different tokens from the same bytes \
         must change the key"
    );
}

/// One file, one node, with both identity inputs under the caller's control.
fn keyed_graph(content_hash: &str, parser_version: &str) -> ResolvedGraph {
    let mut node = full_node("src/a.rs#function:run", "src/a.rs", &["run"], "fn run()");
    node.parser_version = parser_version.to_string();
    let mut graph = graph(vec![("src/a.rs", vec![node])]);
    if let Some(entry) = graph.files.get_mut("src/a.rs") {
        entry.content_hash = content_hash.to_string();
    }
    graph
}

/// A cache root that cannot hold a directory — read-only checkout, sandbox,
/// full disk — costs latency and nothing else.
#[test]
fn an_unwritable_cache_directory_degrades_to_the_scan() {
    let temp = TempDir::new().unwrap();
    let graph = fixture_graph();
    let config = RetrievalConfig::default();
    // A plain FILE where the directory belongs: unwritable for every user,
    // including the root a CI container often runs as, which a permission bit
    // would not be.
    fs::write(temp.path().join(LEXICAL_RELATIVE_DIR), "not a directory").unwrap();

    let cache = LexicalCache::source(temp.path(), &graph);
    let scanned = rank_source_channel(&query_text(), &graph, &config);
    let attempted = rank_source_channel_cached(&query_text(), &graph, &config, Some(&cache));

    assert_identical(&scanned, &attempted, "unwritable cache");
}

/// Passing no cache is the scan, unchanged. The fallback is the correctness
/// oracle for everything above, so it has to stay the default rather than
/// become code that only runs once a cache file is deleted.
#[test]
fn no_cache_is_the_scan_path() {
    let graph = fixture_graph();
    let config = RetrievalConfig::default();

    let direct = rank_source_channel(&query_text(), &graph, &config);
    let explicit_none = rank_source_channel_cached(&query_text(), &graph, &config, None);

    assert!(
        !direct.candidates.is_empty(),
        "the fixture must rank something"
    );
    assert_identical(&direct, &explicit_none, "no cache");
}

/// Two channels at the same revision write two different files, so one channel
/// can never be served the other's corpus however the keys happen to collide.
#[test]
fn the_two_channels_use_distinct_file_names() {
    let temp = TempDir::new().unwrap();
    let knowledge = LexicalCache::new(temp.path(), IndexChannel::Knowledge, "abc123");
    let source = LexicalCache::new(temp.path(), IndexChannel::Source, "abc123");
    assert_eq!(knowledge.revision(), source.revision());

    knowledge.save(&LexicalIndex::build("abc123", &["only"], &[Vec::new()]));

    let lexical = temp.path().join(LEXICAL_RELATIVE_DIR);
    assert!(lexical.join("knowledge-abc123.json").is_file());
    assert!(!lexical.join("source-abc123.json").exists());
    assert!(
        source.load(&["only"]).is_none(),
        "the source channel must not read the knowledge channel's index"
    );
}

/// A revision that would escape the cache directory is folded into a plain file
/// name instead.
#[test]
fn a_revision_cannot_escape_the_cache_directory() {
    let temp = TempDir::new().unwrap();
    let cache = LexicalCache::new(temp.path(), IndexChannel::Source, "../../etc/passwd");

    cache.save(&LexicalIndex::build(
        cache.revision(),
        &["only"],
        &[Vec::new()],
    ));

    let written: Vec<String> = fs::read_dir(temp.path().join(LEXICAL_RELATIVE_DIR))
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        written.len(),
        1,
        "one file, and it is inside the cache: {written:?}"
    );
    assert!(!written[0].contains('/') && !written[0].contains(".."));
    assert_eq!(
        cache.revision(),
        "../../etc/passwd",
        "the file NAME is folded; the revision itself is stored and compared verbatim"
    );
    assert!(
        cache.load(&["only"]).is_some(),
        "a folded name must still round-trip its own revision"
    );
}

/// The shared setup: a warm cache over [`fixture_graph`], plus the scan result
/// every "the file is wrong" test compares against.
fn warm_cache() -> (TempDir, ResolvedGraph, RetrievalConfig, ChannelRanking) {
    let temp = TempDir::new().unwrap();
    let graph = fixture_graph();
    let config = RetrievalConfig::default();
    let cache = LexicalCache::source(temp.path(), &graph);

    let scanned = rank_source_channel(&query_text(), &graph, &config);
    rank_source_channel_cached(&query_text(), &graph, &config, Some(&cache));

    (temp, graph, config, scanned)
}

/// One fixed query, shared by the failure-mode tests so their assertions are
/// about the file on disk and nothing else.
fn query_text() -> RankQuery {
    RankQuery {
        text: "manifest tokens".to_string(),
        ..RankQuery::default()
    }
}
