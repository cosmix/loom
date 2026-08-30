# Context Retrieval

> The retrieval subsystem: two graphs, two lanes, query-side gating, two-tier fusion, and the persistent BM25 index.

## What This Subsystem Is

`loom/src/context/` answers exactly one question — "which curated prose is worth
spending N tokens on for this query?" — and answers it identically every time.
No embedding model, no network call, no randomness: a `ContextPack` is a pure
function of the bytes on disk and the query string (`context/mod.rs:1-8`).

Read `context/mod.rs` first for the module map. **Its own pipeline diagram
still describes plain reciprocal-rank fusion and is stale** — fusion is
two-tier now (see below); read this file for the current pipeline, not the
docstring.

## Two Graphs, Two Lanes, Both Wired

There are two distinct graphs.

| Graph | Built by | Ranked by | Consumed by |
| --- | --- | --- | --- |
| Knowledge-chunk catalog (curated prose under `doc/loom/knowledge/`, plus indexed project prose — see below) | `fs::knowledge::chunker`/`catalog::prose` → `context::ingest` | `context::rank` | `loom knowledge context`, `loom knowledge eval`, the Knowledge Brief |
| Source graph (tree-sitter nodes/edges over the repo) | `context::extract` → `context::refresh::source_graph` | `context::rank_source` | the same three, plus `loom map` via `context::graph_store` |

`rank_channels` dispatches per channel: `Channel::Knowledge` to `rank`,
`Channel::Source` to `rank_source` when a resolved graph is available and to
an empty result when it is not. The two lists then meet in `fuse` → `pack`,
so a pack can mix curated prose, indexed prose and symbol nodes.

**`rank_source` is not `rank` with a different corpus, but the two share the
exact-rung machinery.** `context/rank_source.rs`:

- Whole-**file** nodes are dropped first — no signature, no scope, nothing to
  score (`rank_source.rs:96-99`).
- The exact-match rungs (`ExplicitId` / `ExactPath` / `ExactSymbol`) match
  against the RAW query text via `contains_whole_term`, not against tokens:
  `tokenize` would shred CamelCase symbols and slashed paths
  (`rank_source.rs:228-256`, `paths.rs`).
- Both rankers gate every rung through the same `lexical::ExactGate` and
  accumulate through the same `rank::RungScore` — see **Exact-Rung Gating**
  below. `prepare_lexical` / `score_bm25` were lifted into `rank::corpus` so
  both rankers score identically; only their document construction differs
  (`rank_source.rs:281-294` builds from `node.scope` at `WEIGHT_SYMBOLS` plus
  `node.signature` at `WEIGHT_BODY`).
- A source node in a file a dependency stage owns earns `StageDependency`
  (`BOOST_STAGE_DEPENDENCY = 30.0`), matched on exact normalized paths — a
  prefix match would hand the boost to an entire tree (`rank_source.rs:203-205`,
  `paths.rs::names_dependency_path`).
- A node whose path or scope follows a test convention has its FINAL score
  (rungs plus lexical) multiplied by `config.test_path_factor` (default
  `0.4`) — ordering pressure so implementation outranks the tests that
  exercise it, never exclusion (`rank_source.rs:216`, `paths::apply_test_path_factor`).
- Candidates are capped at `MAX_SOURCE_CANDIDATES = 60`; ties break on
  `(path, line_start)` so the pack is deterministic (`rank_source.rs:53,134-147`).
- **Coverage guard:** a node whose file was not fully extracted loses ALL THREE
  high-confidence rungs together, not selectively (`withhold_partial_coverage`,
  `rank_source.rs:258-276`). `Confidence::from_reasons` promotes to `High` on
  any one of the three, so dropping them one at a time would leave a
  partially-parsed file claiming full confidence. The node is still ranked
  and returned — only its confidence claim is withheld.

Packing dispatches on `candidate.channel`, never by trying both maps
(`pack.rs::build_item`). A source item is `ItemKind::SourceNode` with
`id = node.id` (`<path>#<kind>:<scope>`, disjoint from a knowledge chunk id),
`content_hash = node.body_hash`, and an excerpt taken from the signature. The
strict dispatch is deliberate: if the two id spaces ever did collide, this
surfaces it as a bug instead of silently masking it.

## Exact-Rung Gating: the Query Side Also Has to Look Like Code

Before A.1, boundary-checked substring matching (`contains_whole_term`) fixed
only the *candidate* side of an exact match — a symbol named `n` stopped
matching every prompt containing the letter — but nothing checked whether the
*query occurrence* looked like a code reference at all. Measured, all real:
"why doesn't loom repair --fix do it, **the point** is" pulled in `lerpPoint`,
`repairGini` and `type Point` at `high` confidence purely because an ordinary
English word happened to equal a symbol name.

