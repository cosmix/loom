//! Why a query occurrence is allowed to claim an exact-match rung.
//!
//! The exact rungs (`ExactSymbol`, and the bare-stem arm of `ExactPath` —
//! [`crate::context::schema::SelectionReason`]) search the RAW prompt for a candidate's own
//! name with identifier boundaries — see `contains_whole_term`'s doc in the
//! parent module for why boundaries and not substrings. Boundaries fixed the
//! *candidate* side: a symbol named `n` stopped matching "generation". Nothing
//! fixed the *query* side, and that is the larger hole: an ordinary English
//! word in a prompt still earns an 80-point boost and a `high` confidence label
//! whenever some symbol happens to be spelled the same way. Measured, all real:
//! "why doesn't loom repair --fix do it, the point is" pulled in `lerpPoint`,
//! `repairGini` and `type Point`; "write the recommendation in
//! /home/dkaponis/src/loom/doc" pulled in five `write` helpers and three
//! `home()` functions; "improve … performance … quality" pulled in
//! `BALANCED_GLOBE_QUALITY`.
//!
//! [`TermEvidence`] is the fix: before a match may claim an exact rung, the
//! occurrence has to look like a code reference. Three independent signals
//! admit it, and any one is enough because each is, on its own, evidence the
//! writer meant a symbol rather than a word:
//!
//! - **backticked** — the writer explicitly marked it as code;
//! - **shaped** — the name itself cannot be an English word (`snake_case`,
//!   `camelCase`, `Foo::Bar`);
//! - **rare** — the name occurs in so few documents that it cannot be the
//!   ordinary vocabulary of this corpus, AND it is not spelled like an ordinary
//!   word ([`is_plain_word`]).
//!
//! That second half of the rarity test is the one this file learned last, and
//! the hole it closes was measured: `read the remaining knowledge files that
//! are relevant` put `daemon/server/admission.rs#function:remaining` FIRST, at
//! 94.7 points, because `remaining` occurs in few enough source nodes to look
//! corpus-rare. Rarity cannot tell an uncommon symbol from an uncommon English
//! word — only spelling can, and `remaining` is spelled like a word. Rarity is
//! therefore left to do the job it was added for: admitting a name the
//! tokenizer never emits as a term at all, because it carries punctuation or
//! digits (`Foo::Bar`, `kebab-case`, `sha256`). Such a name is absent from the
//! document-frequency map, counts as frequency `0`, and is not something a
//! prompt says by accident.
//!
//! Nothing here excludes a candidate from the results. A word that fails every
//! test still competes on its BM25 score; it just cannot buy an 80-point boost
//! and a `high` label with a coincidence.

use crate::context::lexical::whole_term_ranges;
use std::collections::BTreeMap;

/// Why a query occurrence is allowed to claim an exact rung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermEvidence {
    /// The occurrence fell inside a `` ` ``-delimited span in the raw prompt.
    pub backticked: bool,
    /// The matched name is identifier-shaped: contains `_`, or an interior
    /// lowercase→uppercase transition (camelCase), or `::`.
    pub shaped: bool,
    /// Document frequency in this channel's corpus is at most `df_ident_max`,
    /// and the name is not spelled like an ordinary word ([`is_plain_word`]).
    pub rare: bool,
}

/// Whether this evidence admits an exact rung at all.
pub fn admits_exact(evidence: &TermEvidence) -> bool {
    evidence.backticked || evidence.shaped || evidence.rare
}

/// True when the name itself is identifier-shaped.
///
/// Deliberately a property of the NAME and not of the prompt: it is what makes
/// the check cheap enough to run per candidate (8,500 source nodes on a large
/// repo) rather than per prompt position, and it is the only one of the three
/// signals that survives a writer who types a symbol without backticks in the
/// middle of a sentence.
///
/// The camelCase rule matches `tokenize`'s own split rule byte for byte
/// (`lexical.rs`, the `is_ascii_lowercase` → `is_ascii_uppercase` scan), so a
/// name the tokenizer treats as multi-word is exactly a name this treats as
/// shaped. A leading capital alone is NOT shaped: `Point`, `Widget` and `Bar`
/// are all ordinary English words that Rust happens to capitalize.
pub fn is_shaped(name: &str) -> bool {
    if name.contains('_') || name.contains("::") {
        return true;
    }
    let bytes = name.as_bytes();
    (1..bytes.len())
        .any(|index| bytes[index - 1].is_ascii_lowercase() && bytes[index].is_ascii_uppercase())
}

/// True when `name` is spelled the way an English word is: ASCII letters only,
/// none of them capital.
///
/// The complement of what [`ExactGate::is_rare`] may vouch for. Rarity is a
/// statement about a CORPUS ("few documents hold this"), and an uncommon
/// English word satisfies it exactly as well as an uncommon symbol does —
/// `remaining`, `relevant` and `complete` are all rare in a corpus of function
/// signatures. Spelling is the only evidence that separates them, so a name
/// that survives lowercasing unchanged and holds nothing but letters is left to
/// the other two signals: back-tick it, or spell it the way code is spelled.
///
/// A capital anywhere makes the name non-plain, `Widget` included. That is
/// weaker evidence than [`is_shaped`] demands, deliberately: `is_shaped` is
/// asking whether a name is UNAMBIGUOUSLY code and answers no for a leading
/// capital, while this is only asking whether rarity is allowed to speak at
/// all, and a capitalized name in a lowercase prompt is at least a spelling the
/// writer did not have to use.
pub fn is_plain_word(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|character| character.is_ascii_lowercase())
}

