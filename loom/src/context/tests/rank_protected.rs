//! Protected terms, stub chunks and the naming count the prompt hook's emit
//! floor reads (`rank/corpus/stopwords.rs`, `rank/candidacy.rs`).

use super::lexical_index::assert_identical;
use super::rank_fixtures::{chunk, TERMLESS_HEADING};
use crate::context::config::RetrievalConfig;
use crate::context::lexical_index::LexicalCache;
use crate::context::rank::{rank_channel, rank_channel_cached, ChannelRanking, RankQuery};
use crate::context::schema::{Channel, KnowledgeChunk};
use tempfile::TempDir;

/// One hundred sections, sixty of whose bodies say `stage` and ninety `the` —
/// both far over the ubiquity floor of ten — with `heading` on the first.
fn corpus_with_first_heading(heading: &str) -> Vec<KnowledgeChunk> {
    (0..100)
        .map(|index| {
            let mut body = String::from("section body");
            if index < 90 {
                body.push_str(" the");
            }
            if index < 60 {
                body.push_str(" stage");
            }
            let mut section = chunk(&format!("chunk-{index:03}"), &body, 10);
            if index == 0 {
                section.heading = heading.to_string();
            }
            section
        })
        .collect()
}

fn rank_knowledge(text: &str, corpus: &[KnowledgeChunk]) -> ChannelRanking {
    rank_channel(
        &query(text),
        corpus,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    )
}

fn query(text: &str) -> RankQuery {
    RankQuery {
        text: text.to_string(),
        ..RankQuery::default()
    }
}

#[test]
fn a_heading_term_survives_a_corpus_where_it_is_ubiquitous() {
    let protected = rank_knowledge("stage", &corpus_with_first_heading("Stage merge rules"));
    let unprotected = rank_knowledge("stage", &corpus_with_first_heading(TERMLESS_HEADING));

    assert!(
        protected.dropped_terms.is_empty(),
        "{:?}",
        protected.dropped_terms
    );
    assert_eq!(protected.candidates.len(), 60);
    assert_eq!(
        unprotected.dropped_terms,
        vec!["stage".to_string()],
        "the control: with no heading naming it, the same term is dropped"
    );
}

#[test]
fn a_function_word_in_a_heading_is_still_dropped() {
    let ranking = rank_knowledge("the stage", &corpus_with_first_heading("The stage"));

    assert_eq!(ranking.dropped_terms, vec!["the".to_string()]);
    assert_eq!(ranking.candidates.len(), 60);
}

/// The naming terms a warm index carries must protect exactly what the scan
/// protects, or a cache hit would drop a term a miss scores.
#[test]
fn protection_agrees_warm_and_cold() {
    let corpus = corpus_with_first_heading("Stage merge rules");
    let config = RetrievalConfig::default();
    let temp = TempDir::new().unwrap();
    let cache = LexicalCache::knowledge(temp.path(), "protected");
    let stage = query("the stage");

    let scanned = rank_channel(&stage, &corpus, Channel::Knowledge, &config);
    let miss = rank_channel_cached(&stage, &corpus, Channel::Knowledge, &config, Some(&cache));
    let hit = rank_channel_cached(&stage, &corpus, Channel::Knowledge, &config, Some(&cache));

    assert_eq!(scanned.candidates.len(), 60);
    assert_identical(&scanned, &miss, "protected term (miss)");
    assert_identical(&scanned, &hit, "protected term (hit)");
}

#[test]
fn a_stub_chunk_is_not_a_candidate_unless_required() {
    let mut stub = chunk("guide.md##0", "# Guide\n\nThe quokka tour.", 10);
    stub.heading = String::new();
    let mut preamble = chunk("notes.md##0", "# Notes\n\nThe quokka tour.\nMore.", 10);
    preamble.heading = String::new();
    let corpus = vec![stub, preamble];

    let ranked = rank_knowledge("quokka tour", &corpus);
    let required = rank_channel(
        &RankQuery {
            required_ids: vec!["guide.md##0".to_string()],
            ..query("quokka tour")
        },
        &corpus,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    );

    let ids: Vec<&str> = ranked
        .candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["notes.md##0"],
        "three non-blank lines is not a stub"
    );
    assert!(required
        .candidates
        .iter()
        .any(|candidate| candidate.id.as_str() == "guide.md##0"));
}

#[test]
fn matched_term_count_counts_only_what_the_heading_or_aliases_name() {
    let mut named = chunk("merges", "phantom stage branch merged quokka", 10);
    named.heading = "Phantom merges".to_string();
    named.aliases = vec!["quokka".to_string()];
    let mut brushed = chunk("brushed", "phantom stage branch merged", 10);
    brushed.heading = "Unrelated topic".to_string();

    let ranked = rank_knowledge("phantom merges stage branch quokka", &[named, brushed]);

    let count = |id: &str| {
        ranked
            .candidates
            .iter()
            .find(|candidate| candidate.id.as_str() == id)
            .unwrap_or_else(|| panic!("{id} is a candidate: {ranked:?}"))
            .matched_term_count
    };
    assert_eq!(count("merges"), 3, "phantom, merges, and the alias quokka");
    assert_eq!(count("brushed"), 0, "body matches name nothing");
}

/// Two-letter words are dropped for length, then put back by the rescue floor
/// when nothing else survives. A heading that spells them still does not count
/// them: a prompt answered on rescued terms names nothing.
#[test]
fn a_rescued_term_never_counts_as_naming_a_chunk() {
    let mut corpus: Vec<KnowledgeChunk> = (0..9)
        .map(|index| chunk(&format!("filler-{index}"), "filler words", 10))
        .collect();
    let mut named = chunk("ci-db", "ci db", 10);
    named.heading = "CI DB".to_string();
    corpus.push(named);

    let ranked = rank_knowledge("ci db", &corpus);

    assert!(
        ranked.dropped_terms.is_empty(),
        "both terms are rescued: {:?}",
        ranked.dropped_terms
    );
    assert_eq!(ranked.candidates.len(), 1);
    assert_eq!(ranked.candidates[0].matched_term_count, 0);
}
