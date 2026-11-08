# Busy tick: shared context for every worker

This file is authoritative for the internal signatures of stage `busy-tick` of
`doc/plans/PLAN-scheduler-busy-status.md`. The stage description is authoritative for the
public surface the frozen contracts call; where the two differ, the stage description wins.

## The problem

`loom/src/orchestrator/tick.rs` stamps `.loom/work/orchestrator.tick` only when the scheduler
loop enters a phase (`loom/src/orchestrator/core/run.rs`, `run_tick`, `poll_run_events`,
`wait_for_next_tick`). `loom/src/orchestrator/scheduling_report.rs` (`stalled_alert`) raises a
Critical "orchestrator loop stalled ... restart with `loom stop`, then `loom run`" once the tick
is 60 s old (`tick::STALL_THRESHOLD_SECS`). Long but legitimate work on the scheduler thread
trips it:

- Brief generation at stage spawn: `write_agent_signal` in
  `loom/src/orchestrator/core/stage_spawn.rs` and `generate_knowledge_signal` in
  `loom/src/orchestrator/core/stage_knowledge.rs` build the stage's knowledge pack
  (`signals/retrieval.rs`, `retrieve_stage_pack`). Observed at 71 s on 2026-11-07.
- Resolved-merge verification: `verify_resolved_completion` in
  `loom/src/orchestrator/core/inbox_drain/merge_resolved.rs` (Sync phase) and
  `loom/src/orchestrator/core/merge_handler/resolver_exit.rs` (Events phase) runs the stage's
  whole acceptance suite. Observed at 187 s.
- Provisioning already re-stamps the tick from a helper thread
  (`loom/src/orchestrator/core/provision_gate.rs`, `run_with_fresh_tick`, `keep_tick_fresh`).

The work stays on the scheduler thread. The plan generalizes the provisioning helper, records
which operation is running, and reports "busy" until a ceiling says otherwise.

## The tick file format

Line 1 is the RFC 3339 timestamp, line 2 the phase (`sync`, `spawning`, `events`, `idle`), as
today. A wrapped operation adds line 3:

```text
2026-11-07T13:51:08.120Z
spawning
busy brief-generation 2026-11-07T13:51:08.120Z criteria-authoring
```

Line 3 is `busy <op> <started_at> <stage_id>`, single spaces. `<op>` is one of
`brief-generation`, `merge-verification`, `provisioning`. `<started_at>` is RFC 3339 and stays the
same for every re-stamp of one operation. `<stage_id>` is the rest of the line. It must be
non-empty and at most 128 bytes, and may hold only ASCII letters, digits, `-`, `_` and `.`.
A third line that fails any rule (unknown op, bad timestamp, empty or bad stage id, wrong
prefix) reads as "not busy": `Tick.busy` is `None` and the tick itself still reads. It is never
an error, and never makes `tick::read` return `Ok(None)`.

`tick::record(work_dir, phase)` keeps writing two lines, so every phase entry clears a busy line.

Both `record` and `record_busy` write the content to `orchestrator.tick.tmp` in the same
directory and rename it over `orchestrator.tick`. Today `record` is a plain `std::fs::write`
(truncate, then write), and a reader that lands between the two sees an empty file
(`read` gives `Ok(None)`, "tick unknown") or a busy line cut inside the stage id, which still
passes validation. With the helper rewriting the tick every 10 s for minutes, that window is
no longer rare. Writes stay best-effort (errors swallowed). One temp name is enough: the
helper's writes and the clearing write are ordered by the join, and phase entries never run
while a helper exists.

## Types and functions (W1 creates them, W2 and W3 consume them)

In `loom/src/orchestrator/busy.rs` (new, `pub mod busy;` in `loom/src/orchestrator/mod.rs`):