/// Byte ranges of the `` `…` `` spans in `raw`, scanned once.
///
/// Backticks are paired in the order they appear — first with second, third
/// with fourth — and each span covers the bytes BETWEEN its pair, excluding the
/// backticks themselves. An unclosed final backtick opens nothing.
///
/// A ```` ``` ```` fence therefore yields one empty span plus one span over the
/// fenced body, which is the right answer for the only question asked here:
/// everything inside a fence is code, so an occurrence there is backticked.
pub fn backtick_spans(raw: &str) -> Vec<(usize, usize)> {
    let ticks: Vec<usize> = raw
        .bytes()
        .enumerate()
        .filter(|(_, byte)| *byte == b'`')
        .map(|(index, _)| index)
        .collect();
    ticks
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[open, close]| (open + 1, close))
        .collect()
}

/// True when some identifier-boundary occurrence of `term` in `lower_text`
/// falls entirely inside one of `spans`.
///
/// `spans` are byte ranges taken from the RAW prompt while the occurrence is
/// found in its ASCII-lowercased copy. The two agree because
/// `str::to_ascii_lowercase` maps only `A-Z` to `a-z` and so preserves every
/// byte offset — a full `to_lowercase()` would not (`İ` is two bytes lowercased
/// to three) and would silently shift every span.
pub(crate) fn occurs_backticked(lower_text: &str, spans: &[(usize, usize)], term: &str) -> bool {
    if spans.is_empty() {
        return false;
    }
    whole_term_ranges(lower_text, &term.to_ascii_lowercase())
        .iter()
        .any(|(start, end)| {
            spans
                .iter()
                .any(|(span_start, span_end)| start >= span_start && end <= span_end)
        })
}

/// The exact-rung admission test for one ranking pass, holding everything that
/// is invariant across candidates: the case-folded prompt, its backtick spans,
/// and the corpus document frequencies.
///
/// Built AFTER the corpus, never before: `rare` reads the document-frequency
/// map that `prepare_lexical` computes, so a gate constructed earlier would
/// answer `rare` for every name and admit exactly the prose words this exists
/// to reject.
pub(crate) struct ExactGate<'a> {
    /// The prompt, ASCII-lowercased once. Every candidate name would otherwise
    /// re-fold it, and the source channel asks about ~8,500 of them per query.
    lower_query: String,
    /// Byte ranges of the prompt's backtick spans.
    spans: Vec<(usize, usize)>,
    /// Document frequencies for every TOKENIZED query term, dropped ones
    /// included — see `prepare_lexical`, which keeps the dropped terms in the
    /// map for exactly this lookup.
    document_frequencies: &'a BTreeMap<String, usize>,
    /// Highest document frequency at which a name still counts as corpus-rare.
    df_ident_max: usize,
}

impl<'a> ExactGate<'a> {
    /// Build the gate for one query against one channel's corpus.
    pub(crate) fn new(
        raw_query: &str,
        document_frequencies: &'a BTreeMap<String, usize>,
        df_ident_max: usize,
    ) -> Self {
        Self {
            lower_query: raw_query.to_ascii_lowercase(),
            spans: backtick_spans(raw_query),
            document_frequencies,
            df_ident_max,
        }
    }

    /// `Some(evidence)` when `name` occurs in the query as a whole term AND
    /// that occurrence may claim an exact rung; `None` when it does not occur
    /// at all, or occurs only as an ordinary word.
    pub(crate) fn admits(&self, name: &str) -> Option<TermEvidence> {
        let lower_name = name.to_ascii_lowercase();
        let ranges = whole_term_ranges(&self.lower_query, &lower_name);
        if ranges.is_empty() {
            return None;
        }
        let evidence = TermEvidence {
            backticked: ranges.iter().any(|(start, end)| {
                self.spans
                    .iter()
                    .any(|(span_start, span_end)| start >= span_start && end <= span_end)
            }),
            shaped: is_shaped(name),
            rare: !is_plain_word(name) && self.is_rare(&lower_name),
        };
        admits_exact(&evidence).then_some(evidence)
    }

    /// Whether `lower_name` is rare enough in this corpus to admit a rung on
    /// its own.
    ///
    /// A name with no entry in the map counts as rare (frequency `0`). That is
    /// not a fallback, it is the answer: the map holds every tokenized query
    /// term, so a name absent from it is a name no query token equals — a
    /// multi-segment or punctuation name like `Foo::Bar` or `$$$`. Such a name
    /// cannot be the English word this gate exists to reject, because an
    /// English word in the prompt would have tokenized to itself and been
    /// counted.
    ///
    /// Asked ONLY of a name [`is_plain_word`] rejected, and the caller's guard
    /// is what makes the paragraph above true rather than merely usual: a name
    /// that IS in the map and IS spelled like a word never reaches here, so a
    /// low frequency can no longer stand in for "this is a symbol".
    fn is_rare(&self, lower_name: &str) -> bool {
        self.document_frequencies
            .get(lower_name)
            .copied()
            .unwrap_or(0)
            <= self.df_ident_max
    }
}
