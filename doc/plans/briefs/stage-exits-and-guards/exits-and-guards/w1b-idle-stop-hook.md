# exits-and-guards / w1b-idle-stop-hook: the Stop hook asks loom whether a stop is an unattended idle stop

Tier: codex sol (`gpt-6.2-sol`, effort xhigh)
Read first: `doc/plans/briefs/stage-exits-and-guards/common.md`.

## Finding

A stage session ended its turn with an uncommitted tree and a message for an operator. Nobody
reads a stage session's final message, the Stop hook `loom-hooks/commit-guard.sh` is advisory,
and the stage sat idle for 47 minutes. Worker w1-idle-stop adds the subcommand
`loom hook idle-stop`, which decides whether such a stop is blocked. This unit calls it from the
one Stop hook that is allowed to block.

`loom-hooks/loom-relay-stop.sh` blocks a stop at most once per stranded relay ticket, with a
JSON `decision: block` on stdout, never when `stop_hook_active` is true, and always exits 0.

## Interface (pinned; another worker implements the other side)

`loom hook idle-stop` reads the Stop payload on stdin and prints either nothing or one line
holding the JSON object `{"decision":"block","reason":"..."}`. It always exits 0 and bounds its
own blocks (at most three per session, at most one per tool call).

## Files

Own (write):
- `loom-hooks/loom-relay-stop.sh`
- `loom-hooks/tests/loom-relay-stop.sh`

Read only: `loom-hooks/spawn-guard.sh` (how a hook finds the `loom` binary and calls a
`loom hook` subcommand under a bound, failing open), `loom-hooks/_bounded.sh`,
`loom-hooks/_relay_kinds.sh`.

## Changes

In `loom-hooks/loom-relay-stop.sh`:

- After the ticket loop, when `REASON` is empty and `LOOM_STAGE_ID` is set and a `loom` binary
  resolves the way `spawn-guard.sh` resolves it: pipe `INPUT_JSON` to `loom hook idle-stop`
  under the same bound `spawn-guard.sh` uses for its subcommand call, capture stdout, and print
  it only when `jq -e` confirms it is an object whose `decision` is `"block"` and whose `reason`
  is a non-empty string. Re-emit it with `jq -nc --arg reason ...` as the ticket branch does;
  never print the captured text raw.
- Anything else (no binary, a timeout, a non-zero exit, other output) prints nothing.
- A ticket reason wins: when the ticket loop found one, the subcommand is not called.
- The early exit on `stop_hook_active` stays first, and the guard on `LOOM_SESSION_ID` and
  `LOOM_SCRATCH_DIR` stays. The script still always exits 0.
- Rewrite the header comment: the hook has two reasons to block, each bounded (once per ticket;
  for an idle stop, the bound the subcommand keeps), and the revisit condition stays.

## Tests (write them; the orchestrator runs them)

In `loom-hooks/tests/loom-relay-stop.sh`, in the file's existing case style and with its stub
conventions (a stub `loom` placed where the hook resolves the binary, recording that it ran):

- the stub prints a block object: the hook prints one block object with the same reason;
- the stub prints `not json`: the hook prints nothing and exits 0;
- the stub prints a block object with an empty reason: nothing is printed;
- the stub exits 3: nothing is printed, the hook exits 0;
- a stranded control ticket is present: the hook prints the ticket reason and the stub never
  ran;
- `stop_hook_active: true`: the stub never ran;
- `LOOM_STAGE_ID` unset: the stub never ran.

The existing cases keep passing unchanged.

## Proof

`bash -n loom-hooks/loom-relay-stop.sh` and `bash -n loom-hooks/tests/loom-relay-stop.sh`.

## Traps

- Bash 3.2 (macOS): no `mapfile`, no associative arrays; guard an empty array before expanding
  it under `set -u`.
- The hook is embedded in the binary and its name is registered for the Stop event
  (`loom/src/fs/permissions/hooks/config.rs`); do not rename it and add no new hook file.
- `commit-guard.sh` went advisory because a Stop hook that blocked on every stop ended sessions
  that were waiting for subagents. This hook must add no block of its own beyond relaying the
  subcommand's decision.
