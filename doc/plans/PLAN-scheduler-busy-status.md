# Plan: Report a busy scheduler as busy, not stalled

Implements: none

## Overview

`loom status`, the TUI and the web dashboard report "orchestrator loop stalled" during normal
runs. The loop is not hung. Long, legitimate work on the scheduler thread does not refresh
`.loom/work/orchestrator.tick`, which is written only when the loop enters a phase, so the
60-second staleness rule fires. The alert then tells the operator to restart the daemon, and a
restart only repeats the same work. This plan keeps that work on the scheduler thread. Each long
operation is wrapped so that it keeps the tick fresh and records its name. The dashboards then
report "busy" with the operation and its elapsed time, and keep "stalled" for a loop that has
stopped advancing.

## Evidence

- **2026-11-07, run of `PLAN-fewer-disputes`.** The tick went stale at 13:52:06 UTC with phase
  `spawning`, and the run recovered by itself at 13:52:21.
  - The loop was building the knowledge brief for stage `criteria-authoring` inside
    `write_agent_signal` (`loom/src/orchestrator/core/stage_spawn.rs`) →
    `generate_contract_signal` → `retrieve_stage_pack` (`loom/src/orchestrator/signals/retrieval.rs`).
  - This took about 71 s and wrote a 320 MB `view.json`.
- **Plan 05.** The same rule fired for 187 s in the sync phase while `verify_resolved_completion`
  ran a resolved merge's acceptance suite on the scheduler thread.
  - The call sites are `loom/src/orchestrator/core/inbox_drain/merge_resolved.rs` and
    `loom/src/orchestrator/core/merge_handler/resolver_exit.rs`.
  - `doc/plans/PLAN-autonomous-recovery.md`, group G7, records this run.
- **Provisioning already avoids the problem.** It re-stamps the tick from a helper thread
  (`loom/src/orchestrator/core/provision_gate.rs`, `run_with_fresh_tick`).
- **The threshold comment assumes otherwise.** The comment on `STALL_THRESHOLD_SECS`
  (`loom/src/orchestrator/tick.rs`) assumes slow work never reaches 60 s, and that assumption no
  longer holds.

## Decisions

1. **The diagnosis.** The reported stalls are false alarms raised during long legitimate work,
   not hangs. Most clear by themselves.
