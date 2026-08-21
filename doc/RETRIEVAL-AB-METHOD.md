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
A.12). This checkout's own cache records a semantic revision several commits
behind HEAD, so that trigger is live, not hypothetical.

To keep the measurement honest (never mutating the real index) and safe
(never spawning an uncontrolled background rebuild on a machine that was
just rebooted from an OOM), every invocation runs with its working directory
inside an isolated **measure root** — a plain directory with no `.git`, one
per binary, seeded once from a snapshot of this checkout's real
`doc/loom/knowledge/` and `.loom/cache/context-v1/` so both binaries score
against the identical index (the code is the only variable under test, not
the data). Each measure root gets an empty `.work/` marker (so
`WorkDir::new`'s upward search can't escape to the real `.work/`), a patched
`state.json` (`semantic.stale = false`), and a periodically-refreshed
"just finished" `reconcile.lock`, so the background reconcile never has a
live reason to fire. See the header comment in `scripts/retrieval-ab` for
the full reasoning — it is long on purpose, because getting this wrong
either corrupts the measurement or repeats the RAM-exhaustion incident this
whole harness was commissioned to avoid triggering again.

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
  stored state).
- jq's `EXPR | index(.)` is a footgun: piping into `EXPR` rebinds `.` to
  `EXPR`'s result *before* `index(.)` evaluates its argument, so a loop
  written as `[items[] | select((set | index(.)) != null)]` silently checks
  "does `set` contain itself" (always true for non-empty `set`) instead of
  "does `set` contain this item." The loop variable must be captured with
  `as $x` and referenced by name; `.` does not survive an intervening `|`.
  This is also why `def a: ...; def b: ...; body` needs no `|` between the
  `def`s or before `body` — a leading `|` there is a syntax error, not a
  no-op.