```rust
pub const BRIEF_GENERATION_CEILING_SECS: i64 = 300;
pub const MERGE_VERIFICATION_CEILING_SECS: i64 = 1_200;
pub const PROVISIONING_CEILING_SECS: i64 = 600;
/// Any wrapped operation running this long is reported as a stalled loop.
pub const GLOBAL_BUSY_CEILING_SECS: i64 = 1_800;
/// How often the helper thread re-stamps the tick; well under STALL_THRESHOLD_SECS.
pub const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyOp { BriefGeneration, MergeVerification, Provisioning }

impl BusyOp {
    pub fn as_str(self) -> &'static str;         // "brief-generation" | "merge-verification" | "provisioning"
    pub fn parse(s: &str) -> Option<Self>;
    pub fn ceiling_secs(self) -> i64;            // the three constants above
    /// "generating the brief for <stage>" | "verifying the merge of <stage>" | "provisioning <stage>"
    pub fn describe(self, stage_id: &str) -> String;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Busy { pub op: BusyOp, pub stage_id: String, pub started_at: DateTime<Utc> }

impl Busy {
    pub fn elapsed_secs(&self, now: DateTime<Utc>) -> i64;   // saturates at 0
}

/// Run `f` on the calling thread. Before `f` starts, the calling thread itself writes the
/// three-line tick (`phase` plus the busy line, `started_at` = now); then a helper thread
/// re-stamps it every REFRESH_INTERVAL with the same `started_at`. When `f` returns or
/// unwinds, the helper is stopped and joined, and only then is a plain two-line
/// `tick::record(work_dir, phase)` written.
pub fn while_busy<T>(work_dir: &Path, phase: Phase, op: BusyOp, stage_id: &str, f: impl FnOnce() -> T) -> T;

/// `while_busy` with a caller-chosen re-stamp interval; `while_busy` calls it with
/// REFRESH_INTERVAL. Public only so the frozen contract can prove the re-stamp in 1.5 s.
#[doc(hidden)]
pub fn while_busy_every<T>(work_dir: &Path, phase: Phase, op: BusyOp, stage_id: &str, interval: Duration, f: impl FnOnce() -> T) -> T;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopState {
    /// No tick file, or an unreadable one.
    Unknown,
    /// The tick is fresh and nothing is wrapped.
    Ticking { age_secs: i64 },
    /// A wrapped operation runs and the tick is fresh.
    Busy { busy: Busy, elapsed_secs: i64, over_ceiling: bool },
    /// The tick is STALL_THRESHOLD_SECS or older (busy or not), or a wrapped operation has
    /// run GLOBAL_BUSY_CEILING_SECS or longer.
    Stalled { age_secs: i64, busy: Option<Busy>, elapsed_secs: Option<i64> },
}

pub fn loop_state(tick: Option<&Tick>, now: DateTime<Utc>) -> LoopState;
```

Rule order in `loop_state`, first match wins:

1. no tick: `Unknown`;
2. tick age at or past `STALL_THRESHOLD_SECS` (60): `Stalled`, carrying the busy line if any;
3. busy and elapsed at or past `GLOBAL_BUSY_CEILING_SECS`: `Stalled`;
4. busy: `Busy`, `over_ceiling` when elapsed is at or past `op.ceiling_secs()`;
5. otherwise `Ticking`.

In `loom/src/orchestrator/tick.rs`: `Tick` gains `pub busy: Option<Busy>`;
`pub fn record_busy(work_dir: &Path, phase: Phase, busy: &Busy)` writes the three-line form
(used only by `while_busy`); `read` parses line 3 per the rules above. Keep `record`
best-effort (errors swallowed), as today.

Callers never nest `while_busy`. The `<started_at>` of one operation never changes across
re-stamps. `Tick::is_stalled` stays in `tick.rs` and is rule 2 above.

Why the first stamp is synchronous: today `keep_tick_fresh` in `provision_gate.rs` writes its
first stamp on the helper thread while the caller goes straight on to the work, so a reader
inside the operation can find no tick at all. The frozen contract
`busy_line_names_op_and_stage_while_running` reads the tick from inside the closure.

The helper loop parks for `interval.min(1 s)` between checks of its stop flag, so a short test
interval re-stamps at that interval. If the helper thread cannot be spawned
(`std::thread::Builder::spawn_scoped` returns `Err`), `f` runs without a helper after the
synchronous stamp; `while_busy` never panics the scheduler because of a thread spawn.

## Wording (W2 and W3 render it; the contracts match substrings)

