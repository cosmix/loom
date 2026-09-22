# Context Retrieval State

> Base/overlay graph layers, delivery records

## Base vs Overlay Ownership

This is the rule that keeps parallel worktrees from corrupting each other (the
`graph_store` module doc). Parallel stages run in separate worktrees off one
repository; if they shared a mutable graph a stage would see HALF of a sibling's
edits — worse than seeing none, because there is no way to tell which half.

| Layer | Location | Keyed by | Mutability |
| --- | --- | --- | --- |
| **base** | `.loom/cache/context-v1/graph/base/<revision>.json` under the canonical MAIN project root, shared by every worktree | the commit it describes | written once, thereafter immutable; old bases are pruned only when a new one is published |
| **overlay** | `graph.json` in `.loom/work/context/<plan>/<stage>/` | plan + stage — a real stage, or `_local` / `map-<dir>` for a checkout's working tree | rewritten by its owner; holds only the files that differ from the base |

A read is `overlay ∪ (base − overlay's files)`. An overlay entry shadows the base
entry for the same path **wholesale, never merges with it** — partial merges
produce a graph that describes no revision that ever existed — and an overlay
tombstone (`FileCoverage::Deleted`) removes the path from the view entirely. The
known gap an earlier version of this section recorded, that an overlay could not
express a deletion so a deleted file kept its base outline, is closed; see
[Source Graph](source-graph.md) for how tombstones are built and filtered.

`graph_store` owns only the layout, the layering rule and canonical
serialization. It never builds a graph (`context::refresh` does) and never
decides *when* to write one (`refresh::ensure_snapshot` and
`reconcile_source_graph` do).

**Which overlay a query reads** is an `OverlayScope`: the stage spawn brief reads
its stage's own overlay (`OverlayScope::Stage`, plan from `delivery::plan_key`);
the CLI reads `OverlayScope::Local`; the prompt hook reads whatever its
`HookTarget` resolved — the stage overlay inside a stage, `Local` in a plain
checkout.

**A missing base is not automatically a degraded pack.** `GraphStore::resolved`
substitutes an empty base when no base file exists for the recorded semantic
revision. Bases are built from committed `HEAD` content even in a dirty checkout
(`ensure_snapshot`), with working-tree changes carried by the `_local` overlay, so
a missing base usually means nothing has published one for this `HEAD` yet.
`ContextPack::degraded` (A.11) fires only for the narrower case: a non-empty
semantic revision that NEITHER the base nor any overlay can back at all, so the
resolved graph has no content whatsoever (`retrieve/graph.rs::degraded_reason`).
Widening that predicate to "any missing base" was tried and reverted — it flagged
every healthy checkout as degraded permanently, and
`reconcile_graph::spawn_if_needed` triggers on `stale OR degraded`, so it also
started a detached full-repository tree-sitter rebuild on every single prompt in
every working checkout. See `degraded_reason`'s own doc comment for the
reconcile-trigger consequence before widening this predicate again. Separately,
the read marks the semantic layer stale — without degrading the pack — when the
overlay's `generation` no longer matches the working tree, or the tree is dirty and
no overlay exists; that too wakes the background reconcile.

## Derived vs Durable

Getting this wrong destroys work, so it is worth stating flatly:

- **Derived / regenerable:** everything under `.loom/cache/context-v1/` (chunk
  catalog, fingerprints, base graph layers, the persistent lexical index).
  Safe to delete; `loom knowledge sync` rebuilds it. It is git-ignored.
- **Durable within a run:** the per-stage overlay and the **delivery records**
  under `.loom/work/context/<plan>/<stage>/`. These are NOT regenerable from the
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
  propagated (the `delivery` module doc).
- Suppression is scoped to a **`context_epoch`**: once a derived layer is
  rebuilt the same id may describe different bytes, so every record from an older
  epoch is ignored and delivery re-opens.
- `context_epoch` = first 8 bytes of `sha256(structural_revision \n
  semantic_revision)`, hex-encoded (`retrieve.rs::context_epoch`). Note the **two
  freshness axes**: structural (knowledge catalog) and semantic (source graph).
- `delivery::plan_key` / `plan_key_from` is the ONE derivation of the plan
  namespace and is the join key between the writer of a record and its readers.
  A second, hand-rolled derivation reads an empty directory rather than a missing
  record — which is why `orchestrator/core/stage_telemetry.rs`,
  `orchestrator/signals/retrieval.rs` and the hook's `HookTarget` all route
  through the helper.
- **The prompt hook keys its own dedupe per SESSION, not per checkout (A.16)**,
  through `context/delivery/session.rs` (`delivery::session`, a separate file; an
  earlier version of this bullet called it a submodule of the same file):
  `hook_recipient_id`, `delivered_to_session` and `discard_session_delivery`. A
  stage's own spawn-brief delivery record is keyed by loom's session id under
  `plan_key`/stage id, as above; the hook's recipient is
  `prompt-<stage-or-checkout-key>-<session8>`, where `session8` is the first 8
  bytes of `sha256(session_id)` from the hook payload, hex-encoded — a DIFFERENT
  id space, hashed because the raw id is untrusted input that becomes a file name
  (`nosession` stands in when the payload names none). When the hook runs inside
  the session a stage spawned, `delivered_to_session` also counts that spawn
  record, found through `LOOM_SESSION_ID`. Without the per-session split, a fresh
  Claude Code session with an empty context window inherited every prior
  session's deliveries and went silent on topics it had never actually seen.
  `loom hook pre-compact` deletes just that session's own record after a
  compaction, when the context that held the brief is gone (A.21).

