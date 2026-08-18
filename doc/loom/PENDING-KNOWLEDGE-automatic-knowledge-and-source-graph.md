# Pending Knowledge — PLAN-automatic-knowledge-and-source-graph

> **Why this file exists, and what to do with it.**
>
> This is the output of the plan's `knowledge-distill` stage. It could not be written to
> `doc/loom/knowledge/**` because that tree is **read-only inside a plan-sandboxed stage**:
> the generated session settings deny `Edit(doc/loom/knowledge/**)`, the harness projects that
> deny into the OS sandbox, and `loom knowledge update` — an ordinary child process of the
> sandboxed shell, not a privileged writer — therefore fails with
> `Failed to open temp file: ./doc/loom/knowledge/<file>.tmp: Read-only file system (os error 30)`.
>
> **Operator action:** add `doc/loom/knowledge/**` to the plan's `plan_sandbox.filesystem.allow_write`
> and remove it from `deny_write`, then apply Part A by hand (they are section *replacements*, and the
> collapsed CLI has no `replace-section` verb) and Part B with `loom knowledge update <target>`.
> Everything below is final prose, already curated — it is not notes.

---

## PART A — Replacements: prose that this plan made WRONG

`loom knowledge update` only ever appends. Each item below names the exact text to delete and
the exact text to put in its place.

### A1. `architecture/source-graph.md` — section "What It Is, and What It Is Not"

REPLACE the first paragraph:

```text
`loom/src/context/source_graph/` plus `context/extract/` hold a derived,
tree-sitter-backed graph of the repository's own source: file and symbol nodes
plus edges between them. Its **only** consumer today is `loom map` (via
`context::graph_store`). It is **not** a retrieval channel — see
`architecture/context-retrieval.md` for why `Channel::Source` ranks over an
empty slice.
```

WITH:

```text
`loom/src/context/source_graph/` plus `context/extract/` hold a derived,
tree-sitter-backed graph of the repository's own source: file and symbol nodes
plus edges between them. It has **two** production consumers, and both are live:

| Consumer | Route in | What it reads |
| --- | --- | --- |
| `loom map` (`--outline`, `--find-all`, `--impact`) | `context::graph_store` | the resolved layer, rendered as read-only views |
| the `Source` retrieval channel | `context::rank_source` → `fuse` → `pack` | symbol nodes, scored and fused with knowledge chunks into one `ContextPack` |

The second consumer is new. Before it existed the graph was built, persisted and
given a CLI while `Channel::Source` was ranked over nothing at all — the failure
class is in `mistakes/store-without-consumer.md`, and the ranking design that
closed it is in `architecture/context-retrieval.md`.

Nobody builds this graph by hand any more either. `loom init` and `loom run`
publish a base layer through `advisory_source_graph_preflight`, and every stage's
overlay is reconciled just before its signal is written — see *Lifecycle* below.
```

### A2. `architecture/context-retrieval.md` — section "Two Graphs, Two Lanes — and Only One Is Wired"

REPLACE the whole section (heading included) — it documents a gap that no longer exists — WITH:

```text
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
```

### A3. `mistakes/store-without-consumer.md` — "The Concrete Trail" is now history

The table lists four shapes as unwired. Three of them are wired now. Do **not** delete the
topic — the failure class is the point, and the epilogue is the most useful part of it. Retitle
the table to `## The Concrete Trail (as it was)` and append:

```text
## Epilogue: the consumer was wired, and the trail is how you check

`ItemKind::SourceNode` is now constructed (`pack.rs::build_source_item`),
`Channel::Source` now reaches a real ranker (`context::rank_source`), and the
`ContextItem.excerpt` `None` arm is still unreachable in production. That last one
is the tell: closing a store-without-consumer gap does not retire every shape the
gap created, so the trail table is the checklist you re-walk when the consumer
finally lands. Delete what stayed dead; do not assume wiring the headline item
wired the rest.
```

### A4. Deleted-verb sweep — the knowledge tree still teaches nine commands that no longer exist

`cli-collapse` reduced `KnowledgeCommands` to exactly three variants — `Update`, `Context`,
`Sync` (`cli/types_memory.rs:6-52`; dispatch at `cli/dispatch.rs:83/85/94`). Deleted:
`show`, `list`, `status`, `check`, `audit`, `gc`, `bootstrap`, `init`, `replace-section`.
`README.md`, `CLAUDE.md`, `CLAUDE.md.template`, `skills/`, `commands/`, `hooks/` and `loom/src`
are clean. The knowledge tree is not. Every line below teaches a command that will now exit 2:

