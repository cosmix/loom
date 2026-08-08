# Codex Concurrency

> Topic notes for the architecture knowledge area.

## Fan-out cap: 6 (a doctrine number, not a code constant)

**`CODEX_MAX_PARALLEL` is NOT a symbol in this codebase.** It was the spike's variable name and it
survives only in these knowledge files. `loom/src/codex.rs` defines `CODEX_IMPLEMENTER_MODEL_TERRA`,
`CODEX_IMPLEMENTER_MODEL_LUNA`, and `CODEX_IMPLEMENTER_EFFORT`; the cap of 6 is a hardcoded literal inside the signal prose emitted
by `format_codex_implementers_section` (`orchestrator/signals/format/sections.rs:821`). Do not go
looking for a constant to tune — if the cap ever needs to be tunable, add `CODEX_MAX_PARALLEL` to
`codex.rs` and interpolate it there the way the model and effort already are.

**Parallel codex subagents over DISJOINT file sets work.** Verified empirically: 6 concurrent
`codex-companion task` runs in ONE workspace, twice consecutively — every file received its intended
content, no unassigned file was touched, every foreground run returned complete stdout, 0 errors.

6 is the highest value **tested**, not a discovered ceiling. No failure appeared at any concurrency
from 1 to 6, so read this as "6 verified safe", not "6 is the limit".

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

This distinction is the whole verdict, and it was nearly reported the other way. The spike's pass
condition was "correct edits AND intact job records"; those two failures have completely different
blast radii, and letting the conjunction set the headline turned a cosmetic sidecar defect into a
false "do not parallelize" limit. When a pass condition is `A AND B`, check whether A and B fail with
the same consequence before reporting the conjunction as a verdict — report per-property, then judge.

## Doctrine

- **Foreground fan-out over disjoint files: allowed**, up to 6 verified. This is the supported way to buy
  speed from codex.
- **Background fan-out: forbidden.** Background results are retrieved through the very record that gets
  clobbered — the `logFile` pointer that `/codex:result` needs is the field most often lost.
- **Avoid `--resume-last` under fan-out.** It resolves "the last job" out of the corrupted state and can
  attach to the wrong thread. Use fresh runs.
- **Same-file work belongs in separate stages** (worktree isolation), never concurrent subagents in one
  worktree. Disjoint file sets are the precondition for everything above.
- **Expect an "appears hung" warning on long foreground runs.** A foreground codex call is one Bash
  tool call, and the heartbeat only advances on PostToolUse — see
  [Long Codex Runs Starve the Loom Heartbeat](../concerns.md). The warning is advisory; nothing is
  killed or retried.

## Evidence status — what execution did and did NOT add

Be precise about this when extending the page. The plan that SHIPPED the `implementer` lane
(`PLAN-codex-implementer-subagents`) ran none of its own stages on it: all four `.work/stages/*.md`
carry no `implementer` key, so the lane went to merge without ever being dogfooded end to end.

- Still the only multi-run evidence: the 2026-08-06 spike table above.
- Added by integration-verify: a **single** live round trip proving the constant names a real model —
  `codex exec -m gpt-5.6-luna -c model_reasoning_effort=xhigh --sandbox read-only` exited 0 and echoed
  `model: gpt-5.6-luna`. Reachability, not concurrency.
- NOT observed anywhere yet: several `codex:codex-rescue` **subagents** running concurrently inside a
  real loom stage. The spike drove `codex-companion` directly, one level below the subagent wrapper.

Do not write "as observed in execution" about parallel codex implementers until a stage actually runs
with codex listed in `implementers`; check `.work/stages/*.md` for the field before claiming runtime
evidence.
