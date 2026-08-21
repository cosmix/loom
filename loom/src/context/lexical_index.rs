//! The persistent inverted index that spares the lexical channel from
//! re-deriving its whole corpus on every prompt (A.13).
//!
//! ## Why this exists
//!
//! Ranking one query used to cost a full pass over every document in the
//! channel: ~656 knowledge chunks and ~7,900 source nodes tokenized from
//! scratch, then scanned once more per query term to count document
//! frequencies. That is paid on EVERY prompt, inside a hook with a hard
//! five-second wall-clock ceiling (`hooks/user-prompt-context.sh`), and it
//! grows with the size of the repository rather than with the query.
//!
//! The corpus itself only changes when the catalog revision or the resolved
//! source layer changes. So the pass is done once per revision and persisted:
//! per-document lengths, the document identities, and a
//! `term -> [(document, weighted term frequency)]` postings map. A later prompt
//! against the same revision reads the file and looks terms up instead of
//! re-deriving them.
//!
//! ## The scan is still the oracle
//!
//! [`crate::context::rank::score_bm25`] and the full corpus scan behind it are
//! not dead code and must not be removed. They run whenever no cache is
//! configured — which is every existing test and every caller that has no
//! cache root to hand — they are what a miss falls back to, and they are what
//! this index is checked against. `rank/corpus.rs` routes both paths through
//! ONE arithmetic implementation for the same reason: two copies of the BM25
//! formula, one per path, is how a cache hit ends up scoring a hair
//! differently from a cache miss.
//!
//! ## What is deliberately NOT stored
//!
//! The proposal's draft schema carried `average_length` and a `df` map beside
//! the postings. Both are exact functions of what is stored — the mean of
//! `lengths`, and `postings[term].len()` respectively — and a persisted copy of
//! a derived value is a second source of truth that can only ever be wrong. A
//! stored `average_length` disagreeing with `lengths` by one ULP would shift
//! BM25's length normalization for every document, on cache hits only. Both are
//! recomputed on load instead, by the same expressions the scan path uses.
//!
//! ## Weights are stored as bits
//!
//! A posting's weight is the `f32` sum the scan path computes, written as its
//! raw IEEE-754 bit pattern. The requirement is that a cache hit scores
//! IDENTICALLY to a cache miss, and "identically" here means bit for bit: a
//! one-ULP difference that appears only on a hit, only on a machine whose cache
//! happens to be warm, is close to untrackable in the field. Decimal would
//! very probably survive the round trip — `serde_json` emits the shortest
//! form that round-trips as `f32` and parses back through `f64`, and double
//! rounding is provably safe at nine significant digits — but "very probably,
//! by an argument about double rounding" is a weak foundation for an invariant
//! this load-bearing, and a bit pattern needs no argument at all.

mod cache;

pub use cache::{source_layer_key, IndexChannel, LexicalCache, LEXICAL_RELATIVE_DIR};

use crate::context::lexical::{
    WEIGHT_ALIASES, WEIGHT_BODY, WEIGHT_HEADINGS, WEIGHT_PATHS, WEIGHT_SYMBOLS, WEIGHT_TITLE,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Schema version of a persisted index file.
///
/// Bump this whenever the meaning of a field changes — and also whenever
/// `lexical::tokenize` changes, which is the one input to a document that has no
/// constant to hash into [`derivation`]. An index is only valid for the code
/// that derived it, and a tokenizer change that kept every file byte-identical
/// would otherwise leave warm caches serving the old tokenization forever.
///
/// A file carrying any other version is a miss, not an error: the reader falls
/// back to the scan and rewrites the file in the current shape.
pub(crate) const INDEX_VERSION: u32 = 1;

/// Identity of the field weights that produced a document's `(term, weight)`
/// pairs.
///
/// Stored in the file and checked on load rather than mixed into the file NAME,
/// so the names stay the human-readable `knowledge-<catalog revision>` the
/// proposal specified while a retuned weight still invalidates every index.
/// Retuning `WEIGHT_BODY` without this would leave a warm cache scoring at the
/// old weight and a cold one at the new — a divergence visible only on hits.
fn derivation() -> String {
    let mut hasher = Sha256::new();
    for weight in [
        WEIGHT_TITLE,
        WEIGHT_ALIASES,
        WEIGHT_HEADINGS,
        WEIGHT_SYMBOLS,
        WEIGHT_PATHS,
        WEIGHT_BODY,
    ] {
        hasher.update(weight.to_bits().to_le_bytes());
    }
    hex::encode(&hasher.finalize()[..8])
}

/// One document's entry in a term's postings list: the document's index in the
/// corpus, and the summed weight that document gives the term as raw IEEE-754
/// bits. See the module docs for why bits and not a decimal float.
type Posting = (u32, u32);

/// One channel's corpus, inverted, as it is persisted.
///
/// Everything here is either a `Vec` in corpus order or a `BTreeMap`, never a
/// `HashMap`: serialization has to be byte-identical across runs over identical
/// bytes, and the candidate order downstream has to be too.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LexicalIndex {
    /// [`INDEX_VERSION`] at the time of writing.
    version: u32,
    /// [`derivation`] at the time of writing: the field weights this file's
    /// term weights were computed with.
    derivation: String,
    /// The corpus revision this describes, exactly as the caller spelled it —
    /// which is not always what the file name embeds, since a name has to be a
    /// legal file name and a revision does not.
    revision: String,
    /// Number of documents. Redundant with the two arrays below ON PURPOSE:
    /// it is not read for scoring, it is the cheapest possible integrity check
    /// that a half-written or hand-edited file is rejected rather than scored.
    documents: usize,
    /// Token count per document, in corpus order.
    lengths: Vec<u32>,
    /// Document identity per document index, in corpus order. The revision key
    /// should already guarantee the corpus is the same one; this proves it,
    /// because scoring the right weights against the wrong documents is a
    /// failure that produces plausible output.
    doc_ids: Vec<String>,
    /// `term -> postings`, each list ascending by document index and holding at
    /// most one entry per document.
    postings: BTreeMap<String, Vec<Posting>>,
}

