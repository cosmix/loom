# W2 (bridge-core): spool, permission hook, state machine, input checks

Read `doc/plans/briefs/plan-session-bridge/common.md` first ("Hooks registered per session",
"Spool", "Permission flow", "State machine", "Input"). You run in wave 3, beside W1 and W3, after codex units C2 (model) and C3 (spool) wrote their files
(`codex-units.md` in this directory). You own the permission hook, the state machine,
`hook_cli.rs` and the input checks of step 5 (`input.rs`: the pane-delivery boundary, kept on
Claude).

## Files you own (write only these)

- `loom/src/plan_session/permission.rs`, `loom/src/plan_session/permission_tests.rs`
- `loom/src/plan_session/state.rs`, `loom/src/plan_session/state_tests.rs`
- `loom/src/plan_session/input.rs` (seeded; tests inline in a `#[cfg(test)] mod tests`)
- `loom/src/plan_session/hook_cli.rs` (plan 6's file: add the nine arms; keep plan 6's
  `PreToolUse` arm and its behaviour byte-for-byte)

## Steps

1. `spool.rs` is codex unit C3 (`codex-units.md`): do not write it. You call its
   `append_event`, `SpoolEvent` and `read_payload` (1 MiB + 1 bound) from `hook_cli.rs` and
   `permission.rs`. Replay after a spool reset is the bridge's concern (W4), not yours.
2. `hook_cli.rs`: `"SessionStart" | "UserPromptSubmit" | "Notification" | "Stop" | "SubagentStart" |
   "SubagentStop" | "PreCompact" | "SessionEnd"` -> read payload; when `hook_event_name` is present
   and differs from the argument, return 0 with no output and no spool line; else `append_event`;
   print nothing; return 0 whatever happens. `"PermissionRequest"` -> `permission::run_hook`.
3. `permission.rs`: `pub fn run_hook(session_dir: &Path, stdin: impl Read, stdout: impl Write)`
   (hook side) and `write_answer`, `Answer`, `Behavior`, `pub const HOOK_WAIT: Duration` (3595 s),
   `pub const POLL: Duration` (200 ms). For tests, `pub(crate) fn run_hook_with(session_dir, stdin, stdout, wait: Duration, poll: Duration)`.
   The deny reasons and output shapes are in common.md; use `crate::plan_session::guard::load_policy`
   for the kind. `DenyReason` is W1's `model::DenyReason`.
4. `state.rs`: `StateMachine`, `Input`, `apply` exactly as common.md. Keep the last idle event's
   `received_at`, the last main-record `at`, and the running-subagent count (never below 0).
   A pure module: no clock reads, no I/O.
5. `input.rs`: `sanitize`, `check_send`, `check_delivered`, `InputRefusal` as common.md.
6. Tests: spool append from four threads at once yields four whole lines; a mismatched event name
   appends nothing; a 1 MiB + 1 payload appends nothing and exits 0; a missing session directory (append fails)
   exits 0 with no output for every spool event; `SpoolReader` returns only
   complete lines (write half a line, read, finish it, read); `PermissionRequest` returns allow
   when an allow answer is written, deny with the note as message for a holder deny, deny
   `hook_error` when `answers/` is removed mid-wait (within 1 s with a 50 ms poll), deny at the
   (short) wait expiry, deny at once for a summary-kind `guard.json` with the spool line marked
   `auto_denied`; `write_answer` refuses `../x`, `a/b` and a non-uuid id; state: one test per
   transition of section 07's table (Starting on spawn; Idle on SessionStart startup, on a main
   Stop, on an interrupt record, on idle_prompt; Working on UserPromptSubmit and on a newer main
   record; Permission on PermissionRequest and back on PermissionAnswered; Ending on EndRequested;
   Ended on SessionEnd and on ProcessGone while Ending; Failed on SpawnFailed and on ProcessGone
   outside Ending; SessionStart with source compact changes nothing), the contradiction cases (Stop then a later main record -> Working;
   `idle_prompt` received before a later record -> Working; a subagent record after Stop ->
   stays Idle; `SubagentStop` with none running -> count stays 0), Ending is sticky (with `activity()` tracking Idle/Working underneath), two pending requests
   keep Permission until both ids are removed (answer, expiry, `RequestResolved` of that id only), an answer for an unknown id
   changes nothing (an Idle machine stays Idle), a Stop, an interrupt or a main record 2 s after
   the newest request clears the set, `SelfStartedTurn` -> Working, `CompactDelivered` then
   `SessionStart { compact }` with no Stop -> activity Idle while a `compact` source without
   `CompactDelivered` changes nothing, the subagent count resets on an interrupt, on
   `AgentsKilled` and on `SessionStart`, ProcessGone
   outside Ending -> Failed; sanitize strips `\x1b[201~`, `\u{9b}`, `\x7f`, `\x00`, keeps `\n`,
   `\t` and non-ASCII letters, turns `\r\n` into `\n`.

## Proof

`cargo test --lib plan_session::spool`, `cargo test --lib plan_session::permission`,
`cargo test --lib plan_session::state`, `cargo test --lib plan_session::input`,
`cargo test --lib tokens::` and `cargo clippy --all-targets -- -D warnings` from `loom/`. Report
anything surprising (and `loom memory note` it), in particular any hook payload field whose name
differed from common.md when you checked Claude Code's hook documentation (fetch nothing; say what
you could not confirm).
