# exits-and-guards / w1-idle-stop: a stage session that ends its turn with nothing pending is told to reach a recorded exit

Tier: codex sol (`gpt-6.2-sol`, effort xhigh)
Read first: `doc/plans/briefs/stage-exits-and-guards/common.md`.

## Finding

A stage session ended its turn with an uncommitted tree after a denied tool call and wrote a
message for an operator. Nobody reads a stage session's final message. The Stop hook
`loom-hooks/commit-guard.sh` is advisory, the stage stayed `Executing`, and the stall line for a
stage with `subagent_timeout_secs: 1500` is 4500 s, so the stage sat idle for 47 minutes. The
stage-exit doctrine already says "Never end a turn asking the operator to act mid-stage"; nothing
enforces it.

`loom-hooks/loom-relay-stop.sh` already blocks a stop at most once per stranded relay ticket,
with a JSON `decision: block`, never when `stop_hook_active` is true. This unit adds a second
reason to that hook: an unattended stop.

## Files

Own (write):
- `loom/src/commands/hook/idle_stop.rs` (new)
- `loom/src/commands/hook/idle_stop_tests.rs` (new)
- `loom/src/commands/hook/mod.rs`
- `loom/src/commands/hook/tests_cli_surface.rs`
- `loom/src/cli/types_ops.rs`
- `loom/src/cli/dispatch.rs`

The hook script that calls this subcommand, `loom-hooks/loom-relay-stop.sh`, belongs to worker
w1b-idle-stop-hook. The interface between you is fixed: `loom hook idle-stop` reads the Stop
payload on stdin and prints either nothing or one line holding the JSON object
`{"decision":"block","reason":"..."}`, and always exits 0.

Read only: `loom/src/commands/hook/target.rs` (`HookTarget`), `loom/src/commands/hook/reviewer_brief.rs`
(a hook subcommand that reads the stage from the environment and prints to stdout),
`loom/src/subagent_lifecycle/store.rs` (`LifecycleIndex::session_child_disposition`,
`ChildDisposition`), `loom/src/native_worker/index.rs` (`stage_disposition`),
`loom/src/host_verify/store.rs` (`load_current`, `JobRecord`, `JobState::is_terminal`),
`loom/src/orchestrator/core/event_handler/freeze_refusal.rs` (`writer_is_quiet`: the existing
"quiet session with no children" test), `loom/src/orchestrator/monitor/heartbeat.rs`
(`Heartbeat`), `loom-hooks/spawn-guard.sh` (how a hook calls a `loom hook` subcommand under a
bound and fails open), `loom-hooks/_bounded.sh`.
Frozen (never edit): `loom/src/commands/hook/idle_stop_contract_tests.rs`.

## Interface (pinned; the frozen contract calls it)

In `loom/src/commands/hook/idle_stop.rs`:

```rust
/// How many times one session is told to reach a recorded exit.
pub(crate) const MAX_IDLE_STOP_BLOCKS: u32 = 3;

/// What the hook knows when the main agent of a session stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StopFacts {
    /// The stage is `Executing` and names this session as its current one.
    pub owns_executing_stage: bool,
    /// The session runs the stage's work (a stage or knowledge session), and
    /// is no contract writer, adjudicator or merge resolver.
    pub stage_session: bool,
    /// `Some(true)`: no subagent or worker of this session is active.
    /// `Some(false)`: one is. `None`: the evidence could not be read.
    pub children_quiet: Option<bool>,
    /// A host job (check, completion, freeze) of the stage is not terminal.
    pub host_job_live: bool,
    /// The main agent ran a tool since this session was last blocked here.
    pub worked_since_last_block: bool,
    /// Blocks this session has already received from this check.
    pub blocks_so_far: u32,
}

/// The reason to block the stop with, or `None` to let the session stop.
pub(crate) fn decide(stage_id: &str, facts: &StopFacts) -> Option<String>

/// `loom hook idle-stop`: reads the Stop payload on stdin, prints one
/// `{"decision":"block","reason":...}` object or nothing. Always `Ok`.
pub fn emit_idle_stop() -> anyhow::Result<()>
```

The function is named `emit_idle_stop`, as `emit_reviewer_brief` is, so that no symbol shares
the module's name: the stage's reachability check looks `emit_idle_stop` up by name.

`decide` returns `Some` only when ALL hold: `owns_executing_stage`, `stage_session`,
`children_quiet == Some(true)`, `!host_job_live`, `worked_since_last_block`,
`blocks_so_far < MAX_IDLE_STOP_BLOCKS`. Every other combination returns `None`: an unreadable
fact lets the session stop.

Reason text (one paragraph, `{id}` is the stage id; keep every sentence):

