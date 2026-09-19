---
sources:
- loom/src/context/pack.rs
- loom/src/context/retrieve.rs
verified: 054528e508d51ede343e254590cdb73ae00f7df6
---
# Context Retrieval

> Retrieval: graphs, lanes, gating, tiered packs

## What This Subsystem Is

`loom/src/context/` answers exactly one question — "which curated prose is worth
spending N tokens on for this query?" — and answers it identically every time.
No embedding model, no network call, no randomness: a `ContextPack` is a pure
function of the bytes on disk and the query string (the `context` module doc).

Read `context/mod.rs` first for the module map. Its pipeline prose now describes
both channels and two-tier fusion, so it is no longer the stale docstring an
earlier version of this section warned about; its diagram still omits prose
indexing, the lexical index, lifecycle filtering and source-lane expansion, which
this file and its two companions cover: [Context Retrieval Corpus](context-retrieval-corpus.md)
(query stopwording and the rescue floor, the persistent BM25 index, indexed prose) and
[Context Retrieval State](context-retrieval-state.md) (base vs overlay ownership, derived vs
durable, delivery records, epoch suppression, brief delivery and telemetry).

## Two Graphs, Two Lanes, Both Wired

There are two distinct graphs.

| Graph | Built by | Ranked by | Consumed by |
| --- | --- | --- | --- |
| Knowledge-chunk catalog (curated prose under `doc/loom/knowledge/`, plus indexed project prose — see below) | `fs::knowledge::chunker`/`catalog::prose` → `context::ingest` | `context::rank` | `loom knowledge context`, `loom knowledge eval`, the Knowledge Brief, the prompt hook |
| Source graph (tree-sitter nodes/edges over the repo) | `context::extract` → `context::refresh::source_graph` | `context::rank_source` | the same, plus `loom map` via `context::graph_store` |

`retrieve/channels.rs::rank_channels_cached` dispatches per channel:
`Channel::Knowledge` to the knowledge ranker (then `apply_lifecycle_policy`),
`Channel::Source` to `rank_source_channel_cached` when a resolved graph is
available and to an empty result when it is not. Each channel gets its own
persistent lexical cache when a cache root exists. The two lists then meet in
`fuse` → `pack`, so a pack can mix curated prose, indexed prose and symbol nodes.

**`rank_source` is not `rank` with a different corpus, but the two share the
exact-rung machinery.** `context/rank_source.rs`:

- Whole-**file** nodes are dropped first — no signature, no scope, nothing to
  score.
- The exact-match rungs (`ExplicitId` / `ExactPath` / `ExactSymbol`) match
  against the RAW query text via `lexical::contains_whole_term`, not against
  tokens: `tokenize` would shred CamelCase symbols and slashed paths
  (`rank_source/paths.rs`).
- Both rankers gate every rung through the same `lexical::ExactGate` and
  accumulate through the same `rank::RungScore` — see **Exact-Rung Gating**
  below. `prepare_lexical` / `score_bm25` live in `rank::corpus` so both rankers
  score identically; only their document construction differs (`node_document`
  builds a source document from `node.scope` at `WEIGHT_SYMBOLS` plus
  `node.signature` at `WEIGHT_BODY`).
- A source node in a file a dependency stage owns earns `StageDependency`
  (`BOOST_STAGE_DEPENDENCY = 30.0`), matched on exact normalized paths — a
  prefix match would hand the boost to an entire tree
  (`paths::names_dependency_path`).
- A node whose path or scope follows a test convention has its FINAL score
  (rungs plus lexical) multiplied by `config.test_path_factor` (default
  `0.4`) — ordering pressure so implementation outranks the tests that
  exercise it, never exclusion (`paths::apply_test_path_factor`).
- Candidates, graph neighbours included, are cut to `MAX_SOURCE_CANDIDATES = 60`;
  ties break on `(path, line_start)` so the pack is deterministic (`rank_order`).
- **Coverage guard:** a node whose file was not fully extracted loses ALL THREE
  high-confidence rungs together, not selectively (`withhold_partial_coverage`).
  `Confidence::from_reasons` promotes to `High` on any one of the three, so
  dropping them one at a time would leave a partially-parsed file claiming full
  confidence. The node is still ranked and returned — only its confidence claim
  is withheld.