`lexical::ExactGate` (`context/lexical/evidence.rs`) now admits a rung only
when the occurrence carries one of three independent signals, any one being
sufficient:

- **backticked** — inside a `` `…` `` span in the raw prompt;
- **shaped** — the matched name is identifier-shaped: contains `_` or `::`,
  or has an interior lowercase→uppercase transition (camelCase). A leading
  capital alone (`Point`, `Widget`) does NOT count — those are ordinary
  English words Rust happens to capitalize;
- **rare** — the name's document frequency in THIS channel's corpus is at
  most `config.df_ident_max` (default `5`). A name absent from the frequency
  map counts as rare — the map holds every tokenized query term, so an
  absent name is one no query token equals at all, e.g. `Foo::Bar`.

A full relative path still fires unconditionally — a path in a prompt is
always deliberate (`rank_source.rs::matches_path`, `PathMatch::FullPath`).

A rung admitted on **rarity alone** is capped at `Confidence::Medium`, never
`High`, via `rank::rungs::RungScore::confidence_ceiling()` — one full-strength
rung (backticked or shaped) is enough to restore `High` even alongside a
weaker one. The ceiling is published, not just computed: `pack` builds every
item through `RankedCandidate::confidence()`, which returns the WEAKER of the
reasons-implied confidence and the ceiling — never `Confidence::from_reasons`
directly (`rank.rs:144-178`, `pack.rs:96-100,154-156`). See
[Tests That Cannot Fail](../mistakes/tests-that-cannot-fail.md) for why a
ranker-level test alone did not catch a packer that forgot this.

Nothing here excludes a candidate — a word that fails every test still
competes on its BM25 score, it just cannot buy the ~80-point exact-symbol
boost with a coincidence.

## Corpus-Derived Query Stopwording, With a Rescue Floor

Query terms are stopworded against the SAME corpus the channel ranks against,
not a fixed English list — a fixed list catches "the" and "is" and stops
there, while the words that actually flood this retrieval are the project's
own ("loom", "stage", "signal", "context"). A term is dropped when its
document frequency exceeds `corpus_size * stop_df_ratio` (default `0.10`) or
it is shorter than `min_query_token_len` (default `3`), UNLESS it occurs
backticked in the raw prompt. A chunk or node is a candidate only if it earned
a rung or matched a surviving term (`rank/corpus/stopwords.rs::partition_terms`).

**The rescue floor exists because indexing prose changed what "ubiquitous"
means.** Measured on this repository: on the curated-only corpus (658 docs,
floor 65.8) the query "worktree claude code sandbox settings rules sessions"
kept `settings`=57, `rules`=48, `sessions`=65 and returned a pack. Once A.15
indexed project prose into the same corpus (904 docs, floor 90.4) those same
terms inflated to 105, 93, 102 — prose is loom's own design docs, sharing the
question's vocabulary — so EVERY term exceeded the floor and an ordinary
question about the codebase returned an empty pack. When stopwording would
drop every term, up to `RESCUE_LIMIT = 3` of the rarest dropped terms are put
back, subject to a hard `stop_rescue_max_ratio` ceiling (default `0.25`): a
term at 11% of the corpus comes back when nothing else survived; a term at
90% never does, so a genuine stopword-only query still retrieves nothing.
Rescued terms are removed from `dropped_terms`, which stays a truthful
account of what was actually dropped (`stopwords.rs::rescue_rarest`).

One document-frequency map serves both stopwording (drops the ubiquitous) and
`ExactGate::is_rare` (admits the rare) — deliberately: excluding prose from
the frequency statistics was considered and rejected, because it would let a
term be simultaneously "too common to score" and "rare enough to claim an
exact symbol match" by two disagreeing counts. Dropped terms stay in the
frequency map for exactly this reason.

## Two-Tier Fusion (Not Reciprocal-Rank Fusion Alone)

`fuse` (`context/fuse.rs`) used to be plain reciprocal-rank fusion (RRF),
which reduces every candidate to its rank *position* — so a knowledge chunk
scoring 1080 on an explicit-id hit and a source node scoring 0.3 on a weak
lexical match both landed at rank 1 and tied exactly, falling through to
alphabetical id order. That systematically ordered the fused head by path
prefix rather than by relevance.

Fusion is now two tiers:

