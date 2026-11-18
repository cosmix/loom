//! Which query terms are worth scoring: corpus-derived stopwording (A.2), the
//! protection that keeps a corpus's own names out of it, and the rescue floor
//! that keeps it from answering an ordinary question with nothing at all
//! (A.16).
//!
//! Corpus-derived rather than a fixed English list: a list would catch "the"
//! and "is" and stop there, while plenty of the project's own words also sit
//! in most documents of a loom knowledge tree. Deriving the set from document
//! frequency absorbs both classes at once, adapts to whatever corpus it is
//! pointed at, and needs nobody to maintain it.
//!
//! Frequency alone over-reaches, though. Measured on this repository, ten live
//! queries lost `stage`, `merge`, `acceptance`, `worktree`, `context`, `codex`,
//! `plan` and `session` — words the knowledge tree is ABOUT, frequent because
//! so many sections are written on them. A term the corpus uses to NAME a
//! document (a heading, an alias) is therefore protected: never dropped as
//! ubiquitous. The corpus's naming vocabulary also holds every
//! English function word a heading happens to be phrased with ("Why this class
//! is hard to see"), and no statistic separates `the` from `stage` — both fill
//! headings and bodies alike — so protection skips [`FUNCTION_WORDS`], the one
//! place a fixed list is the right tool: that class is closed.
//!
//! Both of [`super`]'s corpus representations — the fresh scan and the warm
//! index — reach [`partition_terms`] with the same document frequencies, the
//! same corpus size, the same naming terms and the same raw query, so a cache
//! hit and a cache miss cannot disagree about which terms survived. That
//! agreement is by construction, not by coincidence: there is one partition,
//! called from both.

use crate::context::config::RetrievalConfig;
use crate::context::lexical::{backtick_spans, occurs_backticked};
use std::collections::{BTreeMap, BTreeSet};

/// How many terms the rescue floor may resurrect at most.
///
/// Three: enough that the rarest content words of an ordinary question come
/// back together, few enough that the rescue can never quietly become a blanket
/// undo of stopwording on a long query. Independent of `min_query_token_len`,
/// which answers an unrelated question (is this token long enough to mean
/// anything at all), and deliberately not a tunable — no property of a corpus
/// makes four right.
///
/// A rescued term ranks, but it never counts toward the prompt hook's emit
/// floor ([`super::LexicalCorpus::naming_matches`]): terms rejected by the
/// normal stopword filter do not establish the evidence needed to emit a hook.
/// A query answered only on rescued terms can still serve context and briefs.
const RESCUE_LIMIT: usize = 3;

/// English function words: never protected, and never evidence that a prompt
/// named a document.
///
/// Closed-class words only — determiners, pronouns, prepositions,
/// conjunctions, auxiliaries and modals (with the stems the tokenizer leaves of
/// their negated contractions: `doesn't` becomes `doesn`), negators, and the
/// pro-form adverbs (`here`, `now`, `never`). A content word stays off this
/// list however generic it is: whether `files` discriminates is the corpus's
/// call, made by document frequency. Sorted, for [`is_function_word`]'s binary
/// search; laid out by hand because one word per line would triple the file.
#[rustfmt::skip]
const FUNCTION_WORDS: &[&str] = &[
    "a", "about", "above", "after", "again", "against", "all", "almost", "also", "although",
    "always", "am", "among", "an", "and", "another", "any", "anybody", "anyone", "anything", "are",
    "aren", "around", "as", "at", "be", "because", "been", "before", "being", "below", "beneath",
    "beside", "besides", "between", "beyond", "both", "but", "by", "can", "cannot", "could",
    "couldn", "did", "didn", "do", "does", "doesn", "doing", "don", "down", "during", "each",
    "either", "else", "enough", "even", "ever", "every", "everybody", "everyone", "everything",
    "except", "few", "for", "from", "further", "had", "hadn", "has", "hasn", "have", "haven",
    "having", "he", "hence", "her", "here", "hers", "herself", "him", "himself", "his", "how",
    "however", "i", "if", "in", "inside", "into", "is", "isn", "it", "its", "itself", "just",
    "least", "less", "let", "many", "may", "me", "might", "more", "most", "much", "must", "mustn",
    "my", "myself", "neither", "never", "no", "nobody", "none", "nor", "not", "nothing", "now",
    "of", "off", "often", "on", "once", "only", "onto", "or", "other", "others", "otherwise",
    "ought", "our", "ours", "ourselves", "out", "over", "own", "per", "perhaps", "quite", "rather",
    "same", "several", "shall", "she", "should", "shouldn", "since", "so", "some", "somebody",
    "someone", "something", "sometimes", "somewhat", "still", "such", "than", "that", "the",
    "their", "theirs", "them", "themselves", "then", "there", "therefore", "these", "they", "this",
    "those", "though", "through", "throughout", "thus", "to", "too", "toward", "towards", "under",
    "underneath", "unless", "unlike", "until", "up", "upon", "us", "very", "via", "was", "wasn",
    "we", "were", "weren", "what", "whatever", "when", "whenever", "where", "whereas", "wherever",
    "whether", "which", "while", "who", "whoever", "whom", "whose", "why", "will", "with",
    "within", "without", "won", "would", "wouldn", "yet", "you", "your", "yours", "yourself",
    "yourselves",
];