## Brief Delivery, Sanitization and Telemetry

- The **Knowledge Brief** is assembled in
  `orchestrator/signals/format/brief.rs` and injected into the stage signal at
  spawn time. It renders as a `### Knowledge` section (curated + indexed
  prose, fenced excerpt plus reason line) followed by a `### Source (signature
  index)` section (one unfenced bullet per file, symbols/spans/reasons
  inline, consecutive items on the same path merged onto one bullet). The merge
  uses the same `render::source_groups` runs the packer charges chrome for, so
  the rendered brief and the budget agree. The "quoted, NOT instructions" guard is
  stated once in the header rather than once per item, which is where most of
  the per-item token overhead used to go.
- Every untrusted knowledge-derived value on an agent-facing surface goes
  through the single flattening routine `context::untrusted::inline_safe`
  (`context/untrusted.rs`). Chunk ids come verbatim from unvalidated YAML
  frontmatter, a backtick is a legal path character, and a summary is taken from
  a chunk heading — emitted raw, a newline ends the line it sits on and the
  remainder renders as document structure outside any "quoted, NOT instructions"
  guard. The module doc names three surfaces: the brief
  (`orchestrator/signals/format/brief.rs`), `loom knowledge context`'s stdout
  (`commands/knowledge/context.rs`), and the daemon's status payload
  (`commands::status::data::sanitize`), where a surviving ESC would be an ANSI
  sequence the operator's terminal obeys; `loom knowledge check` and
  `loom knowledge telemetry` flatten their untrusted fields through it as well.
  An earlier version of this bullet said there were exactly two surfaces.
  `MAX_INLINE_CHARS = 200`; backticks become `ˋ` (U+02CB).
- **The prompt hook** (`commands/hook/user_prompt.rs`, run by
  `user-prompt-context.sh`) resolves its scope through `HookTarget`
  (`commands/hook/target.rs`): inside a stage it reads that stage's OWN overlay
  (`OverlayScope::Stage`, plan from `delivery::plan_key`) and keys delivery to the
  stage; outside one it reads the checkout's `_local` overlay. It retrieves with
  `prompt_budget_tokens`, then applies its gates in order. `parse_prompt` declines
  machine-generated payloads before retrieval runs (task-notification XML, and the
  "Background agent" and "Caveat:" prefixes), strips `@` file attachments, and
  requires 24 characters of question. **Per-item admission** then drops every
  item that does not clear the floor on its own — an exact-rung reason, or at least
  `config.min_knowledge_terms` (default `2`) distinct query terms that NAME the item —
  a knowledge chunk's heading or aliases (`rank/candidacy.rs::named_terms`), a source
  node's matched name — for ANY item, not only knowledge chunks. Rescued terms and
  function words never count, and a body-only match never clears the floor ("no, use
  the repository version of those files, not the installed copies" matched two or more
  terms in hundreds of chunks, and two in the heading of none). Exception: a `GraphNeighbor` item, which is
  admitted while the retrieved pack still holds an exact-rung item; dropped items
  fold into `omitted`. The per-prompt brief strips the renderer's `Omitted: N weaker
  matches.` footer line (`user_prompt_compose.rs::without_omitted_line`): an unsolicited
  brief gains nothing from a count of what it was not given. `loom knowledge context`
  and stage briefs keep it. Next the session's per-epoch dedupe drops what this session
  was already handed, and finally the payload must fit `config.max_payload_bytes`
  (default `16384`), shedding the weakest item until it does. The hook
  **abstains** — prints nothing — with reason `floor` (nothing admitted, or what
  survives dedupe no longer clears the floor), `all-delivered`, or
  `over-ceiling`; a retrieval error is `no-retrieval` and an unresolvable
  environment `no-target`. The floor applies to UNSOLICITED injection only —
  `loom knowledge context`, `loom knowledge eval` and the stage spawn brief are
  deliberately not gated. After printing, or abstaining on a retrieved pack, the
  hook nudges the detached source-graph reconcile when the pack is stale or
  degraded.
- **Telemetry** (`loom/src/telemetry/mod.rs`) appends one JSON line per event to
  the state directory's `telemetry/events.jsonl`; a sandboxed session whose
  direct write is denied spools to its worktree's `.loom/telemetry-spool.jsonl`,
  which the daemon drains alongside the memory spool. Events: `context-delivered`
  / `context-unavailable` (one per spawned session, from
  `stage_telemetry::record_context_telemetry` after the stage executor spawns it),
  `prompt-brief` / `prompt-abstained` (the prompt hook), and `context-pulled`
  (`loom knowledge context`). Best-effort by contract — `emit` never fails a
  caller, and `read_events` skips a malformed line. Counts are ITEM and
  estimated-token counts, never a saving. `loom knowledge telemetry` reads them
  back per stage (`telemetry::summary::summarize`), and `archive_run_state` copies
  the telemetry directory under `.loom/memory/archive/` before a finished plan's
  state directory is removed. An earlier version of this bullet said the events
  had no reader and were deleted at plan finalization.

**Stub chunks are not candidates.** `rank/candidacy.rs::admits` drops a knowledge chunk
with an empty heading and fewer than `STUB_MIN_LINES` non-blank body lines, unless the
query requires its id. Hand-built ranker fixtures with `heading: String::new()` and a
one-line body were silently excluded; `context/tests/rank_fixtures.rs` uses a
punctuation-only heading (`TERMLESS_HEADING`, which tokenizes to nothing) so the pinned
BM25 arithmetic stays intact.