| File:line | What it says | What is true now |
| --- | --- | --- |
| `INDEX.md:1` | generated-by marker naming `loom knowledge index` | the marker is `fs/knowledge/index.rs:17-18`; the index regenerates on every knowledge write |
| `patterns.md:9` | "Run `loom knowledge show`" | read the files, or `loom knowledge context --query` |
| `patterns.md:114` | "`replace-section` is the one verb that overwrites in place" | there is **no** overwrite verb; see A6 |
| `patterns.md:191` | `loom knowledge bootstrap` interactive-mode behaviour | deleted wholesale |
| `entry-points.md:454` | "`loom knowledge index` regenerates INDEX.md" | `loom knowledge sync`, or any `update` |
| `entry-points.md:561` | a `loom knowledge status` row | deleted |
| `concerns.md:71` | bootstrap allowlist tightening for `init/check/audit/show/list/gc` | moot — all deleted |
| `concerns.md:242,307,316` | `loom knowledge index` re-run guidance | moot |
| `concerns.md:727` | `replace-section` keeps the heading, replaces the body | moot |
| `architecture/knowledge-hierarchy.md:42,95,99,111` | `index`, `check --min-coverage`, `gc` | `sync` only |
| `architecture/signal-generation.md:72` | signal emits `loom knowledge show` commands | it does not |
| `mistakes/sandbox-and-settings.md:30` | "update stale entries with `replace-section`" | see A6 |
| `mistakes/knowledge-base-drift.md:57,60` | `audit` reports duplicate headers; use `replace-section` | both gone |
| `mistakes/knowledge-cli-invariants.md:8,11,42` | `bootstrap`, `gc`, `index` | gone |
| `mistakes/testing-and-lint.md:122` | regenerate INDEX with `loom knowledge index` and diff | `loom knowledge sync` |

Acceptance for the sweep:

```bash
! rg -q "loom knowledge (check|audit|gc|bootstrap|index|init|list|show|status|replace-section)" doc/loom/knowledge
```

Note the trap in that criterion: `KnowledgeDir::refresh_index_if_hierarchical`
(`fs/knowledge/dir.rs:264-271`) rewrites `INDEX.md` after **every** `loom knowledge update`, and
a `loom` binary built before the collapse writes the old marker back. Re-run the sweep after the
LAST knowledge write, not before it.

### A5. `concerns.md` — "Brief Footer Stage Flag" is false and is quoted into every signal

The entry claims `loom knowledge context` accepts no `--stage`, and that the brief footer's own
suggested command fails with `unexpected argument`. Verified false: `--stage <STAGE>` exists
(`cli/types_memory.rs:20-22`, dispatched at `cli/dispatch.rs:86`) and
`loom knowledge context --stage <id> --query ... --scope source` runs clean. `entry-points.md:560`
repeats it. **Delete both.**

This one matters more than an ordinary stale line, because signal generation quotes concerns
verbatim into every stage's Knowledge Brief: a resolved concern left live is worse than no entry
at all, since it teaches the falsehood to every agent the plan spawns. Rule: **a concerns entry
that describes a broken command must be re-run, not re-read.**

### A6. `patterns.md` / `mistakes/sandbox-and-settings.md` — there is no longer any way to overwrite knowledge in place

`replace-section` was the only verb that overwrote. It is gone, and `loom knowledge update`
appends. Combined with the sandbox rule that makes the tree read-only to file tools, an agent
inside a plan-sandboxed stage can now only ever ADD to knowledge — it cannot correct or delete a
line, which is precisely what distillation step 6 ("remove or update stale knowledge entries")
requires. Record it as the standing limitation it is; see Part C.

---

## PART B — Additions, ready for `loom knowledge update <target>`

### B1. `loom knowledge update architecture/source-graph` — the lifecycle