```text
Stage {id} is still Executing and this session runs unattended: nobody reads a final message. End in a recorded state. (1) Finish: commit, then `loom stage complete {id}`. (2) A wrong check, contract, finding or integrity event: file the dispute that names it; a refused commit does not stop a filing. (3) Something only a person can give (a credential, a host install, a decision, a denied permission): `loom stage block {id} "<what is needed and why>"`, or AskUserQuestion when one answer unblocks you. If a background command or agent of yours is still running, wait for it; this notice does not repeat until you run another tool.
```

## Changes

### 1. `emit_idle_stop()` (the subcommand)

- Resolve the stage, the session and the work directory the way the other hook subcommands do
  (`HookTarget` in `target.rs`; the environment carries `LOOM_STAGE_ID`, `LOOM_SESSION_ID`,
  `LOOM_WORK_DIR`). No stage in the environment: print nothing, return `Ok`.
- Read the Stop payload from stdin with the bounded reader the sibling subcommands use. A payload
  with `stop_hook_active: true` prints nothing.
- Fill `StopFacts`:
  - `owns_executing_stage`: load the stage file; status `Executing` and `stage.session` equals
    this session.
  - `stage_session`: the session record's type is the stage-work type or the knowledge type
    (find the enum with `loom map --find-all SessionType`; the relay matrix in
    `loom/src/relay/matrix.rs` lists the kinds). A session record that cannot be read gives
    `false`.
  - `children_quiet`: `LifecycleIndex` loaded from the work directory, then
    `session_child_disposition(stage, session)`: `NoChildren` gives `Some(true)`, `Active` gives
    `Some(false)`, `Unknown(_)` gives `None`. A native worker of the stage that still runs
    (`stage_disposition` in `native_worker/index.rs`) also gives `Some(false)`.
  - `host_job_live`: `host_verify::store::load_current(work_dir, stage)` holds a record whose
    state is not terminal. A read error gives `true`.
  - `worked_since_last_block` and `blocks_so_far`: one marker file in the session's scratch
    directory (`LOOM_SCRATCH_DIR`), named `.idle-stop`, holding two lines: the block count and
    the heartbeat `timestamp` the last block answered. `worked_since_last_block` is true when no
    marker exists or the stage's current main-agent heartbeat (`subagent == false`, same session)
    carries a different `timestamp`. A marker that is a symlink, is larger than 256 bytes or does
    not parse counts as `blocks_so_far = MAX_IDLE_STOP_BLOCKS`.
- When `decide` returns a reason: rewrite the marker (count plus one, current heartbeat
  timestamp), then print the JSON object on one line. A marker that cannot be written prints
  nothing: a block that cannot be counted is not issued.
- Every internal error is swallowed: print nothing, return `Ok(())`. A Stop hook must never fail
  a session.

### 2. CLI surface

- `HookCommands::IdleStop` in `loom/src/cli/types_ops.rs`, no arguments, doc comment: "Decide
  whether a stage session's stop is an unattended idle stop (the Stop hook calls this)".
- One arm in `dispatch_hook` (`loom/src/cli/dispatch.rs`): `HookCommands::IdleStop =>
  hook::idle_stop::emit_idle_stop(),`. Edit `dispatch_hook` only; the function `dispatch` in the same
  file has an exact ledger row and must keep its length.
- `pub mod idle_stop;` in `loom/src/commands/hook/mod.rs`, with the test module declared the way
  `reviewer_brief_tests.rs` is.
- `tests_cli_surface.rs`: add `idle_stop_takes_no_arguments` beside `reviewer_brief_takes_no_arguments`.

## Tests (write them; the orchestrator runs them)

- `idle_stop_tests.rs`, on `decide`: one test per field showing that flipping it alone from the
  blocking combination returns `None` (six tests), and one showing the reason carries the stage
  id and the three exits (`loom stage complete`, `loom stage block`, "file the dispute").
- `idle_stop_tests.rs`, on the marker: a round trip; a symlinked marker counts as spent; a
  marker with the current heartbeat timestamp gives `worked_since_last_block == false`.
- Contracts to turn green: the two tests in `idle_stop_contract_tests.rs`.

## Proof

From `loom/`: `cargo check --all-targets`.

## Traps

- `commit-guard.sh` went advisory because a Stop hook that blocked on every stop fired while the
  lead waited for subagents and ended sessions before they committed. The three guards against a
  repeat are `children_quiet`, `host_job_live` and the per-tool-call marker. Do not relax one.
- `ChildDisposition::Unknown` must not block: a corrupt lifecycle journal would otherwise turn
  every stop into a block.
- The stage also reads `Executing` during its contract phase, with a contract writer as the
  session. `stage_session` is what keeps this check away from it; a contract writer has its own
  quiet-refusal route.
- Do not write a heartbeat from this subcommand. The monitor reads heartbeats as evidence of
  work.
- The subcommand runs inside a hook, outside the session's command sandbox, with the session's
  environment. It must not open the daemon socket.
