# Codex Concurrency

> Topic notes for the architecture knowledge area.

## CODEX_MAX_PARALLEL = 6

**Parallel codex subagents over DISJOINT file sets work.** Verified empirically: 6 concurrent
`codex-companion task` runs in ONE workspace, twice consecutively — every file received its intended
content, no unassigned file was touched, every foreground run returned complete stdout, 0 errors.

6 is the highest value **tested**, not a discovered ceiling. No failure appeared at any concurrency from
1 to 6, so read this as "6 verified safe", not "6 is the limit".

The real constraint is on **background** mode, not on parallelism — see Doctrine.

## Measured

2026-08-06, plugin 1.0.6, `--write --model gpt-5.6-luna --effort xhigh`, throwaway git repo, each task
assigned a **different** file (disjoint sets — the case that matters for subagent fan-out).

| Concurrency | Runs | File edits | Wrong file touched | Foreground stdout | `state.json` job records |
|---|---|---|---|---|---|
| 1 | 2 | all correct | none | intact | intact — all 18 fields, `logFile` present |
| 2 | 2 | all correct | none | intact | field loss on 1 of 2 records |
| 3 | 2 | all correct | none | intact | field loss on 2-3 of 3 records |
| 6 | 2 | all correct | none | intact | all 6 records present, field loss on 5 of 6 |

**Edits and results were correct at every concurrency tested.** The only casualty is the plugin's job
bookkeeping.

## What actually degrades: the shared state.json sidecar

`scripts/lib/state.mjs`:

- `updateState()` is `loadState` → mutate → `saveState` with **no lock** (state.mjs:118-122).
- `saveState()` re-reads `previousJobs` and, for every previous job absent from the set it is about to
  write, calls `removeJobFile(...)` and `removeFileIfExists(job.logFile)` (state.mjs:105-112) — it can
  delete the job file and live log of a sibling it cannot see.
- State is per workspace root (`$CLAUDE_PLUGIN_DATA/state/<slug>-<hash>/`, else
  `os.tmpdir()/codex-companion/...`), keyed by git root, so all concurrent tasks in one worktree share
  ONE `state.json`.

`scripts/lib/job-control.mjs` is multi-job by construction and imposes no concurrency cap.

Observed damage is **field-level loss, not record loss**: an `upsertJob` merge onto a stale in-memory base
overwrites fields a sibling already committed. Worst case seen (concurrency 2) reduced a record to
`["createdAt","updatedAt","id","phase"]`. `logFile` is the most frequent casualty. The record/log
*deletion* path above is real code but never fired in a successful run. The concurrency-1 control — same
script, same repo, one process, every field intact — is what proves this is caused by concurrency and not
by the test harness.

## Why this does not block parallel fan-out

The `codex:codex-rescue` wrapper returns codex's **stdout verbatim**; a foreground result never travels
through `state.json`. So sidecar corruption costs only `/codex:status`, `/codex:result` and
`/codex:cancel` — observability, not correctness. Edits are written by codex directly to the working tree
and were correct in every run.

## Doctrine

- **Foreground fan-out over disjoint files: allowed**, up to 6 verified. This is the supported way to buy
  speed from codex.
- **Background fan-out: forbidden.** Background results are retrieved through the very record that gets
  clobbered — the `logFile` pointer that `/codex:result` needs is the field most often lost.
- **Avoid `--resume-last` under fan-out.** It resolves "the last job" out of the corrupted state and can
  attach to the wrong thread. Use fresh runs.
- **Same-file work belongs in separate stages** (worktree isolation), never concurrent subagents in one
  worktree. Disjoint file sets are the precondition for everything above.