/// The postings for exactly one query's surviving terms, owned so that the
/// parsed [`LexicalIndex`] — several megabytes on a large repository — can be
/// dropped as soon as the corpus is prepared.
pub(crate) struct QueryPostings {
    lists: BTreeMap<String, Vec<(u32, f32)>>,
}

impl QueryPostings {
    /// The summed weight `document` gives `term`, or `None` when it does not
    /// contain the term at all.
    ///
    /// `None` and `Some(0.0)` are NOT interchangeable: the scan path skips a
    /// term the document does not contain and awards no `matched_term_count`
    /// for it, while a document that did contain it scores it even if the
    /// weights happened to sum to zero. The index preserves that distinction by
    /// recording an entry for every occurrence, whatever its weight.
    pub(crate) fn weighted_frequency(&self, term: &str, document: usize) -> Option<f32> {
        let list = self.lists.get(term)?;
        let document = u32::try_from(document).ok()?;
        list.binary_search_by_key(&document, |(index, _)| *index)
            .ok()
            .map(|position| list[position].1)
    }
}

impl LexicalIndex {
    /// Invert `documents`, whose `index`th entry belongs to `doc_ids[index]`.
    ///
    /// A caller that passes mismatched lengths does not get a panic here; it
    /// gets a file that [`LexicalIndex::accepts`] rejects on the next load,
    /// which degrades to the scan rather than to wrong scores.
    pub(crate) fn build(
        revision: &str,
        doc_ids: &[&str],
        documents: &[Vec<(String, f32)>],
    ) -> Self {
        let postings: BTreeMap<String, Vec<Posting>> = accumulate(documents)
            .into_iter()
            .map(|(term, list)| {
                let list = list
                    .into_iter()
                    .map(|(document, weight)| (document, weight.to_bits()))
                    .collect();
                (term, list)
            })
            .collect();

        Self {
            version: INDEX_VERSION,
            derivation: derivation(),
            revision: revision.to_string(),
            documents: documents.len(),
            lengths: documents.iter().map(|terms| terms.len() as u32).collect(),
            doc_ids: doc_ids.iter().map(|id| (*id).to_string()).collect(),
            postings,
        }
    }