- **Tier 1** holds every candidate (from either channel) whose reasons
  include at least one exact rung — `ExplicitId`, `ExactPath`, `ExactSymbol`,
  `LinkedFrom` or `StageDependency`. Classification runs on the MERGED reason
  set across channels, so a candidate that is exact-rung in one channel and
  lexical-only in another is still tier 1 everywhere. Ordered by raw score
  descending (the max raw score seen across channels, never the sum — summing
  would reward a candidate merely for appearing twice), then id ascending, and
  precedes ALL of tier 2.
- **Tier 2** holds the remainder. Each channel's survivors (tier-1 ids
  removed) are renumbered from 1 and fused by ordinary RRF (`RRF_K = 60`,
  unchanged). Ties in RRF score break by **within-channel normalized score**
  (`raw_score / that channel's max raw score`, `0.0` on a zero or
  non-finite divisor) descending, then id ascending.

Tier-1 raw scores are comparable ACROSS channels only because both rankers
import the same boost constants (`BOOST_EXACT_PATH`, `BOOST_EXACT_SYMBOL`,
`BOOST_EXPLICIT_ID` from `rank.rs`) — a `100.0` exact-path hit means the same
thing whichever channel produced it. **`score` is NOT comparable across the
tier boundary**: tier 1 is raw ladder score (tens to 1000+), tier 2 is RRF
score (roughly `1/RRF_K` and smaller). Downstream readers of `score` —
`pack::build_omission_summary`'s `weakest_included_score` and the hook's
`without_weakest` — happen to behave correctly today because the weakest
item is always tier-2, a consequence of the scale gap, not something either
function checks (`fuse.rs` module doc).

Packing itself is unchanged by tiering: **there is still no per-channel
budget.** `pack` walks the fused (now two-tier) list in order, taking whole
items until the budget is spent, so a query that matches prose strongly can
legitimately fill the pack with prose. A source item costs ~20-30 tokens
against a knowledge chunk's few hundred, so an unbounded source list does not
crowd prose out by token volume — it crowds it out by *slot*, one alternating
rank at a time.

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
- **`INDEX_VERSION` (currently `1`) must be bumped whenever `lexical::tokenize`
  changes** — the one input to a document with no constant to hash. See
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

## The Pipeline

```text
knowledge/*.md ──chunker──┐
doc/**/*.md (prose) ──────┼──> KnowledgeChunk ──catalog──> Catalog (revision)
                           │                                    │
                       fingerprint ──> Freshness              ingest
                           │                                    │
                           └──────────> store (.loom/cache/context-v1/) <┘
                                              │
                          rank ──> fuse (two-tier) ──> pack ──> ContextPack
                            ▲
                    lexical_index (per-revision cache; scan is the oracle)
```

- `rank` scores each requested channel independently, through the shared
  BM25 + exact-rung machinery described above.
- `fuse` merges the per-channel lists by **two-tier fusion** (exact-rung
  candidates first by raw score, the lexical remainder by RRF) — see above,
  not plain reciprocal-rank fusion.
- `pack` walks the fused list in order taking WHOLE chunks until the budget
  is spent; it never exceeds the budget and always reports what it left out
  (`OmissionSummary`).

**One entry point.** `retrieve::retrieve_for_stage` runs the whole pipeline
and is the only way in — `loom knowledge context`, `loom knowledge eval`
(the retrieval evaluation harness, below), signal generation and the prompt
hook all call it, so a brief rendered at spawn time and a brief pulled by
hand are built the same way (`context/mod.rs:36-42`). Adding a fifth
consumer means calling that function, not reimplementing the pipeline.

**Retrieval evaluation harness (A.20).** `loom knowledge eval` (dispatched at
`cli/dispatch.rs:99`, implemented in `commands/knowledge/eval.rs`) scores a
checked-in case file (default `loom/eval/retrieval-cases.yaml`) through
`retrieve_for_stage` against the LIVE on-disk index and reports per-case
hit@5/MRR plus aggregate precision@5, exiting non-zero when aggregate
precision falls below the file's `pass_floor` or any `forbid` id appears
anywhere in a case's results. `forbid`-only cases are excluded from the
precision denominator so a fixed regression case cannot cap the score
forever, and a case with neither `expect` nor `forbid` fails construction —
it could never fail the run. Deliberately NOT wired into `cargo test`: it
reads the live index, which is not reproducible in CI. `scripts/harvest-eval-cases`
drafts further cases from session transcripts for hand-labeling.
`scripts/retrieval-ab` measures precision@5, injected tokens per brief and
hook wall-time percentiles against a baseline binary, routed through one
env-stripping helper so its "isolated" measure root cannot inherit the
calling session's `LOOM_WORK_DIR` (see
[Never Spawn a Surviving Process From a Test](../mistakes/detached-spawn-in-tests.md)
for why an inherited env var made an "isolated" harness mutate the real
checkout).

