# Retrieval A/B measurement method

How `scripts/retrieval-ab` measures the three numbers
`doc/PROPOSAL-retrieval-precision.md` section 5 names — precision@5 on the
eval set, mean injected tokens per brief, and p95 hook wall time — for the
baseline (commit `d06fd2c2`, the last commit before this work started)
against the current working tree.

## What is measured

| # | Metric | Source |
| - | --- | --- |
| 1 | precision@5 / MRR / forbid violations | `loom/eval/retrieval-cases.yaml` (13 cases), scored against `<bin> knowledge context --query ... --json` for both binaries with the SAME per-case query and budget. |
| 2 | Mean injected tokens per brief | `<bin> hook user-prompt` replayed over `scripts/retrieval-ab-prompts.txt` (27 prompts), token estimate = `byte_len(additionalContext) / 4` (`BYTES_PER_TOKEN_ESTIMATE`, `loom/src/context/schema.rs`), reported alongside how many prompts produced NO brief (silence is a deliberate A.5 outcome, not a failure). |
| 3 | p95 hook wall time | The same replay, 3 repetitions per prompt, timed with `date +%s%N`; the first invocation per binary is reported separately as cold-start, p50/p95/max computed over the rest. |

The headline number is **forbid violations**: the five seeded regression
cases in `retrieval-cases.yaml` are specimens straight from the proposal's
section 2 (prose/symbol collisions, a filesystem path, machine-generated
noise) and are expected to go from violating to clean.

The scoring rules (hit@5, MRR, forbid) are mirrored from
`loom/src/commands/knowledge/eval.rs`, which is the reference implementation
— `eval.rs` is authoritative; `scripts/retrieval-ab` is a mirror written in
`jq` because the baseline binary predates the `loom knowledge eval`
subcommand entirely and cannot run it.

## Running it

```bash
# 1. Build both binaries (never run by the script itself - see its printed
#    instructions, which include the exact commands below). Run the current
#    build AFTER the working tree has settled, not mid-edit.
MIN_AVAIL_GB=32 scripts/guarded-cargo.sh \
    bash -c 'git worktree add --detach /tmp/loom-retrieval-ab/baseline-src d06fd2c2 && \
             CARGO_TARGET_DIR=/tmp/loom-retrieval-ab/target-baseline \
             cargo build --release --manifest-path /tmp/loom-retrieval-ab/baseline-src/loom/Cargo.toml'
mkdir -p /tmp/loom-retrieval-ab/bin
cp /tmp/loom-retrieval-ab/target-baseline/release/loom /tmp/loom-retrieval-ab/bin/loom-baseline

MIN_AVAIL_GB=32 scripts/guarded-cargo.sh \
    cargo build --release --manifest-path loom/Cargo.toml \
    # (with CARGO_TARGET_DIR=/tmp/loom-retrieval-ab/target-current)
cp /tmp/loom-retrieval-ab/target-current/release/loom /tmp/loom-retrieval-ab/bin/loom-current

# 2. Run the harness. Prints a markdown report to stdout and
#    /tmp/loom-retrieval-ab/report.md.
scripts/retrieval-ab

# 3. Tear down the git worktree and scratch state when done.
scripts/retrieval-ab --clean
```

`--baseline-rev <rev>` overrides the baseline commit; `--out <dir>` overrides
the scratch directory (default `$TMPDIR/loom-retrieval-ab`, deliberately
outside the checkout — see "Isolation" below).

## Isolation

`loom knowledge context` and `loom hook user-prompt` are not read-only: every
call opportunistically refreshes the structural catalog cache and files a
delivery-dedupe record, and the CURRENT binary may additionally spawn a
**detached background `loom hook reconcile-graph`** — a full tree-sitter
rebuild of the source graph — whenever the pack it just built reports the
semantic layer stale or degraded (`context::reconcile_graph::spawn_if_needed`,
A.12).