`<elapsed>` and `<ceiling>` use `crate::utils::format_elapsed` (Rust) and `formatElapsed`
(web). `<what>` is `op.describe(stage_id)`.

| State | Alert (severity, text) | `loom status` headline | TUI and web daemon line |
| --- | --- | --- | --- |
| Ticking | none | unchanged | unchanged (`daemon running · tick Ns ago`) |
| Busy, elapsed under 60 s | none | `daemon running, busy <what> (<elapsed>)` | `daemon running · busy <what> <elapsed>`, normal tone |
| Busy, elapsed 60 s or more, under ceiling | Info: `orchestrator busy: <what> for <elapsed>` | same as above | same as above |
| Busy, over ceiling | Warning: `orchestrator busy: <what> for <elapsed>, longer than the expected <ceiling>` | same, warning tone | same, warning tone |
| Stalled, stale tick | Critical: today's text, plus ` while <what>` after the phase when busy | today's stalled line | today's `loop stalled <age>` line, `<age>` = the TICK age (TUI raw `{age}s`, web `formatElapsed(tickAgeSecs)`), never the operation's elapsed time |
| Stalled, global ceiling | Critical: `orchestrator loop stalled: <what> has run <elapsed>, past the <global> limit — restart with `loom stop`, then `loom run`` | today's stalled line | `loop stalled <elapsed>`, `<elapsed>` = the operation's elapsed time |

Only Critical alerts keep the restart advice; no Info or Warning text contains `loom stop`. The
word `stalled` appears only in Stalled rows.

Today's strings: TUI `● daemon running` plus dimmed ` · tick {age}s ago` / ` · tick unknown`,
stale `● loop stalled {age}s` (`ledger/header.rs`, `daemon_spans`); web `daemon running` with
detail `tick {n}s ago`, stale `loop stalled <formatElapsed>` (`web/src/lib/format.ts`,
`daemonLine`); headline `daemon running, orchestrator loop stalled` and its Unreachable form
(`loom/src/commands/status.rs`, `daemon_status_line`).

Rules every surface follows:

- **The `loom status` headline.** The Busy and Stalled arms apply only when the daemon is
  `Running` or `Unreachable`, as the stalled override does today; `ProcessOnly`, `NotRunning`
  and the rest keep their arms whatever the tick says (a daemon killed mid-operation leaves a
  three-line tick). `Unreachable` keeps its sandbox wording in the hint. The stalled decision
  comes from `LoopState::Stalled`, never from "any Critical alert" (today's
  `alerts.iter().any(|a| a.severity == Severity::Critical)` also counts unrelated Critical
  alerts). Tones: under the ceiling, marker `StatusTone::Ok` and message `StatusTone::Ok`;
  over it, both `StatusTone::Warning`. `DaemonStatusLine.message` becomes `String`, since the
  busy text is formatted.
- **The TUI header.** Busy text is `● daemon running · busy <what> <elapsed>`, marker styled
  `Theme::status_completed()` and the rest `Theme::dimmed()` under the ceiling, the whole span
  `Theme::status_warning()` over it. When the right-hand span does not fit the header width,
  fall back to `● busy <op> <elapsed>` (`op.as_str()`): the elapsed time is never the part
  that is cut. The TUI only runs against a live daemon and exits on disconnect, so it needs no
  daemon-state gate.