## Base vs Overlay Ownership

This is the rule that keeps parallel worktrees from corrupting each other
(`context/graph_store/mod.rs:3-23`). Parallel stages run in separate worktrees
off one repository; if they shared a mutable graph a stage would see HALF of a
sibling's edits — worse than seeing none, because there is no way to tell which
half.

| Layer | Location | Keyed by | Mutability |
| --- | --- | --- | --- |
| **base** | `.loom/cache/context-v1/graph/base/` under the canonical MAIN project root, shared by every worktree | the source revision it was built from | written once by the host, thereafter immutable |
| **overlay** | `.work/context/<plan>/<stage>/` | plan + stage | per-stage, holds only the files that stage changed |

A read is `overlay ∪ (base − overlay's files)`. An overlay entry shadows the
base entry for the same path **wholesale, never merges with it** — partial
merges produce a graph that describes no revision that ever existed.

`graph_store` owns only the layout, the layering rule and canonical
serialization. It never builds a graph (`context::refresh` does) and never
decides *when* to write one (`refresh::source_graph::reconcile_source_graph`
does).

**A missing base is not automatically a degraded pack.** `GraphStore::resolved`
substitutes an empty graph when no base file exists for the recorded semantic
revision — which is the ORDINARY state of a dirty working tree, since a base
is immutable and revision-keyed, and `refresh::semantic::try_reconcile_semantic`
deliberately falls back to a `_local` overlay instead of ever publishing one.
`ContextPack::degraded` (A.11) fires only for the narrower, genuinely
reachable case: a non-empty semantic revision that NEITHER the base nor any
overlay can back at all, so the resolved graph has no content whatsoever
(`context/retrieve/graph.rs::degraded_reason`). Widening that predicate to
"any missing base" was tried and reverted — it flagged every healthy checkout
as degraded permanently, and `reconcile_graph::spawn_if_needed` triggers on
`stale OR degraded`, so it also started a detached full-repository
tree-sitter rebuild on every single prompt in every working checkout. See
`degraded_reason`'s own doc comment for the reconcile-trigger consequence
before widening this predicate again.

**Known gap — an overlay cannot express a deletion.** `GraphStore::resolved`
computes `overlay ∪ base`, so a file the stage deleted keeps its base entry and
`loom map --outline <deleted-file>` still prints the old outline. Fixing it needs
a tombstone concept in `graph_store` (`context/refresh/source_graph.rs:10-16`).

## Derived vs Durable

Getting this wrong destroys work, so it is worth stating flatly:

- **Derived / regenerable:** everything under `.loom/cache/context-v1/` (chunk
  catalog, fingerprints, base graph layers, the persistent lexical index).
  Safe to delete; `loom knowledge sync` rebuilds it. It is git-ignored.
- **Durable within a run:** the per-stage overlay and the **delivery records**
  under `.work/context/<plan>/<stage>/`. These are NOT regenerable from the
  repo alone — a delivery record states what a specific recipient was already
  given.
- **Durable forever:** only `doc/loom/knowledge/*.md`, the curated prose itself
  (indexed prose under other `doc/` paths is durable too, but it is source
  documentation with its own reason to exist, not knowledge-base content).

The distinction has already caused one 100%-reproducible defect: a discard
routine deleted delivery records out of a directory shared with the graph layer,
so the dependency-ranking boost failed every time on the daemon path. The fix
was to discard only the graph layer, not the shared directory (commit
`7e35eef7`). Rule: **a "discard the derived layer" operation must name the layer,
never the directory** — check what else writes into that directory first.

## Delivery Records and Epoch Suppression

`context/delivery.rs` answers "has this recipient already been given these exact
bytes?", so a second retrieval in the same session can skip what the first
already quoted instead of repeating it.

- The record is an **optimisation, never state the run depends on**. Nothing in
  it may fail a spawn or a hook: a missing directory reads as "nothing
  delivered", and an unreadable or malformed file is skipped rather than
  propagated (`delivery.rs:7-9`).
- Suppression is scoped to a **`context_epoch`**: once a derived layer is
  rebuilt the same id may describe different bytes, so every record from an older
  epoch is ignored and delivery re-opens (`delivery.rs:11-13`).
- `context_epoch` = first 8 bytes of `sha256(structural_revision \n
  semantic_revision)` (`retrieve.rs::context_epoch`). Note the **two freshness
  axes**: structural (knowledge catalog) and semantic (source graph).
