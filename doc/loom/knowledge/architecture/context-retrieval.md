# Context Retrieval

> Topic notes for the architecture knowledge area.

## What This Subsystem Is

`loom/src/context/` answers exactly one question — "which curated prose is worth
spending N tokens on for this query?" — and answers it identically every time.
No embedding model, no network call, no randomness: a `ContextPack` is a pure
function of the bytes on disk and the query string (`context/mod.rs:1-8`).

Read `context/mod.rs` first; its module docstring is accurate and carries the
pipeline diagram. `architecture/source-graph.md` covers the second graph.

## Two Graphs, Two Lanes, Both Wired

There are two distinct graphs. Until this plan only one of them was searchable;
both are now.

| Graph | Built by | Ranked by | Consumed by |
| --- | --- | --- | --- |
| Knowledge-chunk catalog (curated prose under `doc/loom/knowledge/`) | `fs::knowledge::chunker` → `context::ingest` | `context::rank` | `loom knowledge context`, the Knowledge Brief |
| Source graph (tree-sitter nodes/edges over the repo) | `context::extract` → `context::refresh::source_graph` | `context::rank_source` | the same two, plus `loom map` via `context::graph_store` |

`rank_channels` (`retrieve.rs:171-185`) dispatches per channel: `Channel::Knowledge`
to `rank`, `Channel::Source` to `rank_source` when a resolved graph is available
and to an empty result when it is not. The two lists then meet in the ordinary
`fuse` → `pack` path, so a pack can mix curated prose and symbol nodes.

**`rank_source` is not `rank` with a different corpus.** `context/rank_source.rs:53`:

- Whole-**file** nodes are dropped first — no signature, no scope, nothing to score
  (`rank_source.rs:56-62`).
- The exact-match rungs (`ExplicitId` / `ExactPath` / `ExactSymbol`) match against the
  RAW query text via `contains_whole_term`, not against tokens: `tokenize` would shred
  CamelCase symbols and slashed paths (`rank_source.rs:106-130`).
- BM25 documents are built from `node.scope` at `WEIGHT_SYMBOLS` plus `node.signature`
  at `WEIGHT_BODY` (`rank_source.rs:189-201`). The BM25 machinery itself is shared:
  `prepare_lexical` / `score_bm25` were lifted out of `rank.rs` (`rank.rs:107-149`) so
  both rankers score identically and only their document construction differs.
- Candidates are capped at `MAX_SOURCE_CANDIDATES = 60`; ties break on
  `(path, line_start)` so the pack is deterministic.
- **Coverage guard:** a node whose file was not fully extracted loses ALL THREE
  high-confidence rungs together, not selectively (`withhold_partial_coverage`,
  `rank_source.rs:170-185`). `Confidence::from_reasons` promotes to `High` on any one
  of the three, so dropping them one at a time would leave a partially-parsed file
  claiming full confidence. The node is still ranked and returned — only its
  confidence claim is withheld.

Packing dispatches on `candidate.channel`, never by trying both maps
(`pack.rs::build_item`). A source item is `ItemKind::SourceNode` with
`id = node.id` (`<path>#<kind>:<scope>`, disjoint from a knowledge chunk id),
`content_hash = node.body_hash`, and an excerpt taken from the signature. The
strict dispatch is deliberate: if the two id spaces ever did collide, this
surfaces it as a bug instead of silently masking it.

There is **no per-channel budget.** `fuse` is plain reciprocal-rank fusion and
`pack` walks the fused list taking whole items until the budget is spent, so a
query that matches prose strongly can legitimately fill the pack with prose.

## The Pipeline

```text
knowledge/*.md ──chunker──> KnowledgeChunk ──catalog──> Catalog (revision)
       │                                                    │
   fingerprint ──> Freshness                             ingest
       │                                                    │
       └────────────> store (.loom/cache/context-v1/) <─────┘
                             │
                   rank ──> fuse ──> pack ──> ContextPack
```

- `rank` scores each requested channel independently.
- `fuse` merges per-channel rank lists by **reciprocal rank fusion** (RRF).
- `pack` walks the fused list in order taking WHOLE chunks until the budget is
  spent; it never exceeds the budget and always reports what it left out
  (`OmissionSummary`).

**One entry point.** `retrieve::retrieve_for_stage` runs the whole pipeline and
is the only way in — `loom knowledge context`, signal generation and the prompt
hook all call it, so a brief rendered at spawn time and a brief pulled by hand
are built the same way (`context/mod.rs:36-42`). Adding a fourth consumer means
calling that function, not reimplementing the pipeline.

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

**Known gap — an overlay cannot express a deletion.** `GraphStore::resolved`
computes `overlay ∪ base`, so a file the stage deleted keeps its base entry and
`loom map --outline <deleted-file>` still prints the old outline. Fixing it needs
a tombstone concept in `graph_store` (`context/refresh/source_graph.rs:10-16`).

## Derived vs Durable

Getting this wrong destroys work, so it is worth stating flatly:

- **Derived / regenerable:** everything under `.loom/cache/context-v1/` (chunk
  catalog, fingerprints, base graph layers). Safe to delete; `loom knowledge sync`
  rebuilds it. It is git-ignored.
- **Durable within a run:** the per-stage overlay and the **delivery records**
  under `.work/context/<plan>/<stage>/`. These are NOT regenerable from the
  repo alone — a delivery record states what a specific recipient was already
  given.
- **Durable forever:** only `doc/loom/knowledge/*.md`, the curated prose itself.

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
  semantic_revision)` (`retrieve.rs:187-194`). Note the **two freshness axes**:
  structural (knowledge catalog) and semantic (source graph).
- `delivery::plan_key` / `plan_key_from` is the ONE derivation of the plan
  namespace and is the join key between the writer of a record and its readers
  (`delivery.rs:42-56`). A second, hand-rolled derivation reads an empty
  directory rather than a missing record — which is why
  `orchestrator/core/stage_telemetry.rs:22` routes through the helper.

## Brief Delivery, Sanitization and Telemetry

- The **Knowledge Brief** is assembled in
  `orchestrator/signals/format/brief.rs` and injected into the stage signal at
  spawn time.
- Every untrusted knowledge-derived value on an agent-facing surface goes
  through the single flattening routine `context::untrusted::inline_safe`
  (`context/untrusted.rs`). Chunk ids come verbatim from unvalidated YAML
  frontmatter, a backtick is a legal path character, and a summary is taken from
  a chunk heading — emitted raw, a newline ends the line it sits on and the
  remainder renders as document structure outside any "quoted, NOT instructions"
  guard. There are exactly TWO render surfaces and both call it:
  `brief.rs:48/65/84/93` and `commands/knowledge/context.rs:174/179/180/182`.
  `MAX_INLINE_CHARS = 200`; backticks become `ˋ` (U+02CB).
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