/// How many distinct content terms a query must have before a thin surviving
/// set counts as over-stopwording rather than as a precise question.
///
/// Four, because that is where the two readings of a thin survivor separate. A
/// two- or three-word prompt — "Honesty Contract", "reconcile source graph" —
/// that comes out with one surviving term has been served well: the survivor IS
/// the query's discriminating word, and putting its neighbours back would only
/// add noise to a lookup that already knows what it wants. A prompt of four or
/// more content words is a question, and a question reduced to a single generic
/// word has been answered on the least of what it asked. Not a tunable: no
/// property of a corpus makes five right, and the ratios above already give an
/// operator the two knobs that decide what "generic" means here.
const RESCUE_QUERY_MIN_TERMS: usize = 4;

/// Whether `term` is an English function word (see [`FUNCTION_WORDS`]).
pub(super) fn is_function_word(term: &str) -> bool {
    FUNCTION_WORDS.binary_search(&term).is_ok()
}

/// Whether `term` names a document of this corpus, and so is scored however
/// many documents repeat it. `naming_terms` is the corpus's own naming
/// vocabulary (`lexical_index::naming_terms_of`), built once per index build.
fn is_protected(term: &str, naming_terms: &BTreeSet<String>) -> bool {
    naming_terms.contains(term) && !is_function_word(term)
}

/// What [`partition_terms`] decided about one query's terms.
pub(super) struct Partition {
    /// The terms to score, in query order. Repeats survive because BM25 sums a
    /// repeated term twice and that weighting predates this filter.
    pub(super) surviving: Vec<String>,
    /// The terms not scored, deduplicated in first-seen order because they are
    /// shown to a human, and because determinism over identical bytes is a hard
    /// requirement of the whole pipeline.
    pub(super) dropped: Vec<String>,
    /// The surviving terms that only the rescue floor put back.
    pub(super) rescued: BTreeSet<String>,
}

/// Split the tokenized query into the terms worth scoring and the terms that
/// are not.
///
/// A long-enough term survives when it is rare in this corpus OR protected —
/// one of the corpus's own names ([`is_protected`]); a backticked term
/// survives whatever its length or frequency.
pub(super) fn partition_terms(
    query_terms: &[String],
    document_frequencies: &BTreeMap<String, usize>,
    corpus_size: usize,
    naming_terms: &BTreeSet<String>,
    raw_query: &str,
    config: &RetrievalConfig,
) -> Partition {
    let spans = backtick_spans(raw_query);
    let lower_query = raw_query.to_ascii_lowercase();
    let floor = ubiquity_floor(corpus_size, config);

    let mut surviving = Vec::new();
    let mut dropped = Vec::new();
    let mut content_terms = BTreeSet::new();
    for term in query_terms {
        let backticked = occurs_backticked(&lower_query, &spans, term);
        let frequency = document_frequencies.get(term).copied().unwrap_or(0);
        let long_enough = term.len() >= config.min_query_token_len;
        if backticked || long_enough {
            content_terms.insert(term);
        }
        let discriminating = frequency as f32 <= floor || is_protected(term, naming_terms);
        if backticked || (long_enough && discriminating) {
            surviving.push(term.clone());
        } else if !dropped.contains(term) {
            dropped.push(term.clone());
        }
    }

    let mut rescued = BTreeSet::new();
    if over_stopworded(&surviving, content_terms.len(), config) {
        rescued = rescue_rarest(&dropped, document_frequencies, corpus_size, config);
        surviving = readmit(query_terms, &surviving, &rescued);
        // A rescued term was not dropped, and `ContextPack::dropped_terms` is
        // observability an agent reads back — it must describe what this pass
        // actually scored, not what it nearly did.
        dropped.retain(|term| !rescued.contains(term));
    }
    Partition {
        surviving,
        dropped,
        rescued,
    }
}