**Source-lane graph expansion** (`rank_source/expand.rs::expand_from_seeds`, run
on the channel's scored list before it is cut to `MAX_SOURCE_CANDIDATES`). The
strongest `MAX_EXPANSION_SEEDS = 5` candidates holding an `ExplicitId`,
`ExactPath`, `ExactSymbol` or `StageDependency` rung become seeds. Their
neighbours along `Calls`, `Implements`, `Extends` and `References` edges, in
either direction, resolved and with edge confidence at least
`MIN_NEIGHBOR_EDGE_CONFIDENCE = 0.5`, are examined strongest edge first, at most
`MAX_EXAMINED_NEIGHBORS_PER_SEED = 32` per seed. A neighbour is accepted only if
it is not already a candidate, is not a `File` node, and has `FileCoverage::Full`
— at most `MAX_NEIGHBORS_PER_SEED = 4` per seed and `MAX_EXPANDED = 12` in all.
An accepted neighbour carries the single reason `GraphNeighbor`,
`matched_term_count` 0 and a `Confidence::Medium` ceiling; its score is
`seed score × NEIGHBOR_SCORE_FACTOR` (0.2), capped at
`BOOST_EXACT_SYMBOL × test_path_factor` (32 at the defaults) and then put through
the test-path factor, so a neighbour can never outrank a direct exact hit, even
one in a test file. `GraphNeighbor` is not an exact rung, so neighbours fuse in
tier 2; the prompt hook admits one only while the pack still holds an exact-rung
item (see *Brief Delivery* below).

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
  most `config.df_ident_max` (default `5`), AND the name is not spelled like
  an ordinary word. A name of nothing but lowercase ASCII letters is refused
  outright (`evidence.rs::is_plain_word`, applied in `ExactGate::admits`):
  rarity cannot tell an uncommon symbol from an uncommon English word, and
  `remaining`, `relevant` and `complete` are all rare in a corpus of function
  signatures. `Foo::Bar`, `sha256` and `Gini` still qualify — a digit or a
  capital anywhere is a spelling the writer did not have to use. A name
  absent from the frequency map counts as rare — the map holds every
  tokenized query term, so an absent name is one no query token equals at all.

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

Nothing here excludes a candidate from the KNOWLEDGE channel — a word that
fails every test still competes on its BM25 score, it just cannot buy the
~80-point exact-symbol boost with a coincidence. The SOURCE channel has a
second floor one level down, at plain lexical candidacy
(`rank_source/candidacy.rs::admits_lexical_evidence`): a source node that
earned no rung is a candidate only when every word of its terminal scope
segment is a surviving query term and that name has more than one word.
Required ids are exempt, and a one-word name falls back to the gate above, so
`` `tokenize` `` still reaches `fn tokenize` while `what does tokenize do` no
longer does. The reason this floor is needed on one channel and not the other
is per-channel stopwording asymmetry: source documents are ~10-token scope and
signature strings holding almost no prose, so the project vocabulary the
knowledge channel drops as ubiquitous — `hooks`, `sessions`, `settings` —
survives in the source corpus and is the most discriminating thing there.
Measured, that let `fs/permissions/hooks.rs#function:configure_loom_hooks` win
tier 2 of "how do I configure the hooks so sessions get the right settings" on
`configure` and `hooks` alone.

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
function checks (the `fuse` module doc).

A source-lane `GraphNeighbor` candidate never holds an exact rung, so it always
fuses in tier 2. Packing the fused list is the next section's subject.

## Packing: Required Items, Rendered Cost and Excerpts

`pack` (`context/pack.rs`) turns the fused list into a `ContextPack` in two
passes, and **there is still no per-channel budget**: a query that matches prose
strongly can legitimately fill the pack with prose. A source item costs tens of
tokens against a knowledge chunk's hundreds, so an unbounded source list crowds
prose out by *slot*, not by token volume.

**Required items first** (`pack/required.rs::reserve_within_budget`). Every
candidate carrying `ExplicitId` — a `--require-id` — is weighed before any
optional one, in the request's `RequiredRepresentation`: `Full` by default (the
whole chunk body or node signature, never truncated), or `Compact` with
`loom knowledge context --require-compact` (the bounded excerpt, marked
`truncated` when cut). One that does not fit becomes an
`UnmetRequirement { id, needed_tokens, available_tokens, reason }` in
`ContextPack::unmet_required`, rendered as a
`Required but unmet: <id> (needs ~N tokens, M available)` line, and
`loom knowledge context` then exits 3. Because an unmet line costs tokens too,
the reservation re-runs with the measured unmet cost held back until the result
fits — at most one extra pass per required candidate — and admitted items are
never evicted to make room for a line. A pack comes out over budget only when the
frame plus the unmet lines alone exceed it; `items` is then empty and the
estimate is pinned to that floor.

**Then optional items** (`select_optional`), walked in fused order, each built
`Compact` and taken only if the brief it would produce still fits. A candidate
with no backing chunk or node, one that does not fit, and a superseded tier-1
twin (below) are all counted in `OmissionSummary::omitted`.