- **The web daemon line.** Arm order in `daemonLine`: `staleSecs` set; `process-only`;
  `not-running`; tick age 60 s or more (today's stale arm, tick age); busy with elapsed at or
  past `global_ceiling_secs` (`loop stalled <elapsed>`); `unreachable` (today's arm, kept ahead
  of the busy detail so a sandboxed viewer still sees "socket unreachable"); busy (`daemon
  running`, detail `busy <what> <elapsed>`, tone by ceiling); then today's tick arm. A busy
  payload on a `not-running` or `process-only` daemon therefore never shows. Elapsed is
  computed from the browser clock against the daemon's `started_at`; both run on one machine.

## Web payload (W2 emits it, W3 parses it)

`WebSnapshot` (`loom/src/commands/status/web/model.rs`) gains a top-level `busy` field,
serialized as `null` when no operation is wrapped, else:

```json
{
  "op": "merge-verification",
  "what": "verifying the merge of stage-a",
  "stage_id": "stage-a",
  "started_at": "2026-11-07T13:51:08.120Z",
  "ceiling_secs": 1200,
  "global_ceiling_secs": 1800
}
```

It is filled from the tick's busy line whatever the loop state and whatever the daemon state
(a Stalled state with a busy line still sends it; the web line's arm order hides it for a
stopped daemon). Rust builds it with `impl From<&Busy> for WebBusy` (`WebBusy::from(&busy)` in
`collect_snapshot`, from the same `tick::read` that gives `tick_age_secs`); `what` is
`op.describe(stage_id)`, `ceiling_secs` is `op.ceiling_secs()`, `global_ceiling_secs` is
`GLOBAL_BUSY_CEILING_SECS`. The frozen contract `web_snapshot_carries_the_busy_line` calls
`collect_snapshot` on a TempDir and checks these values.

Feed classification: its values do not change while one operation runs, so it is not a clock
key in `loom/src/commands/status/web/feed_split.rs` (`CLOCK_TOP_KEYS` stays as is). It is a
full-only key: add `"busy"` to the `#[cfg(test)]` const `FULL_ONLY_KEYS` (`[&str; 2]` becomes
`[&str; 3]`). The existing test `every_snapshot_key_is_classified`
(`feed_split_tests.rs`) fails for any top-level key missing from those lists. Unclassified keys
already land in `Split.full_only`, and `needs_full` compares `full_only`, so a change from
`null` to an object or back sends one `full` frame, which the client applies wholesale
(`web/src/state/merge.ts` needs no change). Do not make it a state key: that route needs the
strict `stateFrame` schema in `web/src/api/frames.ts` and the client merge, which nobody owns.

Wire-enum parity (W3, which creates the TypeScript file this test reads, so no parallel worker
compiles against a file that does not exist yet): `loom/src/commands/status/web/schema_parity.rs`
checks that every Rust wire value appears in `web/src/api/schema.ts`. Add `all_busy_ops() -> Vec<BusyOp>` (an exhaustive
`match` without a wildcard, like the other `all_*` lists) and a test that `SCHEMA_TS` contains
`"<op.as_str()>"` for each op. `BusyOp` is not `Serialize`, so do not reuse
`assert_all_in_schema`; write the loop directly. Because the zod enum lives in
`web/src/api/busy-schema.ts`, the test reads that file too (`include_str!` it beside
`schema.ts`).

The web client computes elapsed time from `started_at` and its own clock. The web schema is
`export const busySchema = z.object({...})` and `export type Busy = z.infer<typeof busySchema>`
in `web/src/api/busy-schema.ts`, re-exported from `schema.ts`, with
`busy: busySchema.nullable().default(null)` in `snapshotSchema` (zod 4.5.4: input optional,
output `Busy | null`).

## Size limits

Every file a worker adds or grows stays at or under 400 lines and every function at or under 50
(`loom/tests/maintainability.rs`). `loom/maintainability-baseline.txt` is a ratchet file; never
edit it. Its values are exact: a function with an entry (`execute_static` in
`loom/src/commands/status.rs`, 81) must keep exactly that length. The tight files and where
their new code goes:

| File | Lines today | New code goes to |
| --- | --- | --- |
| `loom/src/orchestrator/scheduling_report.rs` | 400 | `scheduling_report_loop.rs` (W2) |
| `loom/src/orchestrator/core/stage_spawn.rs` | 393 | `stage_spawn_signal.rs` if needed (W2) |
| `loom/src/commands/status.rs` | 397 | `loom/src/commands/status/daemon_line.rs` (W2) |
| `loom/src/commands/status/ui/tui/ledger/header.rs` | 329 | `ledger/header_daemon.rs` (W2) |
| `loom/src/commands/status/ui/tui/app.rs` | 384 | keep the change to about 12 lines |
| `web/src/api/schema.ts` | 392 | `web/src/api/busy-schema.ts` (W3) |

The 400/50 limits are enforced for Rust by `loom/tests/maintainability.rs` (which also scans
`loom/tests/`) and bind TypeScript by the project rules.
