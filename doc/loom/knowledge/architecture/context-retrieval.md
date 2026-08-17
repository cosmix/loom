# Context Retrieval

> Topic notes for the architecture knowledge area.

## What This Subsystem Is

`loom/src/context/` answers exactly one question — "which curated prose is worth
spending N tokens on for this query?" — and answers it identically every time.
No embedding model, no network call, no randomness: a `ContextPack` is a pure
function of the bytes on disk and the query string (`context/mod.rs:1-8`).

Read `context/mod.rs` first; its module docstring is accurate and carries the
pipeline diagram. `architecture/source-graph.md` covers the second graph.

## Two Graphs, Two Lanes — and Only One Is Wired

There are two distinct graphs, and the honest description of their relationship
matters more than either one:

| Graph | Built by | Consumed by | In ranking? |
| --- | --- | --- | --- |
| Knowledge-chunk catalog (curated prose under `doc/loom/knowledge/`) | `fs::knowledge::chunker` → `context::ingest` | `context::rank`, `loom knowledge context`, the Knowledge Brief | YES |
| Source graph (tree-sitter nodes/edges over the repo) | `context::extract` → `context::refresh::source_graph` | `commands::map` via `context::graph_store` | NO |

`Channel` (`context/schema.rs:48-53`) has exactly two variants, `Knowledge` and
`Source`, and `Channel::all()` puts BOTH in the default path. But `rank` only
accepts `&[KnowledgeChunk]`, so `rank_channels` in `retrieve` ranks `Source`
over an **empty slice** (`context/mod.rs:27-34`). Every emitted pack therefore
names a scope it never searched. Bridging graph nodes into the ranker is
separate, unbuilt work — ranking `Source` over the same catalog would
double-count the knowledge chunks.

**Do not describe the source graph as a retrieval channel.** Three module
docstrings did exactly that and had to be corrected (see
`mistakes/store-without-consumer.md`).

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