- `delivery::plan_key` / `plan_key_from` is the ONE derivation of the plan
  namespace and is the join key between the writer of a record and its readers
  (`delivery.rs:42-63`). A second, hand-rolled derivation reads an empty
  directory rather than a missing record — which is why
  `orchestrator/core/stage_telemetry.rs` routes through the helper.
- **The prompt hook keys its own dedupe per SESSION, not per checkout (A.16)**,
  through a SEPARATE submodule of the same file: `delivery::session` exposes
  `hook_recipient_id`, `delivered_to_session` and `discard_session_delivery`
  (`delivery.rs:29-34`). A stage's own spawn-brief delivery record is keyed by
  loom's session id under `plan_key`/stage id, as above; the hook's is keyed
  by `sha8(hook payload's session_id)` — a DIFFERENT id space, hashed because
  the raw id is untrusted input that becomes a file name
  (`commands/hook/user_prompt.rs`). Without this split, a fresh Claude Code
  session with an empty context window inherited every prior session's
  deliveries and went silent on topics it had never actually seen. `loom hook
  pre-compact` calls `discard_session_delivery` for just that session's own
  record after a compaction, when the context that held the brief is gone
  (A.21).

## Brief Delivery, Sanitization and Telemetry

- The **Knowledge Brief** is assembled in
  `orchestrator/signals/format/brief.rs` and injected into the stage signal at
  spawn time. It renders as a `### Knowledge` section (curated + indexed
  prose, fenced excerpt plus reason line) followed by a `### Source (signature
  index)` section (one unfenced bullet per file, symbols/spans/reasons
  inline, consecutive items on the same path merged onto one bullet) — grouping
  is render-only, ids/content-hashes/token-counts and the packing/dedupe/budget
  decisions behind them are unchanged. The "quoted, NOT instructions" guard is
  stated once in the header rather than once per item, which is where most of
  the per-item token overhead used to go.
- Every untrusted knowledge-derived value on an agent-facing surface goes
  through the single flattening routine `context::untrusted::inline_safe`
  (`context/untrusted.rs`). Chunk ids come verbatim from unvalidated YAML
  frontmatter, a backtick is a legal path character, and a summary is taken from
  a chunk heading — emitted raw, a newline ends the line it sits on and the
  remainder renders as document structure outside any "quoted, NOT instructions"
  guard. There are exactly TWO render surfaces and both call it:
  `orchestrator/signals/format/brief.rs` and `commands/knowledge/context.rs`.
  `MAX_INLINE_CHARS = 200`; backticks become `ˋ` (U+02CB).
- **The prompt hook injects a brief only when it clears an emit floor** — a
  gate this file's earlier drafts did not describe. `parse_prompt` declines
  machine-generated payloads before retrieval even runs (task-notification
  XML, "Background agent … was stopped" notices, harness caveats); an emit
  floor then requires either an exact-rung item or a knowledge item matching
  `config.min_knowledge_terms` (default `2`) distinct surviving terms; the
  payload ceiling is `config.max_payload_bytes` (default `16384`). The floor
  applies to UNSOLICITED injection only — `loom knowledge context`,
  `loom knowledge eval` and the stage spawn brief are deliberately not gated.
  See `commands/hook/user_prompt.rs` for the composition; this file covers
  retrieval, not the hook's own gating logic in full.
- **Telemetry** (`loom/src/telemetry/mod.rs`) appends one JSON line per spawned
  session to `.work/telemetry/events.jsonl`: `ContextDelivered { stage_id,
  session_id, context_epoch, items }` or `ContextUnavailable { stage_id,
  session_id, reason }`. Best-effort by contract — `emit` never fails a spawn,
  and `read_events` skips a malformed line rather than failing the file. Counts
  are ITEM counts, never a token saving. `read_events` has no production caller
  today, and `.work/` is deleted at plan finalization, so events currently go
  unread; `orchestrator/core/stage_telemetry.rs` is the only writer, called from
  `stage_executor.rs:570`.

## Layering

`context` imports only `crate::context`, `crate::fs`, `crate::language`,
`crate::models` and — deliberately, once — `crate::git`
(`refresh/source_graph.rs:30`, `git::runner::run_git_checked`, needed to list
tracked files and judge tree cleanliness). There is **no** upward edge to
`orchestrator`, `commands` or `daemon`. Verified by
`rg '^use crate::[a-z_]+' loom/src/context`. Keep it that way: the orchestrator
calls into `context`, never the reverse.