**Rendered cost, not raw size** (`context/render.rs`). An item's `token_count` is
`rendered_item_tokens` — the cost of the text the brief will actually print for
it — and every fit decision prices the whole brief through `tentative_total` →
`rendered_brief_tokens` = `BRIEF_FRAME_TOKENS` (128, the brief's header and
footer) + every item's rendered cost + `rendered_chrome_tokens` (the
`### Knowledge` and `### Source (signature index)` headings, the per-path source
bullet prefixes and joiners, and the unmet lines). `ContextPack::recompute_estimate`
calls the same function, so a budget decision and the published
`estimated_tokens` cannot disagree; `within_budget` is
`estimated_tokens <= budget_tokens`. `MIN_BUDGET_TOKENS` is
`BRIEF_FRAME_TOKENS + 128`, so a configured budget always leaves room above the
frame.

**Match-centred excerpts** (`pack/excerpt.rs::bounded_excerpt`). A body within
`EXCERPT_MAX_TOKENS` (400 estimated tokens, 1 600 bytes at
`BYTES_PER_TOKEN_ESTIMATE = 4`) is quoted whole. A longer one is windowed around
its best-matching line for the surviving query terms: that whole line is always
inside the window (a single over-long line becomes the window and is charged
honestly); the remaining budget is spent half before and half after it,
line-aligned, with whatever one side cannot use going to the other;
`[… earlier lines omitted]` appears exactly when the window starts after the first
byte, and `EXCERPT_TRUNCATION_MARKER` exactly when it ends before the body does.
With no matching line the leading window is used. A window that starts or ends
inside a fenced code block has its fence re-opened, closed or trimmed so the quote
stays well-formed. `ContextItem::truncated` reports whether anything was cut.

**Tier-1 summaries never ride along with their tier-2 detail.** A tier-1 file keeps a 2-8 line summary per topic ending in a link to the topic file under the tier-1 file's stem directory, and `loom knowledge update` scaffolds that topic file with the tier-1 heading verbatim, so the pair shares an anchor and scores nearly identically on the same terms. `pack::twins::tier1_twin` maps a tier-2 chunk id to its tier-1 twin, keying strictly on the tier-2 file's parent directory equalling the tier-1 file's stem, so an unrelated pair that merely shares an anchor is never collapsed. `prose:` ids, deeper paths, empty anchors and source-node ids have no twin. While packing, `details_before_summaries` walks a detail immediately before the summary it duplicates, and `select_optional` drops the summary once the detail is packed, counting it in `OmissionSummary::omitted`. When the detail does not fit the budget the summary is packed as before, which is what makes it a fallback rather than a deletion. A summary the caller named through `--require-id` is exempt from both halves: the request is answered literally, and its `ExplicitId` boost is not allowed to promote a weakly-ranked detail to the head of the pack.

## The Pipeline

```text
knowledge/*.md ──chunker──┐
doc/**/*.md (prose) ──────┼──> KnowledgeChunk ──catalog──> Catalog (revision)
                           │                                    │
                       fingerprint ──> Freshness              ingest
                           │                                    │
                           └──────────> store (.loom/cache/context-v1/) <┘
                                              │
      rank (+ graph expansion) ──> lifecycle filter ──> fuse (two-tier) ──> pack ──> ContextPack
         ▲
 lexical_index (per-revision cache; scan is the oracle)
```

- **One evaluate per retrieval.** `retrieve_for_stage` → `resolve_catalog` →
  `refresh(store, knowledge_root, structural_only = true)`, which runs
  `evaluate_inner` exactly once — one fingerprint pass — and hands those
  fingerprints to the rebuild when the catalog is stale; this path never rebuilds
  the semantic layer. The standalone `evaluate_state` runs only when that refresh
  fails (an in-memory catalog is built instead) or when there is no knowledge
  tree. A stored semantic revision behind `HEAD` is reported stale, and
  `retrieve::graph::load_resolved_graph` also marks it stale when the working
  tree's generation no longer matches the overlay being read.
- `rank` scores each requested channel independently, through the shared BM25 +
  exact-rung machinery described above; the source channel adds its graph
  neighbours before truncation.
- **Lifecycle eligibility** (`retrieve/channels.rs::apply_lifecycle_policy`,
  knowledge channel only): under the default `LifecyclePolicy::Current` only
  `Active` and `Draft` chunks are candidates; `loom knowledge context --history`
  switches to `LifecyclePolicy::Historical`, which also admits `Deprecated`,
  `Superseded` and `Historical`. An `ExplicitId` candidate is always admitted. A
  curated chunk's state comes from its file's frontmatter `state`
  (`loom knowledge annotate --state`), default `Active`. An indexed prose file's
  comes from its path (`catalog/prose.rs::prose_lifecycle`): under a `plans`
  directory, `DONE-` files are not indexed, `REVIEW-` files are `Historical`, and
  `PLAN-` / `IN_PROGRESS-PLAN-` files and anything under `briefs` are `Draft`;
  anywhere, `REPORT-` / `PROPOSAL-` files are `Historical` and anything under an
  `archive` directory is not indexed; an explicit frontmatter state overrides the
  path-derived one. Source nodes are always `Active`.