To keep the measurement honest (never mutating the real index) and safe
(never spawning an uncontrolled background rebuild on a machine that was
just rebooted from an OOM), every invocation runs with its working directory
inside an isolated **measure root** — a plain directory with no `.git`, one
per binary, seeded once from a snapshot of this checkout's real
`doc/loom/knowledge/` and `.loom/cache/context-v1/` so both binaries score
against the identical index (the code is the only variable under test, not
the data). Each measure root gets an empty `.work/` marker (so
`WorkDir::new`'s upward search can't escape to the real `.work/`), a patched
`state.json` (`semantic.stale = false`, and `semantic.revision` repointed at
a base graph layer the snapshot actually has — see "Why isolation is hard
here" below), and a periodically-refreshed "just finished" `reconcile.lock`,
so the background reconcile never has a live reason to fire. Every binary
invocation is also routed through `run_bin`, which strips the harness's own
inherited `LOOM_WORK_DIR`/`LOOM_STAGE_ID`/`LOOM_SESSION_ID` before exec'ing.
The run's last act, `verify_real_checkout_untouched`, fingerprints the real
checkout's own cache/work-directory mtimes before seeding and again after
scoring and fails loudly if either moved. See the header comment in
`scripts/retrieval-ab` for the full reasoning — it is long on purpose,
because getting this wrong either corrupts the measurement or repeats the
RAM-exhaustion incident this whole harness was commissioned to avoid
triggering again.

### Why isolation is hard here

An earlier version of this harness looked isolated — measure roots, a
`.work` marker, a patched `state.json` — and still silently measured against
the real checkout. Two independent gaps combined into a third, and the
resulting numbers were wrong enough to invert two conclusions (an apparent
current-binary latency regression that was actually concurrent contention,
and a "clean" precision@5 comparison in which the source channel never fired
for either binary). All three are fixed now; this section exists so the next
person who touches this file understands why the fixes look the way they do
before "simplifying" one away.

1. **The environment goes around the measure root, not just the CWD.**
   `LOOM_WORK_DIR`/`LOOM_STAGE_ID`/`LOOM_SESSION_ID` are ordinary environment
   variables, set in the invoking session and inherited by every child
   process by default. `loom hook user-prompt`/`reconcile-graph`/
   `pre-compact` all resolve their work-dir hint from `LOOM_WORK_DIR` FIRST,
   falling back to the current directory only when it is unset
   (`non_empty_env("LOOM_WORK_DIR")` in
   `loom/src/commands/hook/{user_prompt,reconcile_graph,pre_compact}.rs`).
   `cd`-ing into an isolated measure root does nothing to stop an inherited
   `LOOM_WORK_DIR` from resolving straight past that root's own `.work`
   marker to the REAL `.work/` — delivery-dedupe records get written into
   the real checkout, and the source channel reads the real overlay instead
   of the mirror's. **Fix:** every binary invocation now routes through
   `run_bin`, the single choke point that strips these three variables with
   `env -u` before exec'ing (a child-only removal — it never touches the
   harness's own environment).

2. **A degraded pack is also a spawn trigger, not just a stale one.**
   `reconcile_graph::spawn_if_needed` fires on `stale OR degraded`. Patching
   `state.json`'s `.semantic.stale = false` only disarms half of that gate.
   A checkout's `state.json` can name a `semantic.revision` for which
   `graph/base/<revision>.json` was never published — base layers are
   published on `loom map`/merge, not eagerly, so `state.json` can outrun
   `graph/base/`'s own contents even in a healthy, actively-developed
   checkout. When that happens, `GraphStore::resolved()` reads a silently
   EMPTY base (`unwrap_or_default()` over a missing file, by design — see
   `context/retrieve/graph.rs`'s doc comment), which
   `degraded_reason` turns into `pack.degraded = Some("source graph base
   <rev8> missing — serving overlay only")` regardless of the stale patch.
   With that trigger live, a plain `loom hook user-prompt` or
   `knowledge context` call spawned a detached full source-graph rebuild
   against whatever `LOOM_WORK_DIR` resolved to — which, combined with gap 1,
   was the REAL checkout. The current binary's timing phase ran entirely
   under that concurrent rebuild, which is where its apparent latency
   regression came from; a clean re-measurement showed no regression at all.
   **Fix:** `seed_measure_root` now repoints the copied `state.json`'s
   `.semantic.revision` at whichever base-layer file the snapshot's
   `graph/base/` directory actually has (picked by file mtime, not
   hardcoded — `graph/base/` prunes old layers, so a fixed revision here
   would go stale the next time this script runs). When no base layer is
   available at all, it falls back to only patching `.semantic.stale` and
   prints an explicit warning; the report's "Source channel" line always
   states which case applied — never read a report without checking it.

3. **Consequence of gap 2: the source channel was never exercised.**
   With no base layer, `graph.base_revision` was always empty, so zero
   source-node ids appeared in results for EITHER binary — the eval cases
   that depend on the source channel were unwinnable for both, which reads
   as "fair" (equal for both binaries) but is actually a blind spot: neither
   binary's source-retrieval code path ran at all. Fixing gap 2 fixes this
   for free, since a present base layer means `resolved()` returns real
   nodes and edges instead of an empty graph.

### Known-good invocation

```bash
# Build (see "Running it" above for the exact commands this prints).
scripts/retrieval-ab   # prints build instructions and exits 1 if a binary is missing

# Run. Prints the report to stdout and $OUT/report.md, then fails loudly
# (exit 1) if the real checkout's own cache/work state moved during the run.
scripts/retrieval-ab
```

`.loom/cache/` and `.work/` are both gitignored, so `git status` cannot see
into them — it cannot stand in for the isolation check. The harness's own
stderr, from `verify_real_checkout_untouched`, is the authoritative signal;
do not invent a second one. Read the run's stderr for two things:

- A line like `real checkout's own session-retrieval records: N before, N
  after (delta 0; ...)` and NO `ISOLATION BREACH` block. A nonzero delta by
  itself is not proof of a breach (a concurrent, unrelated Claude Code
  session in this same checkout can legitimately add those records while
  this harness runs) — only a moved `catalog.json`/`state.json`/
  `.work/context` mtime is proof, and that path always exits 1.
- The report's "Source channel" line reading `exercised — ...`, not `NOT
  exercised`. The latter means every source-dependent eval case was scored
  as a miss for BOTH binaries, not a real result.

```bash
# Tear down.
scripts/retrieval-ab --clean
```

## Known weaknesses (read before trusting the numbers)

- **13 eval cases is a small sample.** Precision@5 on 13 cases moves in
  1/N-sized steps (roughly 8 percentage points per case, fewer once
  forbid-only cases are excluded from the denominator); treat the number as
  directional, not a tight confidence interval.
- **27 prompts is a small, hand-picked corpus**, not a random sample of real
  usage. It is deliberately weighted toward the proposal's own failure
  specimens, so it will *overstate* the visible improvement relative to a
  uniform sample of real traffic — that skew is intentional (the point is to
  demonstrate the fixes work on the cases they target) but should not be
  read as "the average prompt sees this much of a token/precision swing."
- **Percentiles over ~8-80 warm samples per binary are noisy.** p95 over a
  couple dozen local, sub-second invocations is sensitive to scheduler
  jitter; treat p95 deltas smaller than a few milliseconds as noise, and
  weight the cold-start and over-ceiling counts more than the exact p95
  figure.
- **The measure-root cache snapshot is a point-in-time copy.** If the real
  checkout's `.loom/cache/context-v1` is itself stale, incomplete, or was
  captured mid-edit by a concurrent process, both binaries inherit that
  equally — fair to the A/B comparison, but not necessarily representative
  of a freshly-`loom map`'d checkout.
- **The baseline binary predates `RetrievalConfig`, A.4's machine-prompt
  skip, and A.5's emit floor.** For the `machine` and `weak` prompt classes
  this is exactly what is being measured (the baseline is EXPECTED to
  retrieve against noise the current binary declines), not a methodology
  flaw — but it means baseline's `mean tokens / brief` and `silent` counts
  are not apples-to-apples with current's for those two classes, only within
  each binary's own row.
- **jq is a mirror of `eval.rs`, not the same code.** They were tested
  against matching synthetic fixtures while this harness was written, but a
  future change to `eval.rs`'s scoring rules (hit@5, MRR, or forbid
  semantics) will silently desynchronize the two unless this file's jq is
  updated to match — there is no shared-code guarantee here, only intent.
- **No `--structural-only` isolation for the catalog refresh.** Every
  `knowledge context`/`hook user-prompt` call opportunistically refreshes
  the structural (knowledge markdown) catalog if the tree fingerprint
  changed; on a freshly-seeded measure root this happens once, on the first
  invocation, and is cheap (markdown chunking, not the source graph) — but
  it does mean the very first invocation per measure root pays a small
  extra cost folded into whatever sample happens to land first, separate
  from the explicit cold-start sample metric 3 already carves out for wall
  time.

## Insights for the knowledge base

- `WorkDir::new` (`loom/src/fs/work_dir.rs`) tries `<hint>/.work` first and
  only walks upward past it as a fallback. A directory living anywhere under
  a checkout that itself has a `.work/` will silently resolve to the REAL
  `.work/` unless it gets its own (even empty) `.work/` marker first. This
  matters for any future tooling that runs a `loom` binary against an
  isolated copy of a checkout's state.
- `reconcile_graph::spawn_if_needed` (A.12) fires off retrieval, not just
  off explicit `loom map`/`sync` calls — a plain `loom hook user-prompt` or
  `loom knowledge context` invocation against a checkout whose semantic
  revision has drifted from HEAD can trigger a detached full source-graph
  rebuild. Anything that shells out to those commands against a real
  checkout for measurement or testing purposes needs the same debounce-lock
  seeding this harness does, or needs to run against an isolated copy with
  no `.git` (so `evaluate()`'s HEAD comparison always falls through to
  stored state). The trigger is `stale OR degraded` — a `state.json` with
  `stale = false` can still be degraded if its `semantic.revision` names a
  `graph/base/<revision>.json` that was never published; patching staleness
  alone is not enough (see "Why isolation is hard here" above).
- `LOOM_WORK_DIR`/`LOOM_STAGE_ID`/`LOOM_SESSION_ID` are read from the
  process environment, not passed as CLI flags, by
  `loom hook user-prompt`/`reconcile-graph`/`pre-compact`
  (`non_empty_env` in `loom/src/commands/hook/*.rs`). They are ordinary
  environment variables and so are inherited by every child process by
  default — `cd`-ing a subprocess into an isolated directory does not stop
  an inherited `LOOM_WORK_DIR` from resolving it straight past that
  directory's own `.work` marker to wherever the *parent* session's
  `LOOM_WORK_DIR` pointed. Any tool that shells out to these hook
  subcommands against an isolated copy must explicitly `env -u` all three
  (or otherwise clear them) at the exact call site, not just change the
  child's working directory.
- jq's `EXPR | index(.)` is a footgun: piping into `EXPR` rebinds `.` to
  `EXPR`'s result *before* `index(.)` evaluates its argument, so a loop
  written as `[items[] | select((set | index(.)) != null)]` silently checks
  "does `set` contain itself" (always true for non-empty `set`) instead of
  "does `set` contain this item." The loop variable must be captured with
  `as $x` and referenced by name; `.` does not survive an intervening `|`.
  This is also why `def a: ...; def b: ...; body` needs no `|` between the
  `def`s or before `body` — a leading `|` there is a syntax error, not a
  no-op.