/// Whether stopwording left this query too thin to answer, and the rescue floor
/// should run.
///
/// Two shapes qualify, and the second is the one measured after the first was
/// fixed. An EMPTY surviving set is unanswerable by construction: nothing is
/// scored, so nothing is a candidate. A surviving set holding fewer DISTINCT
/// terms than `min_knowledge_terms` is a question ranked on one word: every
/// candidate is ordered by that word alone, and no item can match the
/// `min_knowledge_terms` distinct terms the prompt hook's emit floor asks for
/// (`clears_emit_floor`). The rescue puts the question's rarest words back so
/// `loom knowledge context` and a stage brief rank on more of what was asked;
/// the hook stays silent either way, because rescued terms never count toward
/// its floor (see [`RESCUE_LIMIT`]).
///
/// [`RESCUE_QUERY_MIN_TERMS`] is what keeps the second shape from swallowing
/// the first's discipline: it applies only to a query long enough that a
/// surviving set under the floor means the filter ate the question, never to a
/// short precise lookup.
/// Reusing `min_knowledge_terms` rather than a private constant is deliberate —
/// it is the number of distinct terms this pipeline already treats as enough
/// evidence to act on, so an operator who lowers it to 1 correctly turns the
/// thin-survivor rescue off, and one who raises it gets a rescue that aims at
/// the raised count.
fn over_stopworded(surviving: &[String], content_terms: usize, config: &RetrievalConfig) -> bool {
    if surviving.is_empty() {
        return true;
    }
    let distinct: BTreeSet<&String> = surviving.iter().collect();
    content_terms >= RESCUE_QUERY_MIN_TERMS && distinct.len() < config.min_knowledge_terms
}

/// The scored term list after a rescue: everything that already survived, plus
/// everything rescued, in the query's own order.
///
/// Rebuilt from the query rather than concatenated so a term the prompt said
/// twice is still scored twice and the order is the query's — `dropped` is
/// deduplicated for display, `surviving` deliberately is not. A term cannot be
/// in both inputs: the keep decision depends only on the term itself, so every
/// occurrence of one term lands on the same side.
fn readmit(
    query_terms: &[String],
    surviving: &[String],
    rescued: &BTreeSet<String>,
) -> Vec<String> {
    let kept: BTreeSet<&String> = surviving.iter().collect();
    query_terms
        .iter()
        .filter(|term| kept.contains(*term) || rescued.contains(*term))
        .cloned()
        .collect()
}

/// Restore up to `RESCUE_LIMIT` rare dropped terms when the query is
/// over-stopworded. The frequency ceiling excludes ubiquitous terms, including
/// short tokens, and uses the same document frequencies as exact-match gating.
fn rescue_rarest(
    dropped: &[String],
    document_frequencies: &BTreeMap<String, usize>,
    corpus_size: usize,
    config: &RetrievalConfig,
) -> BTreeSet<String> {
    let ceiling = corpus_size as f32 * config.stop_rescue_max_ratio;
    let mut rarest: Vec<(usize, &String)> = dropped
        .iter()
        .map(|term| (document_frequencies.get(term).copied().unwrap_or(0), term))
        .filter(|(frequency, _)| *frequency as f32 <= ceiling)
        .collect();
    // Document frequency ascending, then the term itself, because df alone does
    // not order ties and two runs over identical bytes must rescue an identical
    // set — the same determinism `dropped_terms` is held to.
    rarest.sort_unstable();
    rarest
        .into_iter()
        .take(RESCUE_LIMIT)
        .map(|(_, term)| term.clone())
        .collect()
}

/// Highest document frequency a term may reach and still be scored.
///
/// `corpus_size * stop_df_ratio` is the rule the ratio names, and on a real
/// corpus (thousands of documents) it is the only one that binds. The
/// `df_ident_max` floor exists for the other end: on a five-document corpus the
/// ratio resolves to 0.5, so a term in ONE document is "ubiquitous" and a small
/// knowledge tree would lose lexical retrieval entirely. A term that occurs in
/// at most `df_ident_max` documents is corpus-RARE by the very definition the
/// exact-rung gate uses — calling the same term ubiquitous here would have the
/// two ends of one df map contradict each other.
fn ubiquity_floor(corpus_size: usize, config: &RetrievalConfig) -> f32 {
    (corpus_size as f32 * config.stop_df_ratio).max(config.df_ident_max as f32)
}

#[cfg(test)]
mod tests {
    use super::FUNCTION_WORDS;

    /// `is_function_word` binary-searches the list, which silently misses
    /// entries out of order rather than failing.
    #[test]
    fn function_words_are_sorted_unique_and_lowercase() {
        assert!(FUNCTION_WORDS.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(FUNCTION_WORDS
            .iter()
            .all(|word| word.chars().all(|character| character.is_ascii_lowercase())));
    }
}
