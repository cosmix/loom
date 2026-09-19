# Context Retrieval Corpus

> Stopwording, rescue floor, BM25 index, indexed prose

## Corpus-Derived Query Stopwording, With a Rescue Floor

Query terms are stopworded against the SAME corpus the channel ranks against,
not a fixed English list — a fixed list catches "the" and "is" and stops
there, while the words that actually flood this retrieval are the project's
own ("loom", "stage", "signal", "context"). A term is dropped when its
document frequency exceeds `corpus_size * stop_df_ratio` (default `0.10`) or
it is shorter than `min_query_token_len` (default `3`), UNLESS it occurs
backticked in the raw prompt or is PROTECTED (next paragraph). A chunk or node is a candidate only if it earned
a rung or matched a surviving term (`rank/corpus/stopwords.rs::partition_terms`).

**Naming terms are protected, minus a closed list of function words.** Frequency
alone dropped `stage`, `merge`, `acceptance`, `worktree`, `context`, `codex`, `plan`
and `session` from ten live queries: words the tree is ABOUT, frequent because so
many sections are written on them. A term that NAMES a document is therefore
never dropped as ubiquitous (`stopwords.rs::is_protected`): the set is every term
carried above `WEIGHT_HEADINGS`, which is only a knowledge chunk's heading
(`WEIGHT_TITLE`) and aliases (`WEIGHT_ALIASES`) (`lexical_index.rs::naming_terms_of`).
Chunk symbols, paths and anchors sit AT `WEIGHT_HEADINGS` and are mentions, not names, so
they are not protected (the first version protected everything above `WEIGHT_BODY`;
backticked `write`/`home`/`doc` became protected and a path-word query ranked nine
chunks, pinned by `rank_evidence::words_lifted_out_of_a_filesystem_path_earn_no_exact_rung`).
Source-graph symbol names are deliberately not protected either: a source node's scope
sits at `WEIGHT_HEADINGS`, so the source corpus protects nothing, and its only
over-floor terms are keywords and type names (`fn`, `pub`, `str`, `path`, `result`, `test`).
Headings are also phrased with English function words ("Why this class is hard to
see"), and no statistic separates `the` from `stage`, so protection skips the closed
`FUNCTION_WORDS` list (`stopwords.rs:60`), the one place a fixed list is the right
tool. The persisted BM25 index stores the naming set (`INDEX_VERSION` 2), so a warm
index and a fresh scan partition identically.

## Persistent BM25 Index (A.13)

Every prompt used to re-tokenize the whole corpus from scratch — ~656
knowledge chunks and ~7,900 source nodes on this repository — then scan it
again per query term for document frequencies, inside a hook with a hard
five-second ceiling. A persistent inverted index (`context/lexical_index.rs`)
now makes a cache hit skip the tokenization entirely:

- Keyed per channel: the knowledge index by the catalog revision, the source
  index by `lexical_index::source_layer_key` — a hash of the resolved layer
  actually being indexed (base revision plus each file's path, content hash
  and parser version), NOT the overlay fingerprint, because the ranker never
  receives that. A key/corpus mismatch is structurally impossible rather than
  merely unlikely.
- **The full scan stays the default and the correctness oracle.** Every
  caller with no cache root — every existing test included — gets the scan;
  `rank::corpus::score_terms` is the ONE arithmetic implementation both the
  scanned and indexed representations route through, so a cache hit cannot
  score differently from a miss. A property test asserts indexed scoring
  equals scan scoring exactly (same scores, distinct-term counts, candidates,
  order) across randomly generated corpora.
- **Not stored:** `average_length` and the document-frequency map. Both are
  exact functions of what IS stored (document lengths, postings) and a
  persisted derived value is a second source of truth that can only be
  wrong. Recomputed on load by the same expressions the scan uses.
- **Weights are stored as raw IEEE-754 bits**, not decimal, so a round trip
  cannot shift a score by an ULP.
- The file hashes the `WEIGHT_*` constants (`derivation()`,
  `lexical_index.rs:85-98`) and is rejected — falls back to the scan, then
  rewrites — when they no longer match, so retuning a weight cannot leave a
  warm cache scoring at the old value.
- **`INDEX_VERSION` (currently `2`) must be bumped whenever `lexical::tokenize`
  changes** — the one input to a document with no constant to hash. Version 2
  persists `naming_terms` (the one DERIVED value stored, because summed postings weights cannot
  recover which field a term came from), which the protected-term stopwording needs. See
  [conventions.md](../conventions.md).
- Pruning (`lexical_index/cache.rs`) keeps a bounded number of index files per
  channel (`KEEP_INDEXES = 6`) rather than unlinking every sibling revision:
  parallel worktrees resolve different keys against one shared cache
  directory, so unlink-all would have each stage evict every other stage's
  index on every prompt.
- Every write is best-effort and silent (`debug!`, never an error) — a
  sandboxed or read-only caller still retrieves.

## Indexed Prose: a Third Corpus Component (A.15)

A file with no registered tree-sitter grammar produced only a whole-file
node, and `rank_source` drops whole-file nodes — so design documents under
`doc/` were unreachable by retrieval even though `context::extract`'s own
docstring claimed otherwise. Every `*.md` under `config.prose_roots` (default
`["doc"]`) is now chunked by the same heading chunker the curated tree uses,
with every id PREFIXED `prose:` (`fs::knowledge::catalog::prose::PROSE_ID_PREFIX`)
so it can never collide with a curated chunk id and the prefix itself signals
origin. Completed plans (`DONE-` filenames under a `plans/` path segment) are
excluded as history; the curated knowledge tree itself is skipped during the
walk (it already has its own chunker) so `prose_roots = ["doc"]` does not
double-index every curated chunk as its own prose clone.

Prose participates in the structural (catalog) revision, so editing a design
doc marks the catalog stale and it re-indexes on the next query — one
function derives the prose source list for both the chunker and the
fingerprinter, so the two halves of the freshness contract cannot disagree.

**Curated knowledge keeps priority by DEMOTING prose, never by promoting
curated.** `rank::prose_demotion` subtracts `config.knowledge_curated_prior`
(default `5.0`, an increment applied after BM25 + rung scoring, not a
multiplier) from a `prose:`-prefixed candidate's score. Promoting curated
instead would have been equivalent for curated-vs-prose ordering but would
also inflate the knowledge channel against the source channel and compress
the within-channel normalized scores tier-2 fusion's tie-break depends on.
The demotion is **clamped at zero** — left unclamped, a query answered only
by prose gives the channel a negative maximum, and tier 2's
`raw_score / channel_max` INVERTS the ordering (`-3.0/-1.0 = 3.0` outranks
`-1.0/-1.0 = 1.0`), putting the worst match first. Applied AFTER the
candidacy check, not inside the exact-match ladder, so it never turns "no
rung fired" into a candidate — that would make every curated chunk a
candidate on every query and undo the stopwording candidacy floor.

The pack's `dropped_terms` is now the UNION of both channels' drops, not
whichever channel was consulted first — with per-corpus ubiquity floors the
two channels genuinely differ on what they drop.

## The Rescue Floor and the Thin-Survivor Rule

**The rescue floor exists because indexing prose changed what "ubiquitous"
means.** Measured on this repository: on the curated-only corpus (658 docs,
floor 65.8) the query "worktree claude code sandbox settings rules sessions"
kept `settings`=57, `rules`=48, `sessions`=65 and returned a pack. Once A.15
indexed project prose into the same corpus (904 docs, floor 90.4) those same
terms inflated to 105, 93, 102 — prose is loom's own design docs, sharing the
question's vocabulary — so EVERY term exceeded the floor and an ordinary
question about the codebase returned an empty pack. Up to `RESCUE_LIMIT = 3`
of the rarest dropped terms are put back, subject to a hard
`stop_rescue_max_ratio` ceiling (default `0.25`): a term at 11% of the corpus
comes back when the query needs it; a term at 90% never does, so a genuine
stopword-only query still retrieves nothing. Rescued terms are removed from
`dropped_terms`, which stays a truthful account of what was actually dropped
(`stopwords.rs::rescue_rarest`).

**The floor is under a THIN surviving set, not only an empty one**
(`stopwords.rs::over_stopworded`). The first version fired only when
stopwording dropped every term, which left the measured query "sandbox
settings rules for claude code worktree sessions" answered on `rules` alone:
seven of its eight content words were above the floor, the one generic word
that survived counted as a survivor, and the expected chunk in
[Sandbox and Settings](../mistakes/sandbox-and-settings.md) ("Worktree
Settings Are a Whole-Object Rebuild") never says "rules", so it was not a
candidate at all. The rescue now
also fires when a query of at least `RESCUE_QUERY_MIN_TERMS = 4` distinct
content terms kept fewer DISTINCT surviving terms than `min_knowledge_terms`
(default `2`). Both constants are chosen, not tuned:

- the survivor floor REUSES `min_knowledge_terms` as its measure of "enough
  distinct surviving terms to retrieve on", so an operator who lowers the floor
  to 1 turns the thin-survivor rescue off. The rescue serves `loom knowledge
  context` and stage briefs ONLY: a rescued term ranks, but it never counts
  toward the prompt hook's emit floor (`LexicalCorpus::naming_matches`, see
  Brief Delivery below), because it is ubiquitous and names nothing. A prompt
  answered only on rescued terms gets candidates for a pull or a stage brief
  and silence from the hook;
- `RESCUE_QUERY_MIN_TERMS` keeps short prompts out. "Honesty Contract" left
  with one surviving term was served well — that survivor IS the lookup — and
  putting its neighbours back would only add noise. Four or more content words
  is a question, and a question reduced to one generic word was answered on
  the least of what it asked.

The cap stays flat at three on both paths; sizing it to the deficit instead
(enough to reach `min_knowledge_terms`, no more) was measured and rejected,
because on the case above it restores `sessions` alone rather than `sessions`,
`settings` and `sandbox` together. Note what the fix does and does not buy:
that chunk went from not-a-candidate to a candidate ranked around 28th, and
the rescued `sandbox`/`settings` also feed the source channel's candidacy
rule, so two `sandbox_settings` source nodes now enter the fused top 5. The
`genuine-win-sandbox-settings-rules` eval case still misses at hit@5 — what is
left there is a ranking question, not a candidacy one.

One document-frequency map serves both stopwording (drops the ubiquitous) and
`ExactGate::is_rare` (admits the rare) — deliberately: excluding prose from
the frequency statistics was considered and rejected, because it would let a
term be simultaneously "too common to score" and "rare enough to claim an
exact symbol match" by two disagreeing counts. Dropped terms stay in the
frequency map for exactly this reason.