2. **The work stays on the scheduler thread.** Moving brief generation and merge verification
   off the thread (as stall-recovery's `JobKind::Land` would) is out of scope, chosen as the
   cheaper option. The scheduler still cannot react to anything else while one of them runs;
   only the reporting changes.
3. **One shared helper.** `busy::while_busy` generalizes provisioning's helper thread: it
   re-stamps the tick every 10 s while a wrapped operation runs. Three operations are wrapped:
   - brief generation at stage spawn, for implementation, contract and knowledge sessions. At
     an implementation or contract spawn the wrap covers two calls, each in its own sequential
     `while_busy`: the source-graph overlay refresh (`refresh_stage_overlay` in
     `stage_spawn.rs`, a full walk on a fresh worktree, read only by the brief) and
     `write_agent_signal`;
   - resolved-merge verification, at both call sites;
   - provisioning, moved onto the helper with one `while_busy` per provision entry, so each
     wrap encloses exactly the unit `PROVISION_TIMEOUT` bounds.

   The calling thread writes the first three-line stamp itself, before it starts the helper and
   calls the operation, so the busy line is on disk the moment the operation starts. Every tick
   write (`tick::record`, `tick::record_busy`) goes to `orchestrator.tick.tmp` and is renamed
   over `orchestrator.tick`, because a reader that catches a truncated file reads "tick
   unknown" or a cut stage id. The brief wrap always stamps phase `spawning`, also when the spawn
   is reached from the Events phase (a contract writer's exit spawns the implementation session
   through `contract_phase.rs`); the clearing write leaves `spawning` until the next phase entry.
   This is accepted: passing the phase down would touch `stage_executor.rs` and
   `contract_phase.rs` for a label.
4. **The tick file names the running operation.** It gains an optional third line,
   `busy <op> <started_at> <stage_id>`. A malformed third line reads as "not busy" and never
   hides the tick.
5. **Ceilings.** Each wrapped operation has its own ceiling, and past it the loop reads "busy,
   longer than expected" as a Warning. A global ceiling covers every wrapped operation, and past
   it the loop reads "stalled" as Critical, with the restart advice.
   - Brief generation: 5 minutes.
   - Resolved-merge verification: 20 minutes.
   - Provisioning: 10 minutes, matching `PROVISION_TIMEOUT`.
   - Global: 30 minutes. It must exceed merge verification's 20 minutes while that wrap exists;
     autonomous-recovery's stall-recovery lowers it to 20 minutes when its Land job removes the
     wrap (see "Watchdog thresholds").
   - Unwrapped work keeps the 60-second stale-tick rule, and a stale tick is Critical even while
     an operation is recorded as busy, which catches a helper thread or process that has frozen.
   - Each ceiling is per wrapped call. `PROVISION_TIMEOUT` (`orchestrator/provision.rs`) bounds
     one provision command, and `run_provision` runs the entries in order, so provisioning wraps
     each entry separately: every entry gets its own `started_at`, and no number of slow
     entries can reach the 10-minute ceiling, the 30-minute global ceiling, or the downstream
     watchdog's 20-minute one while each entry stays inside its own timeout.
   - Resolved-merge verification runs the stage's whole acceptance suite, whose own budget
     (`verify::budget::stage_command_secs`) can exceed 30 minutes. A verification past the
     global ceiling reads Critical "stalled" with the restart advice. This is accepted until
     autonomous-recovery's Land job takes verification off the thread: the observed run took
     187 s, and verification runs in the stage's worktree, whose build is already warm.
6. **One classifier, every surface.** `busy::loop_state` decides Unknown, Ticking, Busy or
   Stalled once. `scheduling_report::alerts`, the `loom status` headline and the TUI header use
   it. The web dashboard applies the same rules to a `busy` payload that carries both ceilings.
   Only Critical alerts carry the restart advice. The `loom status` headline takes its stalled
   or busy state from `loop_state`, never from "any Critical alert" (today's
   `alerts.iter().any(..Critical)` would call the loop stalled on an unrelated Critical alert),
   and applies it only while the daemon is Running or Unreachable, as the stalled override does
   today: a daemon killed mid-operation leaves a three-line tick behind.
7. **Independent of the live test lane; autonomous-recovery depends on this plan.** This plan
   does not depend on `PLAN-live-test-lane` or `PLAN-autonomous-recovery`. Autonomous-recovery
   must run after it, on a refined plan (see "What autonomous-recovery keeps"). Nothing
   mechanical enforces that order: the operator must not `loom run` autonomous-recovery until
   the refine has landed.
8. **Out of scope:**
   - the self-restart watchdog;
   - time limits on lock waits on the scheduler thread;
   - moving work off the thread;
   - the size of the per-stage knowledge `view.json`.

   The first three stay with autonomous-recovery's trimmed stall-recovery stage (see "What
   autonomous-recovery keeps"). The view size is left for a later plan.
9. **Merge verification is wrapped only until the Land job exists.** This plan wraps it because
   it runs on the scheduler thread today. Autonomous-recovery's Land job takes it off the thread
   and removes the wrap.
10. **The embedded web bundle is rebuilt in integration-verify.** `web/dist` is committed and
    compiled into the binary (`loom/build/assets.rs`), and the web snapshot schema is
    `.strict()` (`web/src/api/schema.ts`). The daemon sends `busy` on every frame, so an old
    bundle rejects every frame and the header reads "connection error"
    (`doc/loom/knowledge/mistakes/plan-authoring.md`, "Web stages left the embedded bundle
    stale"). As in earlier web plans, only integration-verify commits a bundle: it rebuilds
    `web/dist` before its final review round, commits it last as `chore(web)`, and proves it
    with `bundle-check.mjs` and a scratch `vite build` diffed against `dist`.
11. **New wire values are guarded.** `busy` is a full-only key of the web feed
    (`commands/status/web/feed_split.rs`, `FULL_ONLY_KEYS`): it changes only when an operation
    starts or ends, and each change sends one `full` frame. `BusyOp`'s wire names join the
    enum parity test (`commands/status/web/schema_parity.rs`), so a later variant cannot make
    the strict schema reject frames while it runs.

## Run order and sibling plans

| Plan | Relation |
| --- | --- |
| `IN_PROGRESS-PLAN-fewer-disputes` | Its code stages are merged; no shared code file. Both knowledge-distill stages edit `doc/loom/knowledge/**` (`INDEX.md`, `check-baseline.txt`) and `README.md`, so this plan's knowledge-distill runs after fewer-disputes' knowledge-distill has merged. |
| `PLAN-autonomous-recovery` | Runs after this plan, and only after the refine described in "What autonomous-recovery keeps". Its stall-recovery stage is cut down and rebuilt on this plan's busy line and `loop_state`. Its spawn-recovery stage edits `stage_spawn.rs` and `stage_knowledge.rs`, which this plan also edits, so those briefs are re-anchored on this plan's tree in the same refine. |
| `PLAN-live-test-lane` | Its live check reaches `merge_resolved.rs` and `resolver_exit.rs` through `verify_resolved_completion`; this plan only wraps that call, so the two compose. |
| `PLAN-loom-ui-mods` | Its loom-ui snapshot computes `daemon.stalled` as tick age at or past `STALL_THRESHOLD_SECS` (`doc/plans/briefs/loom-ui-mods/loom-ui/common.md`, the `daemon` field). After this plan a hung wrapped operation keeps the tick fresh, so that rule would read healthy. Run it after this plan, and refine that field to come from `busy::loop_state` (optionally carrying the busy payload) before it runs. |
| `PLAN-web-host-graft-followthrough` | Its stage `files:` glob `loom/src/commands/status/web/**` covers this plan's edits of `model.rs`, `model_tests.rs`, `feed_split.rs`, `feed_split_tests.rs` and `schema_parity.rs`; its workers own other files there (listener, connection, access, http). Do not run the two at the same time; either order works. |
| `PLAN-model-router-hooks` | Its `integration-verify` stage owns `loom/src/**`, a superset of this plan's Rust files (its implementation stages own `loom/src/commands/router/**`, `cli/` and `fs/permissions/`, none of which this plan touches). Do not run the two plans at the same time, so that its integration fixes never land in this plan's territory mid-stage; either order works. |
| `PLAN-planner-background-activity-and-skills` | A draft without stages. Its token-flow rename touches `agentFlowSchema` in `web/src/api/schema.ts` and the snapshot fixture, which this plan also edits; a textual conflict only, either order. |

## Execution Diagram

```mermaid
graph LR
    busy-tick --> integration-verify
    integration-verify --> knowledge-distill
```

No knowledge-bootstrap stage: `doc/loom/knowledge/architecture/status-data-model.md` already
describes the tick, the 60-second rule and the alert pipeline this plan changes.

## Stages

### 1. busy-tick

All the code is in one stage: the shared helper, the tick format, the three wrapped call sites,
the alerts, the `loom status` headline, the TUI header and the web daemon line. It splits into
three workers in two waves:

- **W1 (wave 1)** builds the foundation that W2 compiles against.
- **W2 and W3 (wave 2)** run in parallel: W2 does the Rust surfaces (and the web fixture the
  Rust fixture test compares against), W3 the web package and the Rust test that checks the
  web enum.

They write disjoint files. The Rust and TypeScript work share only the payload shape fixed in
`doc/plans/briefs/scheduler-busy-status/busy-tick/common.md`. No question of the stage necessity
test forces a second implementation stage. The stage does not rebuild `web/dist`;
integration-verify does (Decision 10).

Risk checklist walk:

| Area | Applies because | Contract |
| --- | --- | --- |
| Untrusted input | the third tick line is read from disk and its stage id is rendered in a terminal | `malformed-busy-line-reads-as-not-busy` |
| Lifecycle and concurrency | a helper thread writes the tick while the caller runs; a panic must not leave the busy line behind; the helper must keep re-stamping | `busy-line-clears-after-panic`, `busy-line-names-op-and-stage-while-running`, `busy-line-restamps-while-running` |
| Configuration propagation | per-operation ceilings must differ in effect, not only in value | `per-operation-ceilings-differ` |
| Reachability | the dashboards must see the new state | `busy-under-ceiling-is-not-stalled`, `global-ceiling-is-critical`, `stale-tick-is-critical-while-busy`, `web-snapshot-carries-the-busy-line`, the three web contracts, and literal wiring on every wrapped call site |
| External data correctness | the web client parses the Rust snapshot | `web-snapshot-carries-the-busy-line` freezes the serialized `busy` object; W2 pins the fixture in `model_tests.rs` and the enum values in `schema_parity.rs`; W3 parses the same shape; the orchestrator compares them; integration-verify ships the rebuilt bundle |

Not applicable: filesystem paths and symlinks (the tick path is fixed), process I/O volume.

### 2. Integration Verification

Full Rust suite, doctests, clippy, fmt, rustdoc, the full web checks, code review, and a functional check
that each wrapped call site is the one on the scheduler path. It rebuilds `web/dist` before its
final review round and commits the bundle last (Decision 10).

### 3. Knowledge Distillation

Records the busy line, the ceilings and the classifier in the knowledge base, plus the mistake:
the stall rule assumed slow work never reaches 60 seconds.

## What autonomous-recovery keeps

Settled with the user on 2026-11-07. `PLAN-autonomous-recovery`'s stall-recovery stage is cut
down to three parts: the restart watchdog, the time limits on lock waits, and the `Land` host
job. It reuses this plan's tick format and classifier. This plan's session may write only its own
files, so a refine session on `PLAN-autonomous-recovery` makes those edits; until then that plan
must not run. The changes it needs, checked against its briefs in
`doc/plans/briefs/autonomous-recovery/stall-recovery/`:

| Part of stall-recovery | Change |
| --- | --- |
| C1, the `process::activity` module | Dropped. This plan's busy line names the long operations. No other scheduler-thread operation is known to exceed 60 s once the Land job and the lock limits are in, but that is not proven: a bounded git mutation may take up to 120 s (`git/runner.rs`, `GIT_MUTATION_TIMEOUT`) and still reads stalled until it returns, and `reconcile_base` after a merge and the `before_stage` checks in `pre_spawn_gates_passed` are unmeasured. |
| W1, the operation stack as the third tick line, and `run_tick` step labels | Dropped. The third line is this plan's `busy` line; two different third lines cannot coexist. |
| W1, lock waits limited to 30 s | Kept. A 30 s wait stays under the 60 s stale-tick rule, so it never reads as a stall. Pressure-review finding 6 (`codex-PLAN-autonomous-recovery.md`) still applies: a timed-out lock wait must be logged and retried next tick, never propagated out of `run_tick`, or the daemon exits before the watchdog can act. |
| W1, alert wording | Rewritten on top of `scheduling_report_loop.rs`: the Critical text replaces "restart with `loom stop`, then `loom run`" with the automatic-restart note (or the refused-restart reason). |
| W2, the `Land` host job | Kept. It removes this plan's two `BusyOp::MergeVerification` wraps, since verification no longer blocks the thread, and drops that variant and its ceiling. Dropping the variant touches `busy.rs` (`as_str`, `parse`, `ceiling_secs`, `describe`, `MERGE_VERIFICATION_CEILING_SECS`), `busy_tests.rs`, the `scheduling_report_loop.rs` tests, the `z.enum` in `web/src/api/busy-schema.ts`, `all_busy_ops` in `commands/status/web/schema_parity.rs`, and a rebuilt `web/dist`. It also lowers `GLOBAL_BUSY_CEILING_SECS` from 1800 to 1200 (see "Watchdog thresholds"). Pressure-review findings 4 (`fail_closed`) and 5 (durable delivery) still apply. Its acceptance probe `! rg -q verify_resolved_completion src/orchestrator` already fails on a doc comment in `merge_handler/resolver_handback.rs`; narrow it to the call form `verify_resolved_completion(` or reword the comment. |
| W3, the watchdog | Kept, but it keys on `busy::loop_state` instead of the raw tick age. The helper thread keeps the tick fresh during a wrapped operation, so a tick-age rule would never see a hang inside one. Thresholds and the exact rule: see "Watchdog thresholds". Its restart reason's operation text (`Stall.operation` / `RestartReason::Stalled` in spawn-recovery's restart brief) is `busy.op.describe(stage_id)` when a busy line is present, else the phase name. |
| Stage gates | Its `before_stage` adds `test -f src/orchestrator/busy.rs` (this plan has merged). |

The refine must also remove or rewrite every remaining site that still builds the dropped
pieces, or the trimmed stage completes against contracts for code nobody builds:

- PUBLIC SURFACE entries for `process::activity`, `tick::record_operation`,
  `tick::install_recorder` / `uninstall_recorder`, `Tick { .., operation }` and the alert text
  "drain_session_inboxes > git merge-tree";
- contracts `bounded-run-names-its-operation`, `operation-name-is-one-line` and
  `stall-alert-names-the-operation`;
- wiring `tick::install_recorder(` (run.rs) and `activity::enter(` (process/mod.rs), the
  artifact `src/process/activity.rs` and the `src/process/activity*.rs` files entries;
- the C1 and W1 operation-stack rows of its worker table, its Decision 16, its G7 table rows
  and every "900" (Decisions, G7, the stall-recovery description, `w3-watchdog.md`);
- its risk-table claim that the TUI and web "read `tick_age_secs` only" (after this plan the
  TUI reads `LedgerView.loop_state` and the web reads `WebSnapshot.busy`);
- its references to `stalled_alert` and `Tick::is_stalled` in `scheduling_report.rs`: the
  alert is `scheduling_report_loop.rs`'s `loop_alert`, and `Tick::is_stalled` stays in
  `tick.rs` as rule 2 of `loop_state`.

The watchdog needs three contracts of its own on top of the moved 599 s / 600 s probes: a fresh
tick with a busy line under the global ceiling never restarts; a busy line past the global
ceiling restarts at once; a stale tick at 599 s and 600 s behaves the same with and without a
busy line.

This plan's frozen contracts use only `brief-generation` and `provisioning`. That way the Land job
can delete `MergeVerification` without changing a frozen contract of a finished plan.

One dependency stays as it was. The watchdog needs spawn-recovery's restart facility, and the
Land job's live-receipt check needs `PLAN-live-test-lane`. So the trimmed stall-recovery still
waits for the live-test-lane gates that autonomous-recovery already has. Of the three parts, only
the lock limits have no such dependency.

### Watchdog thresholds

Settled with the user on 2026-11-07. The stall-recovery brief today restarts the daemon at 900 s
of tick age. On top of this plan the watchdog uses two rules instead:

- **Stale tick (unwrapped work hung): restart at 600 s.** The longest single bounded wait the
  scheduler thread issues is a git mutation (120 s, `git/runner.rs` `GIT_MUTATION_TIMEOUT`);
  a lock wait (30 s) is shorter. Git's 300 s network timeout has no scheduler-thread caller
  (the daemon runs no fetch, push, pull or clone). 600 s leaves room for several chained
  bounded waits in one phase. The brief's `STALL_RESTART_SECS` becomes 600 and its contract
  `watchdog-restarts-only-past-the-limit` moves its 899 s / 900 s probes to 599 s / 600 s. In
  `LoopState` terms: restart when the state is `Stalled` and its `age_secs` is 600 or more. A
  `Stalled` state between 60 s and 599 s of tick age never restarts.
- **Wrapped operation hung: restart as soon as `busy::loop_state` reports Stalled past the
  global ceiling**, with no grace period: the state is `Stalled` with `busy` set and
  `elapsed_secs` at or past `GLOBAL_BUSY_CEILING_SECS`, while the tick is fresh.
- **The global ceiling drops to 20 minutes** (twice provisioning's 10), in the same stage whose
  Land job removes `MergeVerification`, the only wrapped operation longer than that. This plan's
  frozen contract `global-ceiling-is-critical` probes 1900 s, past either value, so it still
  holds after the change.

An early restart loses no work: a restart redoes brief generation, and an interrupted provision
re-runs on the still-Queued stage. The 3-restarts-an-hour budget bounds a hang that comes back.

## Open questions

None open.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  ratchet_files:
    - loom/maintainability-baseline.txt
    - doc/loom/knowledge/check-baseline.txt
  provision:
    - working_dir: "web"
      command: "test ! -e .npmrc && test ! -L .npmrc && bun install --frozen-lockfile --ignore-scripts --backend=copyfile --config=/dev/null"
  sandbox:
    enabled: true
    auto_allow: true
    filesystem:
      deny_read:
        ["~/.ssh/**", "~/.aws/**", "~/.config/gcloud/**", "~/.gnupg/**"]
    network:
      allowed_domains:
        [
          "crates.io",
          "index.crates.io",
          "static.crates.io",
          "registry.npmjs.org",
        ]
      allow_local_binding: false
      allow_unix_sockets: []
  stages:
    - id: busy-tick
      name: "Busy operations keep the tick fresh and are reported as busy"
      summary: "Brief generation, resolved-merge verification and provisioning keep the scheduler tick fresh while they run and record their name in it; loom status, the TUI and the web dashboard show 'busy <operation>' with its elapsed time, warn past a per-operation ceiling, and report a stalled loop only on a stale tick or past a 30-minute global ceiling."
      stage_type: standard
      skills: ["loom-rust", "loom-concurrency", "loom-typescript", "loom-react"]
      implementers: ["claude"]
      working_dir: "."
      dependencies: []
      description: |
        Build stage 1 of doc/plans/PLAN-scheduler-busy-status.md. Read its Decisions first, then doc/plans/briefs/scheduler-busy-status/busy-tick/common.md, which is authoritative for every internal signature, the tick file format and the wording table. This description is authoritative for the PUBLIC SURFACE the frozen contracts call.
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is the repository root; Rust lives in loom/ (cargo --manifest-path loom/Cargo.toml), the dashboard in web/. Locate every seam by symbol (loom map --outline, --find-all); line numbers in briefs are advisory.
        PUBLIC SURFACE (the contract session writes loom/tests/scheduler_busy_contracts.rs and web/src/lib/daemon-line-busy.test.ts from this text before any code; Rust contracts use the public API only and write tick files by hand in a TempDir; contracts name only BusyOp::BriefGeneration and BusyOp::Provisioning, never MergeVerification, so a later plan can delete that variant; the Rust contract file obeys the maintainability limits that loom/tests/maintainability.rs applies to loom/tests/ too: at most 400 lines per file and 50 per function, so share a tick-writing helper):
        - loom::orchestrator::busy (new pub module): BusyOp { BriefGeneration, MergeVerification, Provisioning } (Debug, Clone, Copy, PartialEq, Eq) with as_str() -> &'static str ("brief-generation", "merge-verification", "provisioning"), parse(&str) -> Option<BusyOp>, ceiling_secs() -> i64 (300, 1200, 600) and describe(stage_id: &str) -> String ("generating the brief for <stage>", "verifying the merge of <stage>", "provisioning <stage>"); pub const GLOBAL_BUSY_CEILING_SECS: i64 = 1800; Busy { op: BusyOp, stage_id: String, started_at: DateTime<Utc> }; while_busy<T>(work_dir: &Path, phase: loom::orchestrator::tick::Phase, op: BusyOp, stage_id: &str, f: impl FnOnce() -> T) -> T, which writes the three-line tick (phase plus busy line) on the calling thread before calling f, re-stamps it from a helper thread every 10 s with the same started_at, and after f returns or panics leaves a two-line tick with no busy line; #[doc(hidden)] pub fn while_busy_every<T>(work_dir: &Path, phase: Phase, op: BusyOp, stage_id: &str, interval: std::time::Duration, f: impl FnOnce() -> T) -> T is the same with a caller-chosen re-stamp interval (while_busy calls it with 10 s).
        - loom::orchestrator::tick: the file is <work_dir>/orchestrator.tick; line 1 an RFC 3339 timestamp, line 2 the phase (sync, spawning, events, idle), optional line 3 'busy <op> <started_at RFC 3339> <stage_id>' with single spaces. Tick { recorded_at: DateTime<Utc>, phase: Option<Phase>, busy: Option<Busy> } (all pub). read(work_dir) -> anyhow::Result<Option<Tick>>: a third line with a prefix other than 'busy ', an unknown op, an unparseable started_at, an empty stage id, a stage id over 128 bytes or holding any character other than ASCII letters, digits, '-', '_' and '.' gives Some(tick) with busy None; it never returns Ok(None) or Err because of line 3.
        - loom::orchestrator::scheduling_report::alerts(work_dir: &Path, daemon_running: bool) -> Vec<Alert> (unchanged signature; Alert { severity: Severity, text: String }; Severity { Info, Warning, Critical }). In a bare TempDir it returns only the loop alert (no scheduling.json, no guard record). With daemon_running true and the tick per the table in common.md: tick fresh, busy elapsed 60 s or more and under the op's ceiling gives one Info alert whose text contains 'orchestrator busy' and the op's describe(stage) text, and no Critical alert, no text containing 'stalled' and no text containing 'loom stop'; busy at or past the op's ceiling and under 1800 s gives a Warning containing 'longer than the expected', no Critical and no text containing 'loom stop'; busy at or past 1800 s gives a Critical containing 'orchestrator loop stalled', the describe text and 'loom stop'; a tick 60 s or older gives a Critical containing 'orchestrator loop stalled' whether or not line 3 is present.
        - loom::commands::status::web::model::collect_snapshot(work_path: &Path, status: loom::status::data::StatusData, source: SnapshotSource, terminals: bool) -> WebSnapshot (unchanged signature; StatusData derives Default; SnapshotSource { Daemon, Files } in the same module). WebSnapshot serializes (serde_json::to_value) with a top-level "busy" key: null when the tick has no valid busy line, else { "op": op.as_str(), "what": op.describe(stage_id), "stage_id", "started_at" (RFC 3339 of the busy line's started_at), "ceiling_secs": op.ceiling_secs(), "global_ceiling_secs": 1800 }. It is filled from the tick whatever the daemon state, so a bare TempDir (no daemon lock: daemon "not-running") still carries it.
        - web/src/lib/format.ts: daemonLine(daemon: DaemonState, tickAgeSecs: number | null, staleSecs: number | null, busy: Busy | null, nowMs: number) -> { text: string; detail?: string; tone: Tone }, Busy = { op: "brief-generation" | "merge-verification" | "provisioning"; what: string; stage_id: string; started_at: string; ceiling_secs: number; global_ceiling_secs: number } exported from web/src/api/schema.ts. With daemon "running", staleSecs null and busy set, elapsed = round((nowMs - Date.parse(started_at)) / 1000): elapsed under ceiling_secs and tickAgeSecs under 60 gives text "daemon running", detail containing "busy " + what, tone "completed"; elapsed at or past ceiling_secs and under global_ceiling_secs gives the same text and detail with tone "warning"; elapsed at or past global_ceiling_secs gives text "loop stalled " + formatElapsed(elapsed), tone "warning"; tickAgeSecs 60 or more gives today's text "loop stalled " + formatElapsed(tickAgeSecs), tone "warning".
        WAVES: (1) W1 alone (foundation; W2 compiles against it). (2) W2 and W3 in ONE message (disjoint files). After both return, compare W2's pinned WebBusy JSON with the shape W3 parses and fix any difference through a fresh W3 spawn. Territories are DISJOINT. Workers NEVER spawn subagents. Spawn every worker BY AGENT TYPE (sonnet rows loom-software-engineer), each with a short fixed prompt plus "Your brief: <path>. Read it in full before anything else." Wait for background workers with one loom subagents watch.

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | W1 | Busy module, tick format, provisioning on the shared helper (foundation) | sonnet | loom/src/orchestrator/busy.rs; loom/src/orchestrator/busy_tests.rs; loom/src/orchestrator/mod.rs; loom/src/orchestrator/tick.rs; loom/src/orchestrator/core/provision_gate.rs; loom/src/orchestrator/core/provision_gate_tests.rs; loom/src/orchestrator/core/provision_gate_pin_tests.rs | common.md | doc/plans/briefs/scheduler-busy-status/busy-tick/w1-foundation.md |
        | W2 | Wrapped call sites, alerts, status headline, TUI, web payload | sonnet | loom/src/orchestrator/core/stage_spawn.rs; loom/src/orchestrator/core/stage_knowledge.rs; loom/src/orchestrator/core/inbox_drain/merge_resolved.rs; loom/src/orchestrator/core/merge_handler/resolver_exit.rs; loom/src/orchestrator/scheduling_report.rs; loom/src/orchestrator/scheduling_report_loop.rs; loom/src/commands/status.rs; loom/src/commands/status/daemon_line.rs; loom/src/commands/status/ui/tui/app.rs; loom/src/commands/status/ui/tui/app_tests.rs; loom/src/commands/status/ui/tui/ledger/mod.rs; loom/src/commands/status/ui/tui/ledger/header.rs; loom/src/commands/status/ui/tui/ledger/header_daemon.rs; loom/src/commands/status/ui/tui/ledger/layout.rs; loom/src/commands/status/ui/tui/ledger/tests.rs; loom/src/commands/status/ui/tui/ledger/tests_viewport.rs; loom/src/commands/status/ui/tui/ledger/tests_tokens.rs; loom/src/commands/status/ui/tui/ledger/tests_models_contracts.rs; loom/src/commands/status/ui/tui/ledger/tests_tokens_contracts.rs; loom/src/commands/status/ui/tui/ledger/quota_tests.rs; loom/src/commands/status/web/model.rs; loom/src/commands/status/web/model_tests.rs; loom/src/commands/status/web/feed_split.rs; loom/src/commands/status/web/feed_split_tests.rs; web/src/api/fixtures/snapshot.json | common.md | doc/plans/briefs/scheduler-busy-status/busy-tick/w2-rust-surfaces.md |
        | W3 | Web daemon line and the BusyOp schema parity test | sonnet | web/src/api/busy-schema.ts; web/src/api/schema.ts; web/src/lib/format.ts; web/src/lib/format.test.ts; web/src/components/header-lines.tsx; loom/src/commands/status/web/schema_parity.rs | common.md | doc/plans/briefs/scheduler-busy-status/busy-tick/w3-web.md |

        PROVISIONING (plan Decision 5): provision_worktree wraps EACH provision entry in its own while_busy(.., BusyOp::Provisioning, stage_id, ..), in order, stopping at the first failure, because PROVISION_TIMEOUT bounds one command; W1 adds provision_gate_tests.rs tests that copy the tick from inside a running entry and prove the busy line during the run and a later started_at for the second of two entries (w1 brief, tasks 3 and 4).
        A compile error in a file no row owns goes to the row whose change caused it; record the file in loom memory. If W2 must split stage_spawn.rs to stay at or under 400 lines, the new sibling file and loom/src/orchestrator/core/mod.rs become W2's (w2 brief, task 1). W2 owns web/src/api/fixtures/snapshot.json because model_tests.rs fixture_matches_serde_output compares the Rust fixture with it; W3's schema default keeps every web parse valid either way.
        LEDGER: loom/maintainability-baseline.txt is a ratchet file; never edit it. Its values are exact: execute_static in loom/src/commands/status.rs keeps exactly 81 lines, neither more nor fewer. Every file a worker adds or grows stays at or under 400 lines and every function at or under 50; the tight files are named in common.md "Size limits".
        INTEGRITY: the integrity check counts assertions only in test files (Rust *_tests.rs, tests.rs, tests_*.rs, tests/**; TypeScript *.test.ts(x)); the inline test modules of status.rs and scheduling_report.rs are not counted, and a verbatim move between files raises nothing. Expected events: the expect(daemonLine(...)) line of the table test in web/src/lib/format.test.ts gains the busy and nowMs arguments (w3 task 5), and any provision_gate_tests.rs assertion W1 rewrites (W1 adds a new test instead where it can). File them in ONE loom stage dispute-integrity busy-tick with exactly the ids loom stage review integrity busy-tick lists, reason "daemonLine gained the busy and nowMs arguments and the tick refresh moved to orchestrator::busy::while_busy (plan Decisions 3 and 6)". Paste this paragraph into every reviewer prompt.
        AFTER THE WAVES: cargo fmt --manifest-path loom/Cargo.toml --all; (cd web && node_modules/.bin/oxfmt); wc -l every file the stage added or grew and split any above 400; one loom check busy-tick; the review rounds (one reviewer with the loom-concurrency skill on while_busy: the first stamp is written on the calling thread before f, the helper is joined before the clearing write on return and on unwind, no busy line survives a panic, no ticker outlives its scope, started_at is stable across re-stamps, every tick write is a temp-file rename; the same round covers correctness, the wording table, the loom status daemon gate, and the size limits). Do not rebuild web/dist here: integration-verify does (plan Decision 10).
        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); record a stale-knowledge note for doc/loom/knowledge/architecture/status-data-model.md wherever it says the tick is stamped only at phase entry or that 60 s of silence always means a stall; NEVER loom knowledge; NEVER Claude Code auto-memory.
      files:
        - "loom/src/orchestrator/busy.rs"
        - "loom/src/orchestrator/busy_tests.rs"
        - "loom/src/orchestrator/mod.rs"
        - "loom/src/orchestrator/tick.rs"
        - "loom/src/orchestrator/core/provision_gate.rs"
        - "loom/src/orchestrator/core/provision_gate_tests.rs"
        - "loom/src/orchestrator/core/provision_gate_pin_tests.rs"
        - "loom/src/orchestrator/core/stage_spawn.rs"
        - "loom/src/orchestrator/core/stage_spawn_signal.rs"
        - "loom/src/orchestrator/core/mod.rs"
        - "loom/src/orchestrator/core/stage_knowledge.rs"
        - "loom/src/orchestrator/core/inbox_drain/merge_resolved.rs"
        - "loom/src/orchestrator/core/merge_handler/resolver_exit.rs"
        - "loom/src/orchestrator/scheduling_report.rs"
        - "loom/src/orchestrator/scheduling_report_loop.rs"
        - "loom/src/commands/status.rs"
        - "loom/src/commands/status/daemon_line.rs"
        - "loom/src/commands/status/ui/tui/**"
        - "loom/src/commands/status/web/model.rs"
        - "loom/src/commands/status/web/model_tests.rs"
        - "loom/src/commands/status/web/feed_split.rs"
        - "loom/src/commands/status/web/feed_split_tests.rs"
        - "loom/src/commands/status/web/schema_parity.rs"
        - "loom/tests/scheduler_busy_contracts.rs"
        - "web/src/api/busy-schema.ts"
        - "web/src/api/schema.ts"
        - "web/src/api/fixtures/snapshot.json"
        - "web/src/lib/format.ts"
        - "web/src/lib/format.test.ts"
        - "web/src/lib/daemon-line-busy.test.ts"
        - "web/src/components/header-lines.tsx"
      before_stage:
        - command: "rg -qF 'fn run_with_fresh_tick(' loom/src/orchestrator/core/provision_gate.rs"
          description: "the tick refresh still lives only in provisioning; this plan moves it into orchestrator::busy"
      after_stage:
        - command: "! rg -qF 'fn run_with_fresh_tick(' loom/src/orchestrator/core/provision_gate.rs"
          description: "provisioning no longer carries its own tick refresh; it uses busy::while_busy"
        - command: "! rg -qF 'never trips it' loom/src/orchestrator/tick.rs"
          description: "the STALL_THRESHOLD_SECS comment no longer claims slow work never reaches 60 s"
      artifacts:
        - "loom/src/orchestrator/busy.rs"
        - "loom/src/orchestrator/scheduling_report_loop.rs"
        - "loom/src/commands/status/daemon_line.rs"
        - "loom/src/commands/status/ui/tui/ledger/header_daemon.rs"
        - "web/src/api/busy-schema.ts"
      acceptance:
        - command: "cargo test --manifest-path loom/Cargo.toml --test scheduler_busy_contracts"
          timeout_secs: 1200
        - command: "cargo test --manifest-path loom/Cargo.toml --lib orchestrator::busy"
          timeout_secs: 1200
        - command: "cargo test --manifest-path loom/Cargo.toml --lib orchestrator::tick"
          timeout_secs: 1200
        - command: "cargo test --manifest-path loom/Cargo.toml --lib orchestrator::scheduling_report"
          timeout_secs: 1200
        - command: "cargo test --manifest-path loom/Cargo.toml --lib orchestrator::core::provision_gate"
          timeout_secs: 1200
        - command: "cargo test --manifest-path loom/Cargo.toml --lib commands::status"
          timeout_secs: 1200
        - command: "cargo test --manifest-path loom/Cargo.toml --test maintainability"
          timeout_secs: 1200
        - command: "cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings"
          timeout_secs: 1800
        - "cargo fmt --manifest-path loom/Cargo.toml --check"
        - command: "RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path loom/Cargo.toml --workspace --all-features --no-deps"
          timeout_secs: 1800
        - "cd web && node_modules/.bin/tsc -b --noEmit"
        - "cd web && node_modules/.bin/oxlint --deny-warnings"
        - "cd web && node_modules/.bin/oxfmt --check"
        - command: "cd web && node_modules/.bin/vitest run src/lib src/api src/components src/state src/routes"
          timeout_secs: 1200
      wiring:
        - source: "loom/src/orchestrator/core/stage_spawn.rs"
          pattern: "BusyOp::BriefGeneration"
          literal: true
          description: "the overlay refresh and brief generation for implementation and contract sessions run inside while_busy (both calls are in spawn_stage_agent, wherever write_agent_signal is defined)"
        - source: "loom/src/orchestrator/core/stage_knowledge.rs"
          pattern: "BusyOp::BriefGeneration"
          literal: true
          description: "brief generation for knowledge sessions runs inside while_busy"
        - source: "loom/src/orchestrator/core/inbox_drain/merge_resolved.rs"
          pattern: "BusyOp::MergeVerification"
          literal: true
          description: "verification of a --resolved merge runs inside while_busy"
        - source: "loom/src/orchestrator/core/merge_handler/resolver_exit.rs"
          pattern: "BusyOp::MergeVerification"
          literal: true
          description: "verification at a resolver's exit runs inside while_busy"
        - source: "loom/src/orchestrator/core/provision_gate.rs"
          pattern: "BusyOp::Provisioning"
          literal: true
          description: "provisioning uses the shared helper, one while_busy per provision entry"
        - source: "loom/src/commands/status.rs"
          pattern: "daemon_line::loop_state_for("
          literal: true
          description: "execute_static takes the headline state from the daemon-gated classifier helper"
        - source: "loom/src/commands/status/daemon_line.rs"
          pattern: "busy::loop_state("
          literal: true
          description: "the loom status headline is decided by the shared classifier"
        - source: "loom/src/commands/status/ui/tui/ledger/header.rs"
          pattern: "view.loop_state"
          literal: true
          description: "the TUI header reads the per-frame loop state"
        - source: "loom/src/commands/status/ui/tui/ledger/header_daemon.rs"
          pattern: "LoopState::Busy"
          literal: true
          description: "the TUI header renders the busy state"
        - source: "loom/src/commands/status/web/model.rs"
          pattern: "WebBusy::from"
          literal: true
          description: "collect_snapshot fills the busy payload from the tick it reads"
        - source: "web/src/components/header-lines.tsx"
          pattern: "snapshot.busy"
          literal: true
          description: "the web daemon line receives the busy payload"
      contracts:
        - id: busy-line-clears-after-panic
          file: loom/tests/scheduler_busy_contracts.rs
          test: busy_line_clears_after_panic
          scenario: "calls loom::orchestrator::busy::while_busy(work_dir, Phase::Spawning, BusyOp::Provisioning, \"stage-a\", || panic!()) inside std::panic::catch_unwind on a TempDir work_dir, then reads the tick with loom::orchestrator::tick::read"
          rejects: "a while_busy that clears the busy line only on a normal return, or lets the helper thread write once more after the clear, so a panicking operation stays reported as busy"
        - id: busy-line-names-op-and-stage-while-running
          file: loom/tests/scheduler_busy_contracts.rs
          test: busy_line_names_op_and_stage_while_running
          scenario: "inside while_busy(work_dir, Phase::Spawning, BusyOp::BriefGeneration, \"stage-a\", closure) the closure reads the tick; after while_busy returns the test reads it again"
          rejects: "a while_busy that refreshes only the timestamp and phase without the busy line, writes the first stamp only from the helper thread so the closure can find no tick yet, or leaves the busy line in the tick after the operation returns"
        - id: busy-line-restamps-while-running
          file: loom/tests/scheduler_busy_contracts.rs
          test: busy_line_restamps_while_running
          scenario: "calls loom::orchestrator::busy::while_busy_every(work_dir, Phase::Spawning, BusyOp::Provisioning, \"stage-a\", Duration::from_millis(200), closure) on a TempDir; the closure reads the tick with tick::read, sleeps 1500 ms, and reads it again; the test asserts both reads carry a busy line, the second recorded_at is strictly later than the first, and both busy values are equal (same op, stage id and started_at); after while_busy_every returns the test sleeps 600 ms (three intervals), reads the tick again and asserts busy is None"
          rejects: "a while_busy that stamps the tick only once, so an operation past 60 s still reads as a stale tick and a stalled loop, that writes a new started_at on every re-stamp, or whose helper keeps writing busy lines after the operation returned"
        - id: malformed-busy-line-reads-as-not-busy
          file: loom/tests/scheduler_busy_contracts.rs
          test: malformed_busy_line_reads_as_not_busy
          scenario: "writes orchestrator.tick with a fresh timestamp, phase spawning, and in turn each third line: 'busy bogus-op <now> stage-a', 'busy brief-generation not-a-time stage-a', 'busy brief-generation <now> ' (empty stage id), 'busy brief-generation <now> stage\\u{1b}[2Ja' (an escape sequence), 'busy brief-generation <now> ' followed by 129 'a' characters (over 128 bytes), and 'work brief-generation <now> stage-a' (wrong prefix), then calls tick::read for each and asserts Some(tick) with the written phase and busy None"
          rejects: "a parser that returns Ok(None) or Err for the whole tick on a bad third line, so the dashboards show 'tick unknown', or that accepts a stage id holding control characters and later prints it to the terminal"
        - id: busy-under-ceiling-is-not-stalled
          file: loom/tests/scheduler_busy_contracts.rs
          test: busy_under_ceiling_is_not_stalled
          scenario: "writes orchestrator.tick with a timestamp 2 s old, phase spawning, and 'busy brief-generation <now minus 75 s> stage-a', then calls scheduling_report::alerts(work_dir, true) and asserts one Info alert naming 'orchestrator busy' and 'generating the brief for stage-a', no Critical, and no text containing 'stalled' or 'loom stop'"
          rejects: "an alert rule that measures staleness from the operation's start or keeps the 60 s rule for busy operations, reporting 'orchestrator loop stalled' while a brief is legitimately being generated, or that puts the restart advice on a non-Critical alert"
        - id: per-operation-ceilings-differ
          file: loom/tests/scheduler_busy_contracts.rs
          test: per_operation_ceilings_differ
          scenario: "with a fresh tick, writes 'busy brief-generation <now minus 400 s> stage-a' and calls alerts(work_dir, true), asserting a Warning containing 'longer than the expected' and no text containing 'loom stop'; then rewrites the third line as 'busy provisioning <now minus 400 s> stage-a' and calls alerts again, asserting an Info alert and no Warning"
          rejects: "one shared ceiling for every operation, which either warns about a 400 s provisioning run or stays silent about a 400 s brief generation"
        - id: global-ceiling-is-critical
          file: loom/tests/scheduler_busy_contracts.rs
          test: global_ceiling_is_critical
          scenario: "writes orchestrator.tick with a timestamp 2 s old and 'busy provisioning <now minus 1900 s> stage-a', then calls alerts(work_dir, true)"
          rejects: "an implementation in which a tick kept fresh by the helper thread suppresses the Critical alert forever, so a hung wrapped operation is never reported as a stall"
        - id: stale-tick-is-critical-while-busy
          file: loom/tests/scheduler_busy_contracts.rs
          test: stale_tick_is_critical_while_busy
          scenario: "writes orchestrator.tick with a timestamp 90 s old, phase spawning, and 'busy provisioning <now minus 100 s> stage-a', then calls alerts(work_dir, true)"
          rejects: "a rule that trusts the busy line over the tick's age, so a frozen helper thread or process reads as busy instead of stalled"
        - id: web-snapshot-carries-the-busy-line
          file: loom/tests/scheduler_busy_contracts.rs
          test: web_snapshot_carries_the_busy_line
          scenario: "in a TempDir work_dir (no daemon lock, so the daemon reads not-running), writes orchestrator.tick with a fresh timestamp, phase spawning and 'busy provisioning <now minus 30 s> stage-a', calls loom::commands::status::web::model::collect_snapshot(work_dir, StatusData::default(), SnapshotSource::Files, false) and serde_json::to_value; asserts busy.op == \"provisioning\", busy.what == \"provisioning stage-a\", busy.stage_id == \"stage-a\", busy.ceiling_secs == 600, busy.global_ceiling_secs == 1800 and busy.started_at parses as RFC 3339 to the written instant; then rewrites the tick with only the first two lines, collects again and asserts busy is JSON null"
          rejects: "a WebSnapshot whose busy is never filled from the tick, so the web dashboard never shows the operation, or that keeps a busy object after the busy line is gone"
        - id: web-busy-under-ceiling
          file: web/src/lib/daemon-line-busy.test.ts
          test: "daemonLine busy names the operation under its ceiling"
          scenario: "calls daemonLine(\"running\", 2, null, { op: \"brief-generation\", what: \"generating the brief for stage-a\", stage_id: \"stage-a\", started_at: <nowMs minus 75 s as ISO>, ceiling_secs: 300, global_ceiling_secs: 1800 }, nowMs)"
          rejects: "a daemonLine that ignores busy and renders 'daemon running · tick 2s ago', or calls the loop stalled because the operation has run past 60 s"
        - id: web-busy-over-ceiling
          file: web/src/lib/daemon-line-busy.test.ts
          test: "daemonLine busy warns past the operation ceiling"
          scenario: "calls daemonLine(\"running\", 2, null, busy with ceiling_secs 300 and started_at nowMs minus 400 s, nowMs)"
          rejects: "a daemonLine that keeps the normal 'completed' tone after the operation's own ceiling"
        - id: web-busy-past-global-ceiling
          file: web/src/lib/daemon-line-busy.test.ts
          test: "daemonLine busy reports a stall past the global ceiling"
          scenario: "calls daemonLine(\"running\", 2, null, busy with op \"provisioning\", ceiling_secs 600, global_ceiling_secs 1800 and started_at nowMs minus 1900 s, nowMs) and asserts text \"loop stalled \" + formatElapsed(1900) and tone \"warning\""
          rejects: "a daemonLine that decides a stall only from tickAgeSecs, which the helper thread keeps under 60 s"

    - id: integration-verify
      name: "Integration Verification"
      summary: "Confirms the busy reporting reaches loom status, the TUI and the web dashboard on the merged tree, and that the full Rust and web suites pass."
      stage_type: integration-verify
      skills: ["loom-rust", "loom-concurrency", "loom-typescript"]
      working_dir: "."
      dependencies: ["busy-tick"]
      description: |
        Final verification of doc/plans/PLAN-scheduler-busy-status.md. Verify FUNCTIONAL INTEGRATION, not just tests passing. NEVER Claude Code auto-memory.
        Use parallel subagents and skills to maximize performance.
        CONTEXT: read the plan, loom memory show --all, doc/loom/knowledge/INDEX.md and only the sections on the status data model and the orchestrator daemon loop.
        ORDER: build first (cd web && bun run build, then cargo build --manifest-path loom/Cargo.toml), so the dist freshness criteria inspect the new bundle; a first loom check integration-verify is diagnostic; the code review; fixes, each followed by a dist rebuild when it touched web/src; the final full loom check against those exact bytes; the final review round; the commits. The review gate refuses completion when the tree changed after the final round (only *.md prose is exempt), so a fix from that round needs another rebuild, check and round.
        BUILD AND TEST (zero tolerance; nothing is pre-existing): every acceptance command below, through loom check integration-verify; a fix round re-checks the failed criteria with --criterion <n>.
        WEB DIST (plan Decision 10): web/dist is committed and embedded in the binary, and the snapshot schema is strict, so an old bundle rejects every frame that carries busy. After the final round commit the code fixes, then web/dist last as chore(web).
        CODE REVIEW: parallel loom-code-reviewer subagents: (1) concurrency with the loom-concurrency skill on busy::while_busy and its three call sites: the helper is joined before the clearing write on return and on unwind; a wrapped closure that borrows the Orchestrator mutably cannot deadlock or alias; no tick write can leave a busy line after the operation. (2) correctness of the classifier and surfaces: every row of the wording table in doc/plans/briefs/scheduler-busy-status/busy-tick/common.md holds in scheduling_report::alerts, the loom status headline, the TUI header and web daemonLine, and the restart advice appears only on Critical. (3) completeness: every scheduler-thread call that builds a knowledge pack (rg retrieve_stage_pack across loom/src/orchestrator; only CLI paths such as recovery signals may stay unwrapped), the refresh_stage_overlay call before it, and every verify_resolved_completion call made by the daemon runs inside while_busy; every tick write is a temp-file rename; the web feed classifies busy as a full-only key and schema_parity.rs lists every BusyOp. Fix or dispute every finding; never defer one.
        SUGGESTIONS: weigh every pending reviewer suggestion the signal lists; resolve each one implemented with loom memory resolve <id> --outcome implemented --reason <what changed>.
        FUNCTIONAL (one Bash invocation): run cargo test --manifest-path loom/Cargo.toml --test scheduler_busy_contracts and cargo test --manifest-path loom/Cargo.toml --lib orchestrator::busy; the named tests busy_line_names_op_and_stage_while_running, busy_line_restamps_while_running and busy_line_clears_after_panic passing is the proof that a wrapped operation leaves a three-line tick while it runs, keeps re-stamping it, and leaves a two-line tick after (they assert; they print nothing); record the result in loom memory. Then read loom/src/orchestrator/core/stage_spawn.rs, stage_knowledge.rs, inbox_drain/merge_resolved.rs and merge_handler/resolver_exit.rs and confirm each wrapped call is the one on the scheduler path, and read loom/src/commands/status/daemon_line.rs and confirm the busy and stalled headline arms apply only to a Running or Unreachable daemon.
        Record discoveries to loom memory for knowledge-distill, including any knowledge file contradicted by the tree: loom memory note "stale-knowledge: ...".
      acceptance:
        - command: "cargo test --manifest-path loom/Cargo.toml --all-targets --no-fail-fast"
          timeout_secs: 3600
        - command: "cargo test --manifest-path loom/Cargo.toml --doc --workspace --all-features"
          timeout_secs: 1800
        - command: "cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings"
          timeout_secs: 1800
        - "cargo fmt --manifest-path loom/Cargo.toml --check"
        - command: "cargo build --manifest-path loom/Cargo.toml"
          timeout_secs: 1800
        - command: "RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path loom/Cargo.toml --workspace --all-features --no-deps"
          timeout_secs: 1800
        - "cd web && node_modules/.bin/tsc -b --noEmit"
        - "cd web && node_modules/.bin/oxlint --deny-warnings"
        - "cd web && node_modules/.bin/oxfmt --check"
        - command: "cd web && node_modules/.bin/vitest run"
          timeout_secs: 1200
        - "cd web && node scripts/bundle-check.mjs"
        - command: "cd web && d=$(mktemp -d \"${TMPDIR:-/tmp}/dist.XXXXXX\") && node_modules/.bin/vite build --outDir \"$d\" --emptyOutDir --logLevel error && diff -r \"$d\" dist"
          timeout_secs: 600
      files:
        - "web/dist/**"
      wiring:
        - source: "loom/src/orchestrator/scheduling_report.rs"
          pattern: "loop_alert("
          literal: true
          description: "alerts() decides the loop alert through the busy-aware classifier"

    - id: knowledge-distill
      name: "Knowledge Distillation"
      summary: "Records the busy tick line, the per-operation and global ceilings and the shared loop classifier in the knowledge base, with the mistake that a 60-second stall rule assumed slow work never took that long."
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      description: |
        Curate all stage memories into permanent knowledge; update user docs. NEVER Claude Code auto-memory.
        SINGLE-AGENT: do NOT spawn subagents; memories are compact summaries; keep code spot-reads narrow.
        START with loom memory pending --group (corrections, mistakes, decisions, other); read the plan and the knowledge sections it touches.
        CORRECTIONS FIRST: apply every stale-knowledge memory in place with loom knowledge replace-section <file> "<heading>" "<body>", never loom knowledge update; read each command's output to confirm the heading matched.
        Then curate, current truth only outside the mistakes files: the tick's busy line, busy::while_busy, the ceilings and busy::loop_state in doc/loom/knowledge/architecture/status-data-model.md (the section describing orchestrator.tick and the stall alert) and in patterns/orchestrator-daemon-loop.md where it describes what blocks a tick. Record as a mistake with prevention: the 60 s stall rule assumed slow scheduler-thread work never reached 60 s, so brief generation and merge verification were reported as a stalled loop with restart advice; prevention: wrap any scheduler-thread operation that can exceed a few seconds in busy::while_busy with a ceiling. TIER ROUTING: findings ~40 lines or fewer go inline in the tier-1 file; larger ones via loom knowledge update <category>/<slug> with a 2-4 line tier-1 summary + link and loom knowledge annotate --blurb for a new topic. Prefer folding into an existing topic.
        DOCS: if README.md describes the status line or the stall alert, update that paragraph; otherwise skip and record why in memory.
        SUGGESTIONS: record every unimplemented reviewer suggestion in concerns or its topic, then resolve it promoted, merged or discarded.
        RECEIPTS: every memory taken into knowledge gets loom memory resolve <id> --outcome promoted|merged|discarded|deferred right after the write that used it; finish with loom memory pending --strict.
        LAST, if this stage removed structural issues: loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
        - "loom memory pending --strict"
      files: ["doc/loom/knowledge/**", "README.md"]
```

<!-- END loom METADATA -->

## Pressure tests

- 2026-11-07: Claude opus, Codex gpt-6-astra; 1 round, review addressed