```text
## Lifecycle: Who Builds It, and When

Nothing in the normal path asks a human to build the graph. There are three
publish points and one fallback, and every one of them is **advisory** — it
reports failure and continues, because a missing graph must degrade retrieval,
never block a run.

| When | Call site | Scope |
| --- | --- | --- |
| `loom init` | `commands/init/execute.rs:187` | `Base`, `allow_overlay_fallback = true` |
| `loom run` (daemon) | `commands/run/mod.rs:101`, in `prepare_background_run` | `Base`, `allow_overlay_fallback = false` |
| `loom run --foreground` | `commands/run/foreground.rs:39`, in `run_startup` | same |
| before a stage's signal is written | `orchestrator/core/stage_executor.rs:429-430` (fresh spawn) and `commands/stage/skip_retry.rs:205` (recovery) | `Overlay { plan, stage }` via `MergeLifecycle::reconcile_overlay` |

`advisory_source_graph_preflight(repo_root, work_dir, allow_overlay_fallback)`
(`commands/run/checks.rs:103-111`) wraps the fallible `publish_source_graph`; on
error it prints one `eprintln!` line and swallows the result. It never returns a
`Result`, so it cannot bail startup — deliberately modelled on
`advisory_codex_lane_preflight`. `publish_source_graph` (`checks.rs:115`) is
idempotent and silent on the common path: it early-returns when a base layer for
`HEAD` already exists (`checks.rs:127-129`).

**Ordering is load-bearing in `loom run`.** The preflight must run BEFORE
`plan_lifecycle::mark_plan_in_progress`: that rename dirties a tracked file, and a
dirty tree always refuses a base publish (`run/mod.rs:96-100`). A publish that
"stopped working" after an unrelated startup reorder is this.

**Recovery signals need their own call.** Signal bytes are embedded once at write
time and `start_stage` later re-uses them verbatim from disk, so a crash/hang
retry that did not reconcile first would hand the agent a stale overlay
(`skip_retry.rs:190-202`). `start_knowledge_stage` deliberately has no reconcile
call — it runs in the main repo with no worktree, and `reconcile_overlay` would
early-return anyway.

**The dirty-tree fallback.** `try_reconcile_semantic`
(`context/refresh/semantic.rs:146-176`) asks `dirty_tree_reason`
(`refresh/source_graph.rs:128-139`, `git status --porcelain=v1 --untracked-files=no`)
first. Clean tree → publish `Base { revision }`. Dirty tree, or the check itself
erroring → build `Overlay` at the address `local_overlay_key(project_root)` owns,
reported as `SemanticLayer::LocalOverlay { plan, stage, refusal }`. A base layer is
immutable and keyed to a revision, so a dirty tree can never publish one; but
publishing NOTHING left the user with no graph at all, and the overlay address is
exactly what retrieval defaults to reading. So `sync` always leaves a usable graph
and always says which one it left.
```

### B2. `loom knowledge update mistakes/writer-reader-address` — new tier-2 topic

```text
# Writer/Reader Address

> Topic notes for the mistakes knowledge area.

## A Fallback That Writes Under a Key No Reader Consults

**What happened:** the working-tree source-graph overlay is addressed by a
`(plan, stage)` pair, and the two sides derived that pair differently. The producer
keyed the stage component on `project_root.file_name()`
(`context/local_overlay.rs:20`), while `WorkDir::project_root()` returns `.` at the
repo root and an ABSOLUTE path from any subdirectory — so ONE tree had TWO
addresses, `_local/map` and `_local/map-<dirname>`. Proven in a fresh clone with no
base layer: `loom knowledge sync` at the repo root wrote `_local/map`, then
`cd loom && loom knowledge context --scope source --json` returned ZERO source-node
items, because it read `_local/map-<dirname>`. It also built two ~16 MB overlays for
one tree.

**Why it is invisible:** `GraphStore::resolved` returns the base layer with NO error
when the overlay key is absent or `None` (`graph_store/mod.rs:251-271`), so a
mismatch degrades to "the last merged revision" instead of failing. The write
succeeded, the read succeeded, the command exited 0, every gate stayed green. A
fallback that WRITES a derived layer under a key no reader consults is
indistinguishable, from outside, from doing nothing at all — and it spends disk and
wall-clock producing the thing nobody reads.

**Prevention:**

1. When a layer is addressed by a composite key, that key belongs to exactly ONE
   shared definition. Here it is `context/local_overlay.rs` — `LOCAL_PLAN_KEY`,
   `local_overlay_stage_name`, `local_overlay_key`, `OverlayScope::resolve`. Two
   derivations that "should agree" is the same bug in a different costume; the
   earlier instance in this codebase is `delivery::plan_key`, where a hand-rolled
   second derivation read an empty directory rather than a missing record (see
   *Delivery Records* in [Context Retrieval](../architecture/context-retrieval.md)).
2. An address derived from a path must be derived from the CANONICAL path.
   `file_name()` on a caller-supplied spelling is an identity bug, not a naming
   choice — and a comment asserting "every caller spells it the same way" is the
   smell, never the proof.
3. **The test that matters is the round trip:** write through the producer's
   address, read through the CONSUMER's, assert the same bytes come back. A test
   that asserts the producer wrote something, or that the reader reads back what the
   test itself planted, proves nothing about the two agreeing.

**Fix:** canonicalize `project_root` before taking `file_name()`, keeping the
un-canonicalized path as the fallback for a path that does not exist yet.

## The Same Shape One Layer Up: The Stage Overlay Key

`orchestrator/signals/retrieval.rs` builds a stage brief's overlay scope with
`stage_overlay_scope(stage)`, whose plan component MUST equal
`delivery::plan_key(stage)` — the address `MergeLifecycle::reconcile_overlay` writes
to. The agreement is spelled out inline at the call site rather than merely
imported, precisely because a mismatch is silent by construction: the brief degrades
to the last merged revision and nothing anywhere reports it.

`OverlayScope::Local` is the WRONG default here and that is worth knowing before you
"simplify" it: the daemon runs from the main repo, so `Local` would resolve against
the main checkout's directory name rather than the stage's worktree.

Residue, not yet fixed: a blank `plan_id` in `.work/config.toml` and a stage record
carrying no plan both resolve to `"default"` through `plan_key`, but the writer side
does not normalize the same way.

## Why This Class Keeps Recurring Here

Silent degradation is the house style in `context/`, on purpose: `load_resolved_graph`
returns `None` on a missing base, a missing overlay or any IO error rather than
failing retrieval. That is right — a cold cache must not break a spawn. The cost is
that EVERY addressing bug in this subsystem presents as "slightly thinner output",
never as an error. In a subsystem that degrades silently by design, the round-trip
test is not a nicety; it is the only detector you have.
```

