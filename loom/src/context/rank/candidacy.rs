//! Which knowledge chunks may be candidates at all, and which of their terms
//! name them — the knowledge channel's counterpart of `rank_source::candidacy`.

use super::{tokenize, LexicalCorpus, RankQuery};
use crate::context::schema::KnowledgeChunk;
use std::collections::BTreeSet;

/// Fewest non-blank body lines a headingless chunk needs to be a candidate.
const STUB_MIN_LINES: usize = 3;

/// Whether `chunk` may be ranked for `query`.
///
/// A chunk with no heading and a body under [`STUB_MIN_LINES`] non-blank lines
/// is a stub: the preamble every knowledge file has before its first `##`,
/// typically a `# Title` and one line of introduction. It answers no question
/// on its own, yet it shares its file's vocabulary, so it rode into packs
/// beside the section that actually answered (`architecture/source-graph.md##0`
/// ranked second for "Honesty Contract"). It stays in the corpus statistics —
/// only candidacy changes — and a caller who demanded its id by hand still gets
/// it.
pub(super) fn admits(query: &RankQuery, chunk: &KnowledgeChunk) -> bool {
    !is_stub(chunk) || query.required_ids.iter().any(|id| id == &chunk.id)
}

fn is_stub(chunk: &KnowledgeChunk) -> bool {
    chunk.heading.is_empty()
        && chunk
            .body
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
            < STUB_MIN_LINES
}

/// How many distinct scored terms `chunk`'s own name carries — the count the
/// prompt hook's emit floor reads as `matched_term_count`.
///
/// Only the heading and aliases count. A prompt that shares two words with a
/// section's body has brushed against it; one that shares two with its heading
/// has named it. Measured on this repository, "no, use the repository version
/// of those files, not the installed copies" matched two or more terms in
/// hundreds of chunks, and two in the heading of none.
pub(super) fn named_terms(corpus: &LexicalCorpus, chunk: &KnowledgeChunk) -> usize {
    corpus.naming_matches(&name_terms(chunk))
}

/// The terms `chunk` is named by: its heading and its aliases, tokenized the
/// way the query was.
fn name_terms(chunk: &KnowledgeChunk) -> BTreeSet<String> {
    let mut terms: BTreeSet<String> = tokenize(&chunk.heading).into_iter().collect();
    for alias in &chunk.aliases {
        terms.extend(tokenize(alias));
    }
    terms
}