    /// `Ok(())` when this file may be scored against the corpus identified by
    /// `revision` and `doc_ids`; otherwise the reason, for a `debug!` line.
    ///
    /// Every rejection is a cache MISS, never an error: the caller falls back
    /// to the scan and rewrites the file. That is what makes it safe to be
    /// this strict.
    pub(crate) fn accepts(&self, revision: &str, doc_ids: &[&str]) -> Result<(), &'static str> {
        if self.version != INDEX_VERSION {
            return Err("schema version mismatch");
        }
        if self.derivation != derivation() {
            return Err("field weights changed since this index was written");
        }
        if self.revision != revision {
            return Err("corpus revision mismatch");
        }
        if self.documents != self.lengths.len() || self.documents != self.doc_ids.len() {
            return Err("document count disagrees with the per-document arrays");
        }
        if self.doc_ids.len() != doc_ids.len()
            || self
                .doc_ids
                .iter()
                .zip(doc_ids)
                .any(|(stored, current)| stored.as_str() != *current)
        {
            return Err("document identities changed");
        }
        self.postings_are_sound()
    }

    /// Token count per document, in the `usize` form the scorer wants.
    pub(crate) fn lengths(&self) -> Vec<usize> {
        self.lengths.iter().map(|length| *length as usize).collect()
    }

    /// Document frequency for every term of `query_terms`, reproducing what
    /// `prepare_lexical` computes by scanning — including an explicit `0` for a
    /// term that occurs nowhere in the corpus.
    ///
    /// The map covers EVERY tokenized query term, dropped ones included,
    /// because `lexical::ExactGate` reads it from the opposite end: it asks how
    /// rare a candidate's own name is, and treats a name absent from the map as
    /// rare. The names it most needs to reject — `point`, `write`, `quality` —
    /// are exactly the ubiquitous words stopwording just dropped, so a map
    /// holding only the survivors would readmit every prose word the exact-rung
    /// gate exists to keep out.
    ///
    /// This is also why the whole corpus vocabulary is NOT handed over instead,
    /// tempting as it is now that the index knows all of it: the gate would
    /// then see real frequencies for names no query term equals, and start
    /// rejecting rungs the scan path admits. Widening that map is a ranking
    /// change to make deliberately, in `lexical/evidence.rs`, not a side effect
    /// of a cache hit.
    pub(crate) fn document_frequencies(&self, query_terms: &[String]) -> BTreeMap<String, usize> {
        let mut frequencies = BTreeMap::new();
        for term in query_terms {
            frequencies
                .entry(term.clone())
                .or_insert_with(|| self.postings.get(term).map_or(0, Vec::len));
        }
        frequencies
    }

    /// The postings for `terms`, decoded back into `f32`.
    pub(crate) fn project(&self, terms: &[String]) -> QueryPostings {
        let mut lists: BTreeMap<String, Vec<(u32, f32)>> = BTreeMap::new();
        for term in terms {
            if lists.contains_key(term) {
                continue;
            }
            if let Some(postings) = self.postings.get(term) {
                let decoded = postings
                    .iter()
                    .map(|(document, bits)| (*document, f32::from_bits(*bits)))
                    .collect();
                lists.insert(term.clone(), decoded);
            }
        }
        QueryPostings { lists }
    }

    /// Every postings list is non-empty, strictly ascending by document index,
    /// and inside the corpus.
    ///
    /// [`QueryPostings::weighted_frequency`] binary-searches these lists, and a
    /// binary search over an unsorted list does not fail loudly — it silently
    /// misses entries, which reads downstream as "this document does not
    /// contain the term". A file that is valid JSON but structurally wrong is
    /// exactly the case that would otherwise score plausibly and differently.
    fn postings_are_sound(&self) -> Result<(), &'static str> {
        for list in self.postings.values() {
            let mut previous: Option<u32> = None;
            if list.is_empty() {
                return Err("empty postings list");
            }
            for (document, _) in list {
                if *document as usize >= self.documents {
                    return Err("posting outside the corpus");
                }
                if previous.is_some_and(|earlier| earlier >= *document) {
                    return Err("postings are not strictly ascending");
                }
                previous = Some(*document);
            }
        }
        Ok(())
    }
}

/// Sum each document's weights per term, in document order.
///
/// The accumulation order is the corpus order and the seed is `0.0`, matching
/// `corpus::scanned_frequency` exactly — that identity is the whole basis for
/// an indexed score equalling a scanned one, so neither side may be
/// "simplified" into a different fold without the other.
fn accumulate(documents: &[Vec<(String, f32)>]) -> BTreeMap<String, Vec<(u32, f32)>> {
    let mut postings: BTreeMap<String, Vec<(u32, f32)>> = BTreeMap::new();
    for (document, terms) in documents.iter().enumerate() {
        let document = document as u32;
        for (term, weight) in terms {
            let list = postings.entry(term.clone()).or_default();
            match list.last_mut() {
                Some(entry) if entry.0 == document => entry.1 += weight,
                _ => {
                    // Seeded at 0.0 like `Iterator::sum`, not at the weight
                    // itself: `0.0 + -0.0` is `+0.0`, so seeding from the first
                    // weight would keep a sign bit the scan path discards.
                    let mut total = 0.0f32;
                    total += weight;
                    list.push((document, total));
                }
            }
        }
    }
    postings
}