Tier-1 pointer for `loom knowledge update mistakes`:

```text
## Writer and Reader Disagreeing on One Address

A derived layer written under a key its reader never consults is indistinguishable
from doing nothing, and `GraphStore::resolved` degrades to the base layer with no
error, so every gate stays green. One shared definition of the key, and a round-trip
test through the CONSUMER's address, are the only defences.
→ [Writer/Reader Address](mistakes/writer-reader-address.md)
```

### B5. `loom knowledge update conventions` — the maintainability ledger is a single-owner shared file

```text
## The Maintainability Ledger Is Shared State, and Only One Concurrent Stage May Own It

`loom/maintainability-baseline.txt` is an EXACT-match ledger: it fails when the code
SHRINKS as well as when it grows, and a plain `cargo test` runs it. It is also one
file at one path, shared by every worktree in a plan.

Three consequences a plan author has to design around:

1. **Exactly one CONCURRENT stage may own the ledger.** Two parallel stages that both
   grow or delete ledgered code will conflict on merge, and each will have reconciled
   against a baseline the other invalidated.
2. **A plan that grows or deletes ledgered code without owning the ledger cannot pass
   its own acceptance.** Deleting a ledgered function fails exactly like adding an
   over-long one, so a stage that removes ~4000 lines of orphaned surface MUST also
   hold the ledger.
3. **When a refactor drops an entry under the limit, DELETE the entry rather than
   lowering it.** Lowering keeps a permanent claim on a function that no longer needs
   one.

Before adding lines to any function: `rg '<fn name>' loom/maintainability-baseline.txt`.
If it is listed, refactor rather than extend.
```

### B6. `loom knowledge update conventions` — the test gate is never plain `cargo test`