- `fuse` merges the per-channel lists by **two-tier fusion** (exact-rung
  candidates first by raw score, the lexical remainder by RRF) — see above.
- `pack` reserves required items, then takes optional items in fused order as
  compact, match-centred excerpts while the rendered brief fits, and always
  reports what it left out (`OmissionSummary`, `unmet_required`) — see *Packing*
  above.

**Budgets come from config.** `RetrievalConfig::load` reads the `[retrieval]`
table of the main project root's `.loom/config.toml`: the stage spawn brief
(`orchestrator/signals/retrieval.rs::retrieve_stage_pack`) is packed to
`stage_brief_budget_tokens` (default 3000) and the prompt hook to
`prompt_budget_tokens` (default 1500); an out-of-range value is clamped rather
than rejected. `loom knowledge context --budget-tokens` defaults to 2000.

**One entry point.** `retrieve::retrieve_for_stage` runs the whole pipeline
and is the only way in — `loom knowledge context`, `loom knowledge eval`
(the retrieval evaluation harness, next section), signal generation and the prompt
hook all call it, so a brief rendered at spawn time and a brief pulled by
hand are built the same way (the `context` module doc). Adding a fifth
consumer means calling that function, not reimplementing the pipeline.

## Layering

`context` depends only on `crate::context`, `crate::fs`, `crate::language`,
`crate::models`, one `crate::utils::truncate_for_display` call (inside
`context::untrusted::inline_safe`) and — deliberately — `crate::git`:
`git::runner::run_git_checked` in the three files under `refresh/source_graph/`
(`enumerate.rs`, `generation.rs`, `layer.rs`), needed to list tracked and
untracked files, read committed blobs and fingerprint the working tree. There is
**no** production edge up to `orchestrator`, `commands` or `daemon`: the one
`crate::orchestrator` import is in the test module `context/tests/overlay_key.rs`,
and `crate::commands` appears only in doc comments. Verified with
`rg -o 'crate::[a-z_]+' loom/src/context`, excluding tests and doc comments. Keep
it that way: the orchestrator calls into `context`, never the reverse. An earlier
version of this section put the single `git` edge in `refresh/source_graph.rs`
itself.

## Retrieval Evaluation Harness (A.20)

`loom knowledge eval` (dispatched from
`cli/dispatch.rs::dispatch_knowledge`, implemented in `commands/knowledge/eval.rs`)
scores a checked-in case file (default `loom/eval/retrieval-cases.yaml`) through
`retrieve_for_stage` against the LIVE on-disk index and reports per-case
hit@5/MRR plus aggregate precision@5, exiting non-zero when the aggregate hit
rate falls below the file's `pass_floor`, aggregate precision@5 falls below its
`precision_floor` (0.40, set to 0.9 x the 0.45 measured on 2026-09-19), or any
`forbid` id appears anywhere in a case's results (`eval/report.rs::exit_reason`).
**Which pack is judged depends on the case `mode`** (`eval/metrics.rs::score_case` /
`judged_pack`): `precision_at_5` and `relevant_token_fraction` are scored over the
pack the prompt hook would actually deliver for `mode: prompt` cases (0.0 when the
hook would abstain, computed through `commands/hook/user_prompt_compose.rs::delivered`)
and over the raw retrieved pack for `mode: stage` cases; hit@5, MRR, forbid checks,
mandatory recall and rendered cost always use the raw pack. The reason: each case
judges exactly one relevant id (two for one case) and packs are budget-filled to five
or more items, so raw-pack p@5 is capped near 1/min(5,len) per case and no ranking or
floor change can move it; the delivered-set metric moved from 0.32 to 0.45 with the
naming floor. Raising raw p@5 needs more relevance judgments. `forbid`-only cases are excluded from the
precision denominator so a fixed regression case cannot cap the score
forever, and a case with neither `expect` nor `forbid` fails construction —
it could never fail the run. Deliberately NOT wired into `cargo test`: it
reads the live index, which is not reproducible in CI. It also reports mandatory recall, abstention and rendered cost
(`commands/knowledge/eval/metrics.rs`). `scripts/harvest-eval-cases` drafts further cases from session transcripts
for hand-labeling. `scripts/retrieval-ab` measures precision@5, injected tokens per
brief and hook wall-time percentiles against a baseline binary, routed through one
env-stripping helper so its "isolated" measure root cannot inherit the
calling session's `LOOM_WORK_DIR` (see
[Never Spawn a Surviving Process From a Test](../mistakes/detached-spawn-in-tests.md)
for why an inherited env var made an "isolated" harness mutate the real
checkout).