```text
## `cargo test` Is Not This Repo's Test Gate — `--all-targets --no-fail-fast` Is

Never write plain `cargo test` into a loom plan's acceptance criteria. The gate is:

    cargo test --all-targets --no-fail-fast

Both flags earn their place:

- **`--all-targets` is what compiles `loom/tests/**`.** Without it the external
  integration tests are never built, so a changed signature breaks them and NOTHING
  reports it until somebody runs the full command by hand. Signature changes are
  exactly what a refactor stage produces, which is where this bites hardest.
- **`--no-fail-fast` is what makes the report exhaustive.** Stopping at the first
  failing target hides how much else is red; an agent then fixes one failure, re-runs,
  and discovers the next — one round trip at a time.

Know the two non-hermetic tests, so a red run inside a stage session is not
misdiagnosed as your own breakage. The stage-finalisation tests
(`commands/stage/tests/complete.rs`) route through `sandbox_control_session`
(`control_session.rs:70,94`), which reads `LOOM_STAGE_ID` / `LOOM_SESSION_ID` /
`LOOM_WORKTREE_PATH` from the ambient process environment. Running the suite from
INSIDE a loom worktree session leaves those set, silently routing the call down the
sandboxed worktree path instead of the host-side one the test means to exercise, and
it fails with a wrapper-identity mismatch. It is also order-dependent: it failed in
one full `--all-targets` run and passed in the next.

Re-run with `env -u LOOM_STAGE_ID -u LOOM_SESSION_ID` BEFORE concluding your change
broke it. The durable fix is an RAII env guard at test start — mirroring `EnvGuard`
in `commands/memory/handlers/tests.rs` — that restores on `Drop`, so a panic mid-test
cannot leak state into later tests. Do not apply that fix from an unrelated stage:
touching a file outside your territory is cross-stage merge-conflict bait.
```

### B7. `loom knowledge update mistakes` — acceptance criteria are written from the wrong vantage point

```text
## Write Acceptance Criteria From Inside a Sandboxed Worktree, Not From Your Checkout

Every criterion below looked green and was wrong, and all four failed the same way:
they were authored from the main checkout, where `.work` is a real directory and the
derived cache is writable. In a stage worktree `.work` is a SYMLINK to the main repo
and the plan sandbox denies writes to it, so any criterion whose command writes a
derived cache behaves differently there than where it was written.

| Criterion as written | What actually happens in a stage worktree |
| --- | --- |
| `loom map --outline src/main.rs \| rg -q function` | unsatisfiable — `loom map` called `reconcile_source_graph`, which WRITES an overlay under `.work/context`, so every invocation hard-failed with `Read-only file system (os error 30)` even though a readable base layer existed |
| `loom knowledge sync --json \| rg -q '"semantic":{'` | cannot fail — the denied write returns exit 0 with `{"semantic":{"layer":"skipped",...}}`, so the key is present on a sync that did nothing |
| `$L init >/dev/null 2>&1 \|\| true` then check layers | cannot pass — `loom init` REQUIRES a `<PLAN_PATH>` and exits 2; `\|\| true` turns the usage error into a silent zero-result |
| `rg --files doc/plans/PLAN-x.md > /dev/null && ...` | fails on an absent file — a worktree materialises only TRACKED files, and those sibling plans were untracked |

**Prevention, in the order the failures appear:**

1. **Run every CLI acceptance criterion from inside a stage worktree with the plan
   sandbox ON before shipping the plan.** "Works in my checkout" is not evidence; the
   stage sandbox is the primary environment for these commands.
2. **A read-only CLI verb must degrade when its derived cache is unwritable**, the way
   `context/retrieve.rs:87` `resolve_catalog` already does. `loom map` is documented as
   a read-only view and was writing on every call — that is the bug the criterion
   exposed, not a criterion problem.
3. **Grep for the VALUE that proves work happened, never for a key the degraded path
   also emits.** `'"layer":"base"'` or a non-zero node count, not `'"semantic":{'`.
4. **A wiring test that invokes a CLI verb must pass that verb's required arguments**,
   and must not wrap it in `|| true`.
5. **`git ls-files <path>` every file a stage is told to read or edit, at plan time.**
   An untracked file is invisible to every worktree stage.

**And know that the escape hatch is shut.** `loom stage dispute-criteria` — the only
channel an agent has for "this criterion is impossible" — authenticates over daemon RPC
by reading `.work/user.token`, which the generated stage settings put in `denyRead`. It
dies with `Failed to read .work/user.token for daemon authentication` before any RPC. So
an agent facing an unsatisfiable criterion has no structured escape and falls back to
finishing the stage as CompletedWithFailures, which auto-retries a stage whose criteria no
retry can ever satisfy. When you hit one: say so explicitly in the finishing report and
name `loom stage amend` as the operator fix (`commands/stage/amend.rs`, added by this
plan for exactly this) — do NOT keep working the stage, and never quietly rewrite your
own gate to green.
```

### B8. `loom knowledge update patterns` — three patterns this plan established

```text
## Advisory Preflight: Do the Work, Report the Failure, Never Bail

`advisory_source_graph_preflight` (`commands/run/checks.rs:103-111`) is the second
instance of a shape worth copying, after `advisory_codex_lane_preflight`. The contract
is three rules and no more:

- it returns `()`, never a `Result`, so no caller can accidentally make it fatal;
- on failure it prints ONE `eprintln!` line with a stable prefix and swallows the error;
- it is idempotent and silent on the common path — `publish_source_graph`
  (`checks.rs:127-129`) early-returns when the layer for `HEAD` already exists.

Use it for derived state that IMPROVES a run but must never block one. The signature is
the enforcement: a function that cannot return an error cannot be made load-bearing by a
later caller who forgets it was optional.

## Spool-and-Drain: Writing Through a Sandbox You Cannot Widen

A stage agent's `.work` is a symlink into the main repo and the sandbox denies writes to
it, so `loom memory note` could not reach its own journal. Rather than widening the
sandbox, the write was made asynchronous:

- the agent appends to `<worktree_root>/.loom/memory-spool.jsonl` (`SPOOL_RELPATH`,
  `fs/memory/spool.rs:33`), size-capped at `SPOOL_MAX_BYTES` (1 MiB);
- `record()` (`commands/memory/handlers/record.rs:17`) falls into `record_via_spool`
  ONLY when the direct write failed AND `is_write_denied(&error)` matches
  `PermissionDenied`/EROFS — every other error still propagates unchanged;
- the daemon drains it: `Orchestrator::drain_stage_spools`
  (`orchestrator/core/spool_drain.rs:38`) every tick, plus a teardown drain
  `drain_spool_before_removal` (`git/cleanup/batch.rs:67`) so worktree-removal paths with
  no live orchestrator do not destroy pending entries.

Two design points to keep if you copy it. **The spool payload carries no stage id** — the
daemon attributes entries to the stage that owns the worktree it drained, so an agent
cannot forge another stage's journal (a real prompt-injection channel: a stage's journal
is quoted into that stage's later prompts). And **`drain_stage_spools` enumerates stages
by scanning `.work/stages/` on disk**, not from `active_worktrees`/`active_sessions`:
neither in-memory map survives a daemon restart, so disk is the only source of truth for
a stage recovered as still-Executing.

Known gap: with no daemon running at all, spooled entries stay pending until the next
tick or a teardown drain. `record_via_spool` says so in its own warning
(`record.rs:131`) rather than pretending the write landed.

## Ask Which Surfaces Render the Type, Not Who Copied the Helper

`context/untrusted.rs:5-8` names its call sites in a doc comment — "this has exactly two
call sites, do not add a third copy". That does not prevent a THIRD SURFACE from having
ZERO copies. `loom map` was rewritten into an agent-facing renderer of the same
graph-derived strings — scopes, paths, ids, and a `ParseError` detail built from a raw
line of the offending source file — and flattened none of them.

**Rule:** when a new command renders values an existing renderer flattens, the review
question is "which surfaces render this TYPE?", not "did anyone copy the helper?" — grep
for the type's FIELDS, not for the helper's name. And when you do flatten, route every
variant through it, not only the one that is currently attacker-controlled: uniform
treatment is free (`inline_safe` passes fixed-format strings through unchanged by its own
contract) and it avoids an asymmetry that will catch out whoever adds the next variant.
```

### B9. `loom knowledge update entry-points` — surfaces this plan added

```text
## Source Graph as a Retrieval Channel, and Its Lifecycle (2026-08-18)

| Surface | File | Notes |
| --- | --- | --- |
| `context::rank_source` | `context/rank_source.rs:53` | ranks source-graph nodes for `Channel::Source`; re-exported at `context/mod.rs:64` |
| shared BM25 core | `context/rank.rs:107-149` | `prepare_lexical` / `score_bm25`, now `pub(crate)`, used by both rankers |
| `context/local_overlay.rs` | whole file | the ONE definition of the working-tree overlay address: `LOCAL_PLAN_KEY`, `local_overlay_stage_name`, `local_overlay_key`, `OverlayScope` |
| `advisory_source_graph_preflight` | `commands/run/checks.rs:103-111` | never returns `Result`; called from `init/execute.rs:187`, `run/mod.rs:101`, `run/foreground.rs:39` |
| `SemanticLayer` / `SemanticOutcome` | `context/refresh/semantic.rs:50-64` | what `loom knowledge sync` reports: `base` \| `local-overlay` \| `skipped` |
| `stage_overlay_scope` | `orchestrator/signals/retrieval.rs:~110` | gives a stage brief its own overlay; plan component MUST equal `delivery::plan_key(stage)` |
| `loom stage amend` | `commands/stage/amend.rs` | operator repair of an impossible criterion; thin wrapper over the pre-existing `apply_amendment` (atomic, flock, snapshot + audit row) |
| `criterion_needs_ungrantable_resource` | `plan/schema/validation.rs:647` | plan-time warning when a criterion needs `loom map`, `loom knowledge context`, `tmux` or `docker` — resources a worktree sandbox cannot grant |
| memory spool | `fs/memory/spool.rs:33,59,191` + `orchestrator/core/spool_drain.rs:38` + `git/cleanup/batch.rs:67` | see the spool-and-drain pattern |

`loom map` is now three read-only flags and nothing else: `--outline <PATH>`,
`--find-all <SYMBOL>`, `--impact <SYMBOL_OR_PATH>` (`commands/map.rs:17-28`). `--deep`
and `--focus` are gone, along with `map/{analyzer,detectors,knowledge_sync}.rs`. Note
that the GLOBAL agent doctrine file still documents `loom map [--deep] [--focus <area>]`
— that text is stale against this repo.
```

### B10. `loom knowledge update concerns` — what this plan left open

```text
## Open After PLAN-automatic-knowledge-and-source-graph (2026-08-18)

**Whole-file read ahead of the size cap.** `context/refresh/source_graph.rs:228` does
`fs::read` on every tracked file BEFORE `extract_file` applies the 512 KiB
`MAX_EXTRACTED_FILE_BYTES` cap, so the cap bounds parsing but not allocation, and the
daemon spikes to the size of the largest tracked blob on every merge reconcile.
Deliberately not fixed at the quality gate: `FileExtraction::file_level`
(`extract/mod.rs:103`) needs the BYTES to build the file node's span, so avoiding the
read means changing the oversized node's span semantics or threading a streamed line
count through the extractor API — a hot-path refactor. Peak is one file at a time and
`EXCLUDED_ROOTS` already skips `target/` and `node_modules/`, so the realistic worst
case is a transient spike, not corruption.

**Six production-dead `KnowledgeDir` methods.** Deleting `loom knowledge show`/`list`/
`replace-section` orphaned the whole read/replace side: `read`, `append`, `read_index`,
`read_target`, `replace_section`, `replace_section_target` have no non-test callers, and
all are `pub` on a `pub` type so clippy cannot see them. They were kept because ~15
tests in `tests_dir.rs` exercise them against each other (append → read, replace_section
→ read), so deleting the methods deletes most of that file's coverage. `fs/knowledge/
summary.rs` and `KnowledgeDir::generate_summary` are in the same position — the brief
that justified keeping `summary.rs` named `dir.rs:280` as its caller, but that IS the
wrapper, and the wrapper's only caller was the deleted `show`. **Settle them
deliberately in one follow-up: either delete methods and tests together, or wire them to
a real consumer.** General rule: when a stage deletes a read-side CLI verb, audit every
accessor that verb was the last caller of — and when a brief justifies keeping a module
by naming a caller, check whether that caller is itself reachable. A wrapper is not a
consumer.

**Plan-key normalisation on the writer side.** `delivery::plan_key` resolves both a blank
`plan_id` in `.work/config.toml` and a stage record with no plan to `"default"`;
`MergeLifecycle`'s writer side does not normalise identically. Silent by construction —
see `mistakes/writer-reader-address.md`.

**A permission deny now reaches child processes.** The knowledge tree is denied to the
agent AND to the `loom` binary the doctrine tells agents to use. See Part C of the
pending-knowledge document, and `concerns/sandbox-write-rules-inert.md` for the history.

**`fs/permissions/constants.rs`** still declares `LOOM_PERMISSIONS_WORKTREE` with
`Write(.work/**)` / `Bash(loom *)` rules that read like a blanket grant but have no real
consumers, and `Write(path)` rules are inert anyway. A documented fossil.
```

### B11. `loom knowledge update mistakes` — two verification techniques that paid for themselves

```text
## Mutation-Test the Test, and Re-Prove a Silent-Drop Claim End to End

**Mutation testing is cheap and decisive for this repo's most recurrent defect class.**
After fix agents reported done, each behaviour was broken one line at a time — remove the
`canonicalize`; replace the `File`-kind filter with `true`; neuter the `reasons.is_empty()`
guard; empty the span `push_str`; early-return from `publish_source_graph` — and ONLY the
matching test was run, then `git checkout --` restored it. All five tests went red on
their own mutation and stayed green on the others. That is what distinguishes a real test
from one that merely passes.

**COMMIT BEFORE MUTATING.** Restoring with `git checkout --` otherwise discards the
subagents' uncommitted work, and that is unrecoverable.

**A silent-drop claim needs an end-to-end proof, not a count.** `git ls-files` C-quotes
non-ASCII paths, so the old newline-split-plus-`exists()`-filter dropped them from the
source graph with no diagnostic. The proof: create the pathological name in a scratch
clone, run `git ls-files` to see the WIRE format, then grep the built layer for a symbol
only that file defines. Counting files is not enough — the count can move for unrelated
reasons. (Fix: `git ls-files -z` and NUL splitting.)
```

---

## PART C — The finding this stage produced by failing

### C1. `loom knowledge update mistakes/knowledge-write-channel` — new tier-2 topic

```text
# Knowledge Write Channel

> Topic notes for the mistakes knowledge area.

## The Distillation Stage Cannot Write Knowledge

**What happened:** the `knowledge-distill` stage of
`PLAN-automatic-knowledge-and-source-graph` could not write a single byte of
`doc/loom/knowledge/`. Six of its nine acceptance criteria were unsatisfiable by
construction. Probed from inside the worktree: `README.md` writable, `doc/` writable,
`doc/loom/` writable, `doc/loom/knowledge/` READ-ONLY. `loom knowledge update` fails with
`Failed to open temp file: ./doc/loom/knowledge/<f>.tmp: Read-only file system (os error 30)`.

**Why — and it is a REGRESSION, not a config typo.** Three facts compose:

1. The plan sandbox carries `deny_write = ["doc/loom/knowledge/**"]`, and
   `doc/loom/knowledge/**` is absent from `allow_write`. That rule is CORRECT for
   implementation stages: agents must not hand-edit knowledge.
2. That rule used to be inert. `sandbox/settings.rs` emitted denies as `Write(path)`, and
   Claude Code's file permission check consults only `Edit(path)` rules — see
   `concerns/sandbox-write-rules-inert.md`. Every earlier distillation stage wrote
   knowledge fine *because the guard did nothing*. Correcting the emitter to `Edit(...)`
   (`settings.rs:287`) made the guard real.
3. The harness now projects permission denies down into the OS sandbox, so the block
   reaches CHILD PROCESSES. That is why `loom knowledge update` gets EROFS rather than a
   tool refusal.

**The load-bearing false assumption:** the doctrine "agents update knowledge through
`loom knowledge ...`, never by editing the files directly" (`README.md`) treats the loom
CLI as a PRIVILEGED writer. It is not. It is an ordinary child of the sandboxed shell and
is denied by exactly the same rule as the agent's own tools. There is no privileged
writer anywhere: the daemon protocol has no knowledge RPC (`daemon/protocol.rs` carries
only stage finalisation and dispute verdicts).

**Prevention:**

1. **Any rule of the form "only tool X may write path P" requires X to run OUTSIDE the
   sandbox that denies P.** If X is a child process of the sandboxed session, the rule
   does not gate X — it disables X. Either give X a privileged channel (a daemon RPC, or
   the spool-and-drain shape in `patterns.md`) or put P in `allow_write` for the stages
   that legitimately use X.
2. **Making an inert guard effective is a behaviour change everywhere the no-op was
   load-bearing.** After fixing one, re-run the stage TYPES that depend on writing the
   newly protected path — not just the tests.
3. **Plan-time detection, mechanical:** intersect every stage's `files:` list with the
   plan sandbox's `deny_write` list. A stage told to modify a path it cannot write is a
   PLAN bug, and it surfaces as a stage failure hours later and one worktree away.
4. A `knowledge-distill` stage MUST have `doc/loom/knowledge/**` in `allow_write` and
   MUST NOT have it in `deny_write`.

**And the escape hatch was shut at the same time:** `loom stage dispute-criteria` reads
`.work/user.token`, which the same generated settings put in `denyRead`. An agent that
correctly diagnoses an impossible criterion has no structured way to say so.

## Append-Only Is Not Enough for a Reduce Step

Even with the sandbox fixed, distillation step 6 — "remove or update stale knowledge
entries" — has no tool. `loom knowledge update` appends; `replace-section` was the only
overwrite verb and the CLI collapse deleted it. A distillation stage can therefore ADD
knowledge but cannot CORRECT it, while its own doctrine requires correction and stale
entries actively mislead (a resolved concern left live is quoted into every later stage's
Knowledge Brief).

Whoever owns the next knowledge plan must close this: either restore an
overwrite/delete verb, or make `doc/loom/knowledge/**` writable to distillation stages so
ordinary file edits work. Until then, treat "the knowledge base only ever grows" as a
known property, and put corrections in a clearly-marked superseding section rather than
leaving a silent contradiction.

## Doctrine Baked Into Signals Reaches Only the NEXT Plan

The signal for this stage ordered "ALWAYS finish distillation with `loom knowledge
index`" — a verb this very plan deleted. The merged source already says the opposite
(`orchestrator/signals/cache.rs:590`: the index regenerates on every knowledge write and
there is NO index step). Signal prefixes are rendered by the RUNNING daemon binary, so a
plan that rewrites agent doctrine cannot change the signals of its own later stages.

**Rule:** when a stage signal contradicts the merged source it describes, trust the
source, do not run the deleted command, and say so in the finishing report.

## Verify a CLI Surface Against the Source, Never Against `--help`

The `loom` on `PATH` during this plan predated the merge and still advertised all nine
deleted `loom knowledge` verbs, so `loom knowledge --help` "proved" nothing had been
deleted. Check `loom/src/cli/types_memory.rs` instead.

This has teeth beyond confusion: `KnowledgeDir::refresh_index_if_hierarchical`
(`fs/knowledge/dir.rs:264-271`) rewrites `INDEX.md` after EVERY `loom knowledge update`,
and a pre-collapse binary writes the pre-collapse generated-by marker — which names a
deleted verb, silently re-injecting into `doc/loom/knowledge/` the exact string this
plan's acceptance forbids. Run any deleted-verb sweep AFTER the last knowledge write, and
include `INDEX.md` in it.
```

Tier-1 pointer for `loom knowledge update mistakes`:

```text
## The Channel a Doctrine Names Must Be Privileged, or the Doctrine Disables It

"Only `loom knowledge ...` may write knowledge" assumed the loom CLI was a privileged
writer. It is an ordinary child of the sandboxed shell, so the deny that was meant to
gate hand-edits disabled the distillation stage outright once an inert `Write(...)` rule
was corrected to `Edit(...)`. Fixing a no-op guard is a behaviour change everywhere the
no-op was load-bearing.
→ [Knowledge Write Channel](mistakes/knowledge-write-channel.md)
```

### C2. Operator checklist

1. Add `doc/loom/knowledge/**` to `plan_sandbox.filesystem.allow_write` and remove it from
   `deny_write`, for any plan ending in a `knowledge-distill` stage.
2. Re-run this stage, or apply Parts A and B by hand. Part A items are section
   REPLACEMENTS and need a text editor; Part B items are `loom knowledge update <target>`.
3. Rebuild and reinstall `loom` from `main` before the next run — the binary on `PATH`
   predates this plan's merge.
4. `CONTRIBUTING.md` does not exist in this repository and has never been tracked
   (`git log -- CONTRIBUTING.md` is empty), although this stage was told to update it and
   the sandbox grants write to it. Either create it deliberately or drop it from plan
   file lists.
5. `loom review` was run and wrote
   `doc/plans/REVIEW-PLAN-automatic-knowledge-and-source-graph.md` (58 KB) — but
   `.gitignore:46` ignores `doc/plans/REVIEW-*`, so the copy produced inside a stage
   worktree is destroyed with the worktree and never reaches `main`. Re-run `loom review`
   from the main repository if you want to keep it.
